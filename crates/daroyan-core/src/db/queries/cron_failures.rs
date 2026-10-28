use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};

use crate::db::Database;
use crate::db::entities::cron_failures::{ActiveModel, Column, Entity, Model};
use crate::db::ids::CronId;

impl Database {
    pub async fn list_cron_failures_since(
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

    pub async fn create_cron_failure(
        &self,
        id: String,
        cron_id: &str,
        kind: &str,
        cause: String,
        started_at: DateTime<Utc>,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id),
            cron_id: Set(cron_id.to_string()),
            kind: Set(kind.to_string()),
            cause: Set(cause),
            started_at: Set(started_at),
            resolved_at: Set(None),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_open_cron_failures(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::ResolvedAt.is_null())
            .all(&self.conn)
            .await
    }

    pub async fn list_open_failures_of_cron(
        &self,
        cron_id: &str,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::CronId.eq(cron_id))
            .filter(Column::ResolvedAt.is_null())
            .all(&self.conn)
            .await
    }

    pub async fn list_cron_failures(
        &self,
        cron_id: &CronId,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::CronId.eq(cron_id.as_str()))
            .order_by_desc(Column::StartedAt)
            .limit(limit)
            .all(&self.conn)
            .await
    }

    pub async fn resolve_cron_failure(
        &self,
        id: &str,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            resolved_at: Set(Some(at)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }
}
