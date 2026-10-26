pub mod probe;

pub use reqwest::Url;

use std::collections::HashMap;
use std::time::Duration;

use rustls::ClientConfig;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::analytics::Analytics;
use crate::analytics::uptime_results::{append_result, count_recent};
use crate::db::Database;
use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::db::ids::UptimeCheckId;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};
use crate::uptime::probe::{Probe, STATUS_DOWN, build_tls_config, run_probe};

const TICK: Duration = Duration::from_secs(1);
const MANUAL_RUNS: usize = 8;
pub const FAILURES_TO_OPEN: u32 = 2;

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
        let probe = run_probe(&self.tls, check).await;
        append_result(&self.analytics, check.id.clone(), probe.clone()).await?;
        self.track_outage(check, &probe).await?;
        Ok(probe)
    }

    async fn track_outage(&self, check: &UptimeCheck, probe: &Probe) -> Result<()> {
        let check_id = UptimeCheckId::from(check.id.as_str());
        let open = self.database.find_open_uptime_outage(&check_id).await?;
        match (open, probe.status == STATUS_DOWN) {
            (Some(outage), false) => {
                self.database.resolve_uptime_outage(&outage.id, probe.at).await?;
                tracing::info!(check_id = %check.id, "uptime check recovered");
            }
            (None, true) => {
                let downs = count_recent(
                    &self.analytics,
                    check.id.clone(),
                    STATUS_DOWN,
                    FAILURES_TO_OPEN,
                )
                .await?;
                if downs < FAILURES_TO_OPEN {
                    return Ok(());
                }
                let cause =
                    probe.error.clone().unwrap_or_else(|| "Check failed".to_string());
                let started_at =
                    probe.at - chrono::Duration::seconds(i64::from(check.interval_secs));
                self.database
                    .create_uptime_outage(
                        new_id(Prefix::UptimeOutage),
                        &check_id,
                        STATUS_DOWN,
                        cause.clone(),
                        started_at,
                    )
                    .await?;
                tracing::info!(check_id = %check.id, cause = %cause, "uptime check down");
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn format_ms(ms: u32) -> String {
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let secs = format!("{:.1}", f64::from(ms) / 1000.0);
    format!("{} s", secs.trim_end_matches(".0"))
}

pub fn spawn_uptime_runner(uptime: Uptime) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        let mut next_runs: HashMap<String, Instant> = HashMap::new();
        loop {
            tick.tick().await;
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
                let interval =
                    Duration::from_secs(u64::try_from(check.interval_secs).unwrap_or(60));
                let next = next_runs.entry(check.id.clone()).or_insert(now);
                if *next > now {
                    continue;
                }
                *next = now + interval;
                let uptime = uptime.clone();
                tokio::spawn(async move {
                    if let Err(error) = uptime.run_check(&check).await {
                        tracing::error!(check_id = %check.id, error = %error, "run an uptime check");
                    }
                });
            }
        }
    })
}
