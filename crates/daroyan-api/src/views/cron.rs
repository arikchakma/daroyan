use chrono::{DateTime, Utc};
use daroyan_core::analytics::cron_runs::{CronRunBar, CronSummary};
use daroyan_core::crons::schedule::{Schedule, find_due};
use daroyan_core::crons::{FAILED_RUN, MISSED_RUN};
use daroyan_core::db::entities::cron_failures::Model as CronFailure;
use daroyan_core::db::entities::crons::Model as CronModel;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Cron)]
pub struct CronView {
    pub id: String,
    pub name: String,
    pub ping_key: String,
    #[schema(value_type = String, example = "cron")]
    pub schedule_kind: String,
    pub expression: Option<String>,
    pub interval_secs: Option<i32>,
    pub time_zone: String,
    pub grace_secs: i32,
    pub max_duration_secs: Option<i32>,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub command: Option<String>,
    pub paused: bool,
    #[schema(value_type = String, example = "ok")]
    pub status: String,
    pub last_ping_at: Option<i64>,
    pub run_started_at: Option<i64>,
    pub next_expected_at: Option<i64>,
    pub last_duration_ms: Option<u32>,
    pub runs: Vec<CronRunBar>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

pub fn read_status(
    cron: &CronModel,
    open: &[&CronFailure],
    is_running: bool,
) -> &'static str {
    let is_open = |kind: &str| open.iter().any(|failure| failure.kind == kind);
    if cron.paused {
        "paused"
    } else if is_open(FAILED_RUN) {
        "failed"
    } else if is_open(MISSED_RUN) {
        "late"
    } else if is_running {
        "running"
    } else if cron.last_ping_at.is_none() {
        "new"
    } else {
        "ok"
    }
}

impl CronView {
    pub fn new(
        cron: CronModel,
        summary: CronSummary,
        open: &[&CronFailure],
        server_name: Option<String>,
        running_since: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Self {
        let next_expected_at = Schedule::read(&cron).and_then(|schedule| {
            match (cron.last_ping_at, &schedule) {
                (None, Schedule::Cron { .. }) => schedule.next_after(now),
                (None, Schedule::Every(_)) => None,
                (Some(_), _) => find_due(&cron, &schedule),
            }
        });
        Self {
            status: read_status(&cron, open, running_since.is_some()).to_string(),
            last_ping_at: cron
                .last_ping_at
                .max(running_since)
                .map(|at| at.timestamp_millis()),
            run_started_at: running_since.map(|at| at.timestamp_millis()),
            next_expected_at: next_expected_at.map(|at| at.timestamp_millis()),
            last_duration_ms: summary.last_duration_ms,
            runs: summary.bars,
            id: cron.id,
            name: cron.name,
            ping_key: cron.ping_key,
            schedule_kind: cron.schedule_kind,
            expression: cron.expression,
            interval_secs: cron.interval_secs,
            time_zone: cron.time_zone,
            grace_secs: cron.grace_secs,
            max_duration_secs: cron.max_duration_secs,
            server_id: cron.server_id,
            server_name,
            command: cron.command,
            paused: cron.paused,
            created_at: cron.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = CronFailure)]
pub struct CronFailureView {
    pub id: String,
    #[schema(value_type = String, example = "missed")]
    pub kind: String,
    pub cause: String,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl From<CronFailure> for CronFailureView {
    fn from(failure: CronFailure) -> Self {
        Self {
            id: failure.id,
            kind: failure.kind,
            cause: failure.cause,
            started_at: failure.started_at,
            resolved_at: failure.resolved_at,
        }
    }
}
