use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

use crate::db::Database;
use crate::db::entities::alert_deliveries::{ActiveModel, Column, Entity, Model};
use crate::db::ids::AlertRuleId;

pub struct NewAlertDelivery {
    pub id: String,
    pub rule_id: String,
    pub alert_id: Option<String>,
    pub integration_id: String,
    pub integration_name: String,
    pub message: String,
    pub error: Option<String>,
    pub sent_at: DateTime<Utc>,
}

impl Database {
    pub async fn create_alert_delivery(
        &self,
        delivery: NewAlertDelivery,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(delivery.id),
            rule_id: Set(delivery.rule_id),
            alert_id: Set(delivery.alert_id),
            integration_id: Set(delivery.integration_id),
            integration_name: Set(delivery.integration_name),
            message: Set(delivery.message),
            status: Set(
                if delivery.error.is_none() { "sent" } else { "failed" }.to_string()
            ),
            error: Set(delivery.error),
            sent_at: Set(delivery.sent_at),
        }
        .insert(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn list_alert_deliveries(
        &self,
        rule_id: &AlertRuleId,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::RuleId.eq(rule_id.as_str()))
            .order_by_desc(Column::SentAt)
            .order_by_desc(Column::Id)
            .limit(limit)
            .all(&self.conn)
            .await
    }
}
