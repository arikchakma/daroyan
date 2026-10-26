pub mod schedule;

use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use tokio::task::JoinHandle;

use crate::alerts::rules::format_duration;
use crate::analytics::Analytics;
use crate::analytics::cron_runs::{CronRun, append_runs};
use crate::crons::schedule::{Schedule, find_due};
use crate::db::Database;
use crate::db::entities::cron_failures::Model as CronFailure;
use crate::db::entities::cron_open_runs::Model as OpenRun;
use crate::db::entities::crons::Model as CronModel;
use crate::db::queries::crons::CronRunState;
use crate::error::Result;
use crate::id::{Prefix, new_id};

pub const STATUS_OK: &str = "ok";
pub const STATUS_LATE: &str = "late";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_MISSED: &str = "missed";
pub const MISSED_RUN: &str = "missed";
pub const FAILED_RUN: &str = "failed";
pub const MAX_OUTPUT_BYTES: usize = 10_000;
const MAX_CAUSE_CHARS: usize = 200;
const MAX_MISSED_PER_TICK: usize = 100;
const TICK: StdDuration = StdDuration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PingKind {
    Start,
    Success,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ping {
    pub kind: PingKind,
    pub exit_code: Option<u8>,
    pub run_id: Option<String>,
    pub output: Option<String>,
}

pub fn read_ping_action(action: Option<&str>) -> Option<(PingKind, Option<u8>)> {
    match action {
        None => Some((PingKind::Success, None)),
        Some("start") => Some((PingKind::Start, None)),
        Some("fail") => Some((PingKind::Fail, None)),
        Some(code) => match code.parse::<u8>().ok()? {
            0 => Some((PingKind::Success, Some(0))),
            code => Some((PingKind::Fail, Some(code))),
        },
    }
}

pub fn read_output(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut end = text.len().min(MAX_OUTPUT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(text[..end].to_string())
}

fn describe_failure(exit_code: Option<u8>, output: Option<&str>) -> String {
    let last_line = output
        .and_then(|output| output.lines().rev().find(|line| !line.trim().is_empty()))
        .map(|line| line.trim().chars().take(MAX_CAUSE_CHARS).collect::<String>());
    match (exit_code, last_line) {
        (Some(code), Some(line)) => format!("Exit code {code}: {line}"),
        (Some(code), None) => format!("Exit code {code}"),
        (None, Some(line)) => line,
        (None, None) => "The job reported a failure".to_string(),
    }
}

fn to_secs(duration: Duration) -> u32 {
    u32::try_from(duration.num_seconds()).unwrap_or(u32::MAX)
}

#[derive(Clone)]
pub struct Crons {
    pub database: Database,
    pub analytics: Analytics,
}

impl Crons {
    pub fn new(database: Database, analytics: Analytics) -> Self {
        Self { database, analytics }
    }

    pub async fn record_ping(
        &self,
        cron: &CronModel,
        ping: Ping,
        now: DateTime<Utc>,
    ) -> Result<()> {
        if ping.kind == PingKind::Start {
            self.database
                .open_cron_run(&cron.id, ping.run_id.as_deref().unwrap_or_default(), now)
                .await?;
            return Ok(());
        }
        let started_at = self
            .database
            .take_cron_run(&cron.id, ping.run_id.as_deref())
            .await?
            .map(|run| run.started_at);
        let began_at = started_at.unwrap_or(now);
        let scheduled_at = Schedule::read(cron)
            .and_then(|schedule| schedule.find_slot(began_at, cron.last_ping_at));
        let open = self.database.list_open_failures_of_cron(&cron.id).await?;
        let was_missed = open.iter().any(|failure| failure.kind == MISSED_RUN);
        let failed = ping.kind == PingKind::Fail;
        let status = match (failed, was_missed) {
            (true, _) => STATUS_FAILED,
            (false, true) => STATUS_LATE,
            (false, false) => STATUS_OK,
        };
        let cause = describe_failure(ping.exit_code, ping.output.as_deref());
        append_runs(
            &self.analytics,
            cron.id.clone(),
            vec![CronRun {
                scheduled_at: scheduled_at.map(|at| at.timestamp_millis()),
                started_at: started_at.map(|at| at.timestamp_millis()),
                finished_at: now.timestamp_millis(),
                status: status.to_string(),
                exit_code: ping.exit_code,
                duration_ms: started_at
                    .and_then(|at| u32::try_from((now - at).num_milliseconds()).ok()),
                output: ping.output,
            }],
        )
        .await?;
        self.database
            .save_cron_run_state(
                &cron.id,
                CronRunState {
                    last_ping_at: Some(Some(now)),
                    ..Default::default()
                },
            )
            .await?;
        if cron.paused {
            return Ok(());
        }
        if failed {
            self.open_failure(cron, &open, cause, now).await
        } else {
            self.resolve(&open, |_| true, now).await
        }
    }

    async fn resolve(
        &self,
        open: &[CronFailure],
        matches: impl Fn(&CronFailure) -> bool,
        now: DateTime<Utc>,
    ) -> Result<()> {
        for failure in open.iter().filter(|failure| matches(failure)) {
            self.database.resolve_cron_failure(&failure.id, now).await?;
            tracing::info!(cron_id = %failure.cron_id, kind = %failure.kind, "cron recovered");
        }
        Ok(())
    }

    async fn open_failure(
        &self,
        cron: &CronModel,
        open: &[CronFailure],
        cause: String,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.resolve(open, |failure| failure.kind == MISSED_RUN, now).await?;
        if open.iter().any(|failure| failure.kind == FAILED_RUN) {
            return Ok(());
        }
        tracing::info!(cron_id = %cron.id, cause = %cause, "cron failed");
        self.database
            .create_cron_failure(
                new_id(Prefix::CronFailure),
                &cron.id,
                FAILED_RUN,
                cause,
                now,
            )
            .await?;
        Ok(())
    }

    pub async fn check_crons(&self, now: DateTime<Utc>) -> Result<()> {
        let open_runs = self.database.list_open_cron_runs().await?;
        for cron in self.database.list_crons().await? {
            if cron.paused {
                continue;
            }
            let runs: Vec<&OpenRun> =
                open_runs.iter().filter(|run| run.cron_id == cron.id).collect();
            if let Err(error) = self.check_cron(&cron, &runs, now).await {
                tracing::error!(cron_id = %cron.id, error = %error, "check a cron");
            }
        }
        Ok(())
    }

    async fn check_cron(
        &self,
        cron: &CronModel,
        runs: &[&OpenRun],
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Some(schedule) = Schedule::read(cron) else {
            return Ok(());
        };
        if !runs.is_empty() {
            return self.check_timeouts(cron, &schedule, runs, now).await;
        }
        let grace = Duration::seconds(i64::from(cron.grace_secs));
        let mut slots = Vec::new();
        let mut due = find_due(cron, &schedule);
        while let Some(slot) = due.filter(|slot| *slot + grace <= now) {
            slots.push(slot);
            if slots.len() == MAX_MISSED_PER_TICK {
                break;
            }
            due = schedule.next_after(slot);
        }
        let (Some(first), Some(last)) = (slots.first().copied(), slots.last().copied())
        else {
            return Ok(());
        };
        append_runs(
            &self.analytics,
            cron.id.clone(),
            slots
                .iter()
                .map(|slot| CronRun {
                    scheduled_at: Some(slot.timestamp_millis()),
                    started_at: None,
                    finished_at: (*slot + grace).timestamp_millis(),
                    status: STATUS_MISSED.to_string(),
                    exit_code: None,
                    duration_ms: None,
                    output: None,
                })
                .collect(),
        )
        .await?;
        self.database
            .save_cron_run_state(
                &cron.id,
                CronRunState {
                    missed_slot_at: Some(Some(last)),
                    ..Default::default()
                },
            )
            .await?;
        let open = self.database.list_open_failures_of_cron(&cron.id).await?;
        if open.iter().any(|failure| failure.kind == MISSED_RUN) {
            return Ok(());
        }
        let cause = format!(
            "No ping within the {} grace period",
            format_duration(to_secs(grace))
        );
        tracing::info!(cron_id = %cron.id, cause = %cause, "cron missed a run");
        self.database
            .create_cron_failure(
                new_id(Prefix::CronFailure),
                &cron.id,
                MISSED_RUN,
                cause,
                first + grace,
            )
            .await?;
        Ok(())
    }

    async fn check_timeouts(
        &self,
        cron: &CronModel,
        schedule: &Schedule,
        runs: &[&OpenRun],
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Some(max) =
            cron.max_duration_secs.map(|secs| Duration::seconds(i64::from(secs)))
        else {
            return Ok(());
        };
        let expired: Vec<&&OpenRun> =
            runs.iter().filter(|run| now - run.started_at >= max).collect();
        if expired.is_empty() {
            return Ok(());
        }
        let cause = format!("Still running after {}", format_duration(to_secs(max)));
        append_runs(
            &self.analytics,
            cron.id.clone(),
            expired
                .iter()
                .map(|run| CronRun {
                    scheduled_at: schedule
                        .find_slot(run.started_at, cron.last_ping_at)
                        .map(|at| at.timestamp_millis()),
                    started_at: Some(run.started_at.timestamp_millis()),
                    finished_at: now.timestamp_millis(),
                    status: STATUS_FAILED.to_string(),
                    exit_code: None,
                    duration_ms: u32::try_from((now - run.started_at).num_milliseconds())
                        .ok(),
                    output: Some(cause.clone()),
                })
                .collect(),
        )
        .await?;
        for run in &expired {
            self.database.delete_cron_run(&run.cron_id, &run.run_id).await?;
        }
        self.database
            .save_cron_run_state(
                &cron.id,
                CronRunState {
                    missed_slot_at: Some(Some(now)),
                    ..Default::default()
                },
            )
            .await?;
        let open = self.database.list_open_failures_of_cron(&cron.id).await?;
        self.open_failure(cron, &open, cause, now).await
    }
}

pub fn spawn_cron_runner(crons: Crons) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            if let Err(error) = crons.check_crons(Utc::now()).await {
                tracing::error!(error = %error, "check crons");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ping_actions() {
        assert_eq!(read_ping_action(None), Some((PingKind::Success, None)));
        assert_eq!(read_ping_action(Some("start")), Some((PingKind::Start, None)));
        assert_eq!(read_ping_action(Some("fail")), Some((PingKind::Fail, None)));
        assert_eq!(read_ping_action(Some("0")), Some((PingKind::Success, Some(0))));
        assert_eq!(read_ping_action(Some("1")), Some((PingKind::Fail, Some(1))));
        assert_eq!(read_ping_action(Some("255")), Some((PingKind::Fail, Some(255))));
        assert_eq!(read_ping_action(Some("256")), None);
        assert_eq!(read_ping_action(Some("log")), None);
    }

    #[test]
    fn keeps_output_short_and_whole() {
        assert_eq!(read_output(b"  \n"), None);
        assert_eq!(read_output(b" done\n").as_deref(), Some("done"));
        let long = "é".repeat(MAX_OUTPUT_BYTES);
        let output = read_output(long.as_bytes()).unwrap();
        assert_eq!(output.len(), MAX_OUTPUT_BYTES);
        assert!(output.chars().all(|char| char == 'é'));
    }

    #[test]
    fn describes_failures_from_the_last_output_line() {
        assert_eq!(
            describe_failure(Some(1), Some("connecting\npsql: connection refused\n\n")),
            "Exit code 1: psql: connection refused"
        );
        assert_eq!(describe_failure(Some(2), None), "Exit code 2");
        assert_eq!(describe_failure(None, Some("boom")), "boom");
        assert_eq!(describe_failure(None, None), "The job reported a failure");
    }
}
