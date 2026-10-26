use std::collections::HashSet;

use chrono::{DateTime, Utc};
use daroyan_core::analytics::uptime_results::{CheckSummary, DayBar};
use daroyan_core::db::entities::uptime_checks::Model as UptimeCheck;
use daroyan_core::db::entities::uptime_outages::Model as UptimeOutage;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = UptimeCheck)]
pub struct UptimeCheckView {
    pub id: String,
    pub name: String,
    #[schema(value_type = String, example = "http")]
    pub kind: String,
    pub target: String,
    pub method: String,
    pub expected_status: Option<i32>,
    pub body_contains: Option<String>,
    pub timeout_secs: i32,
    pub interval_secs: i32,
    pub paused: bool,
    #[schema(value_type = String, example = "up")]
    pub status: String,
    pub last_checked_at: Option<i64>,
    pub last_response_ms: Option<u32>,
    pub uptime: Option<f64>,
    pub days: Vec<DayBar>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

impl UptimeCheckView {
    pub fn new(
        check: UptimeCheck,
        summary: CheckSummary,
        alerting: &HashSet<String>,
    ) -> Self {
        let status = match summary.status {
            _ if check.paused => "paused".to_string(),
            Some(status) if status == "up" && alerting.contains(&check.id) => {
                "degraded".to_string()
            }
            Some(status) => status,
            None => "pending".to_string(),
        };
        Self {
            id: check.id,
            name: check.name,
            kind: check.kind,
            target: check.target,
            method: check.method,
            expected_status: check.expected_status,
            body_contains: check.body_contains,
            timeout_secs: check.timeout_secs,
            interval_secs: check.interval_secs,
            paused: check.paused,
            status,
            last_checked_at: summary.checked_at,
            last_response_ms: summary.response_ms,
            uptime: summary.uptime,
            days: summary.days,
            created_at: check.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = UptimeOutage)]
pub struct UptimeOutageView {
    pub id: String,
    #[schema(value_type = String, example = "down")]
    pub kind: String,
    pub cause: String,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl From<UptimeOutage> for UptimeOutageView {
    fn from(outage: UptimeOutage) -> Self {
        Self {
            id: outage.id,
            kind: outage.kind,
            cause: outage.cause,
            started_at: outage.started_at,
            resolved_at: outage.resolved_at,
        }
    }
}
