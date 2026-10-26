use chrono::{DateTime, Utc};
use daroyan_core::db::entities::alerts::Model as Alert;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Alert)]
pub struct AlertView {
    pub id: String,
    #[schema(value_type = String, example = "firing")]
    pub status: String,
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: Option<String>,
    pub check_id: Option<String>,
    pub cron_id: Option<String>,
    pub target_name: String,
    pub subject: String,
    pub value: String,
    #[schema(value_type = String, example = "critical")]
    pub severity: String,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl AlertView {
    pub fn new(alert: Alert, rule_name: String, target_name: String) -> Self {
        Self {
            id: alert.id,
            status: alert.status,
            rule_id: alert.rule_id,
            rule_name,
            server_id: alert.server_id,
            check_id: alert.check_id,
            cron_id: alert.cron_id,
            target_name,
            subject: alert.subject,
            value: alert.value,
            severity: alert.severity,
            started_at: alert.started_at,
            resolved_at: alert.resolved_at,
        }
    }
}
