use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};

use crate::db::Database;
use crate::db::entities::alerts::{ActiveModel, Column, Entity, Model};
use crate::db::ids::{AlertId, AlertRuleId};

pub const STATUS_FIRING: &str = "firing";
pub const STATUS_RESOLVED: &str = "resolved";

pub struct NewAlert {
    pub id: String,
    pub rule_id: String,
    pub server_id: Option<String>,
    pub check_id: Option<String>,
    pub cron_id: Option<String>,
    pub subject: String,
    pub value: String,
    pub severity: String,
    pub started_at: DateTime<Utc>,
}

impl Database {
    pub async fn list_alerts_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(
                Condition::any()
                    .add(Column::StartedAt.gte(since))
                    .add(Column::ResolvedAt.gte(since))
                    .add(Column::ResolvedAt.is_null()),
            )
            .order_by_desc(Column::StartedAt)
            .all(&self.conn)
            .await
    }

    pub async fn create_alert(&self, alert: NewAlert) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(alert.id),
            rule_id: Set(alert.rule_id),
            server_id: Set(alert.server_id),
            check_id: Set(alert.check_id),
            cron_id: Set(alert.cron_id),
            subject: Set(alert.subject),
            value: Set(alert.value),
            severity: Set(alert.severity),
            status: Set(STATUS_FIRING.to_string()),
            started_at: Set(alert.started_at),
            resolved_at: Set(None),
            last_notified_at: Set(Some(Utc::now())),
            reminders_sent: Set(0),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_alerts(
        &self,
        status: Option<&str>,
        rule_id: Option<&AlertRuleId>,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        let mut query = Entity::find();
        if let Some(status) = status {
            query = query.filter(Column::Status.eq(status));
        }
        if let Some(rule_id) = rule_id {
            query = query.filter(Column::RuleId.eq(rule_id.as_str()));
        }
        query
            .order_by_desc(Column::StartedAt)
            .order_by_desc(Column::Id)
            .limit(limit)
            .all(&self.conn)
            .await
    }

    pub async fn find_alert(&self, id: &AlertId) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn list_firing_alerts(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::Status.eq(STATUS_FIRING))
            .all(&self.conn)
            .await
    }

    pub async fn resolve_alert(
        &self,
        id: &AlertId,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            status: Set(STATUS_RESOLVED.to_string()),
            resolved_at: Set(Some(at)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn escalate_alert(
        &self,
        id: &AlertId,
        severity: &str,
        value: String,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            severity: Set(severity.to_string()),
            value: Set(value),
            last_notified_at: Set(Some(at)),
            reminders_sent: Set(0),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn mark_alert_reminded(
        &self,
        id: &AlertId,
        reminders_sent: i32,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            reminders_sent: Set(reminders_sent),
            last_notified_at: Set(Some(at)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }
}
