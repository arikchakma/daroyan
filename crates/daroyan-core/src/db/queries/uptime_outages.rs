use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

use crate::db::Database;
use crate::db::entities::uptime_outages::{ActiveModel, Column, Entity, Model};
use crate::db::ids::UptimeCheckId;

impl Database {
    pub async fn create_uptime_outage(
        &self,
        id: String,
        check_id: &UptimeCheckId,
        kind: &str,
        cause: String,
        started_at: DateTime<Utc>,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id),
            check_id: Set(check_id.to_string()),
            kind: Set(kind.to_string()),
            cause: Set(cause),
            started_at: Set(started_at),
            resolved_at: Set(None),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn find_open_uptime_outage(
        &self,
        check_id: &UptimeCheckId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::CheckId.eq(check_id.as_str()))
            .filter(Column::ResolvedAt.is_null())
            .one(&self.conn)
            .await
    }

    pub async fn list_open_uptime_outages(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::ResolvedAt.is_null())
            .all(&self.conn)
            .await
    }

    pub async fn list_uptime_outages(
        &self,
        check_id: &UptimeCheckId,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::CheckId.eq(check_id.as_str()))
            .order_by_desc(Column::StartedAt)
            .limit(limit)
            .all(&self.conn)
            .await
    }

    pub async fn resolve_uptime_outage(
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
