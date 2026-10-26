use chrono::{DateTime, Utc};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
};

use crate::db::Database;
use crate::db::entities::crons::{ActiveModel, Column, Entity, Model};
use crate::db::ids::CronId;

pub struct CronFields {
    pub name: String,
    pub schedule_kind: String,
    pub expression: Option<String>,
    pub interval_secs: Option<i32>,
    pub time_zone: String,
    pub grace_secs: i32,
    pub max_duration_secs: Option<i32>,
    pub server_id: Option<String>,
    pub command: Option<String>,
    pub paused: bool,
}

#[derive(Default)]
pub struct CronRunState {
    pub last_ping_at: Option<Option<DateTime<Utc>>>,
    pub missed_slot_at: Option<Option<DateTime<Utc>>>,
}

fn to_value<T: Into<sea_orm::Value>>(value: Option<T>) -> sea_orm::ActiveValue<T> {
    value.map_or(NotSet, Set)
}

impl Database {
    pub async fn create_cron(
        &self,
        id: String,
        ping_key: String,
        fields: CronFields,
    ) -> Result<Model, DbErr> {
        let now = Utc::now();
        let mut model = to_active_model(id, fields);
        model.ping_key = Set(ping_key);
        model.created_at = Set(now);
        model.last_ping_at = Set(None);
        model.missed_slot_at = Set(None);
        model.insert(&self.conn).await
    }

    pub async fn list_crons(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn find_cron(&self, id: &CronId) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn find_cron_by_key(&self, ping_key: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::PingKey.eq(ping_key))
            .one(&self.conn)
            .await
    }

    pub async fn update_cron(
        &self,
        id: &CronId,
        fields: CronFields,
        missed_slot_at: Option<DateTime<Utc>>,
    ) -> Result<Model, DbErr> {
        let mut model = to_active_model(id.to_string(), fields);
        model.missed_slot_at = to_value(missed_slot_at.map(Some));
        model.update(&self.conn).await
    }

    pub async fn save_cron_run_state(
        &self,
        id: &str,
        state: CronRunState,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            last_ping_at: to_value(state.last_ping_at),
            missed_slot_at: to_value(state.missed_slot_at),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn delete_cron(&self, id: &CronId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}

fn to_active_model(id: String, fields: CronFields) -> ActiveModel {
    ActiveModel {
        id: Set(id),
        name: Set(fields.name),
        schedule_kind: Set(fields.schedule_kind),
        expression: Set(fields.expression),
        interval_secs: Set(fields.interval_secs),
        time_zone: Set(fields.time_zone),
        grace_secs: Set(fields.grace_secs),
        max_duration_secs: Set(fields.max_duration_secs),
        server_id: Set(fields.server_id),
        command: Set(fields.command),
        paused: Set(fields.paused),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
}
