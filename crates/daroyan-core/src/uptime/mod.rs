pub mod probe;

pub use reqwest::Url;

use std::collections::HashMap;
use std::time::Duration;

use chrono::DateTime;
use rustls::ClientConfig;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::analytics::Analytics;
use crate::analytics::uptime_results::{ResultItem, append_result, list_recent_results};
use crate::db::Database;
use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::db::ids::UptimeCheckId;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};
use crate::uptime::probe::{
    Probe, STATUS_CONFIRMING, STATUS_DOWN, STATUS_UP, build_tls_config, run_probe,
};

const TICK: Duration = Duration::from_secs(1);
const MANUAL_RUNS: usize = 8;

#[derive(Clone)]
pub struct Uptime {
    pub database: Database,
    pub analytics: Analytics,
    pub tls: Arc<ClientConfig>,
    manual_runs: Arc<Semaphore>,
}

impl Uptime {
    pub fn new(database: Database, analytics: Analytics) -> Self {
        Self {
            database,
            analytics,
            tls: build_tls_config(),
            manual_runs: Arc::new(Semaphore::new(MANUAL_RUNS)),
        }
    }

    pub async fn run_check_now(&self, check: &UptimeCheck) -> Result<Probe> {
        let _permit = self.manual_runs.try_acquire().map_err(|_| {
            Error::new(
                ErrorName::RateLimitExceeded,
                "Too many checks are running. Try again soon",
            )
        })?;
        self.run_check(check).await
    }

    pub async fn run_check(&self, check: &UptimeCheck) -> Result<Probe> {
        let mut probe = run_probe(&self.tls, check).await;
        let check_id = UptimeCheckId::from(check.id.as_str());
        let open = self.database.find_open_uptime_outage(&check_id).await?;
        let limit = check.failure_threshold.max(check.success_threshold);
        let recent = list_recent_results(
            &self.analytics,
            check.id.clone(),
            u32::try_from(limit).unwrap_or(1),
        )
        .await?;

        let is_up = probe.status == STATUS_UP;
        let streak: Vec<&ResultItem> = recent
            .iter()
            .take_while(|result| (result.status == STATUS_UP) == is_up)
            .collect();
        let count = i32::try_from(streak.len()).unwrap_or(i32::MAX).saturating_add(1);
        let first_at = streak
            .last()
            .and_then(|result| DateTime::from_timestamp_millis(result.at))
            .unwrap_or(probe.at);

        match open {
            Some(outage) if is_up && count >= check.success_threshold => {
                self.database.resolve_uptime_outage(&outage.id, first_at).await?;
                tracing::info!(check_id = %check.id, "uptime check recovered");
            }
            None if !is_up && count < check.failure_threshold => {
                probe.status = STATUS_CONFIRMING;
            }
            None if !is_up => {
                let cause =
                    probe.error.clone().unwrap_or_else(|| "Check failed".to_string());
                self.database
                    .create_uptime_outage(
                        new_id(Prefix::UptimeOutage),
                        &check_id,
                        STATUS_DOWN,
                        cause.clone(),
                        first_at,
                    )
                    .await?;
                tracing::info!(check_id = %check.id, cause = %cause, "uptime check down");
            }
            _ => {}
        }

        append_result(&self.analytics, check.id.clone(), probe.clone()).await?;
        Ok(probe)
    }
}

pub fn format_ms(ms: u32) -> String {
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let secs = format!("{:.1}", f64::from(ms) / 1000.0);
    format!("{} s", secs.trim_end_matches(".0"))
}

fn to_duration(secs: i32) -> Duration {
    Duration::from_secs(u64::try_from(secs).unwrap_or(60))
}

pub fn spawn_uptime_runner(uptime: Uptime) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        let mut next_runs: HashMap<String, Instant> = HashMap::new();
        let (retries, mut retry_rx) = mpsc::unbounded_channel::<(String, Instant)>();
        loop {
            tick.tick().await;
            while let Ok((id, at)) = retry_rx.try_recv() {
                if let Some(next) = next_runs.get_mut(&id) {
                    *next = (*next).min(at);
                }
            }
            let checks = match uptime.database.list_uptime_checks().await {
                Ok(checks) => checks,
                Err(error) => {
                    tracing::error!(error = %error, "list uptime checks");
                    continue;
                }
            };
            let now = Instant::now();
            next_runs.retain(|id, _| {
                checks.iter().any(|check| &check.id == id && !check.paused)
            });
            for check in checks.into_iter().filter(|check| !check.paused) {
                let next = next_runs.entry(check.id.clone()).or_insert(now);
                if *next > now {
                    continue;
                }
                *next = now + to_duration(check.interval_secs);

                let uptime = uptime.clone();
                let retries = retries.clone();
                tokio::spawn(async move {
                    match uptime.run_check(&check).await {
                        Ok(probe) if probe.status == STATUS_CONFIRMING => {
                            let at = now + to_duration(check.retry_interval_secs);
                            retries.send((check.id, at)).ok();
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::error!(check_id = %check.id, error = %error, "run an uptime check");
                        }
                    }
                });
            }
        }
    })
}
