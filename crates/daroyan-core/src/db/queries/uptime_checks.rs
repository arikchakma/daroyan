use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DbErr, EntityTrait, QueryOrder};

use crate::db::Database;
use crate::db::entities::uptime_checks::{ActiveModel, Column, Entity, Model};
use crate::db::ids::UptimeCheckId;

pub struct UptimeCheckFields {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub method: String,
    pub expected_status: Option<i32>,
    pub body_contains: Option<String>,
    pub timeout_secs: i32,
    pub interval_secs: i32,
    pub paused: bool,
}

impl Database {
    pub async fn create_uptime_check(
        &self,
        id: String,
        fields: UptimeCheckFields,
    ) -> Result<Model, DbErr> {
        let now = Utc::now();
        to_active_model(id, fields, Set(now)).insert(&self.conn).await
    }

    pub async fn list_uptime_checks(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn find_uptime_check(
        &self,
        id: &UptimeCheckId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn update_uptime_check(
        &self,
        id: &UptimeCheckId,
        fields: UptimeCheckFields,
    ) -> Result<Model, DbErr> {
        to_active_model(id.to_string(), fields, sea_orm::ActiveValue::NotSet)
            .update(&self.conn)
            .await
    }

    pub async fn delete_uptime_check(&self, id: &UptimeCheckId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}

fn to_active_model(
    id: String,
    fields: UptimeCheckFields,
    created_at: sea_orm::ActiveValue<chrono::DateTime<Utc>>,
) -> ActiveModel {
    ActiveModel {
        id: Set(id),
        name: Set(fields.name),
        kind: Set(fields.kind),
        target: Set(fields.target),
        method: Set(fields.method),
        expected_status: Set(fields.expected_status),
        body_contains: Set(fields.body_contains),
        timeout_secs: Set(fields.timeout_secs),
        interval_secs: Set(fields.interval_secs),
        paused: Set(fields.paused),
        created_at,
        updated_at: Set(Utc::now()),
    }
}
