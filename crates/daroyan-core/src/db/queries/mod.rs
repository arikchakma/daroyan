pub mod alert_deliveries;
pub mod alert_mutes;
pub mod alert_rules;
pub mod alerts;
pub mod commands;
pub mod cron_failures;
pub mod cron_open_runs;
pub mod crons;
pub mod incidents;
pub mod ingest_tokens;
pub mod install_tokens;
pub mod integrations;
pub mod mcp_tokens;
pub mod members;
pub mod servers;
pub mod sessions;
pub mod status_pages;
pub mod uptime_checks;
pub mod uptime_outages;
pub mod users;

use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, Condition, DbErr, EntityTrait, QueryFilter, QueryOrder};

use crate::db::Database;

impl Database {
    pub async fn list_open_or_since<E: EntityTrait>(
        &self,
        started_at: E::Column,
        resolved_at: E::Column,
        since: DateTime<Utc>,
    ) -> Result<Vec<E::Model>, DbErr> {
        E::find()
            .filter(
                Condition::any()
                    .add(started_at.gte(since))
                    .add(resolved_at.gte(since))
                    .add(resolved_at.is_null()),
            )
            .order_by_desc(started_at)
            .all(&self.conn)
            .await
    }
}
