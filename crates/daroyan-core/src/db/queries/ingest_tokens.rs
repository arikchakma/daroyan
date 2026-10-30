use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::Database;
use crate::db::entities::ingest_tokens::{ActiveModel, Column, Entity, Model};
use crate::db::ids::IngestTokenId;

impl Database {
    pub async fn create_ingest_token(
        &self,
        id: String,
        name: String,
        token_hash: String,
        hint: String,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id),
            name: Set(name),
            token_hash: Set(token_hash),
            hint: Set(hint),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
            throttled_log_records: Set(0),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_ingest_tokens(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().order_by_desc(Column::CreatedAt).all(&self.conn).await
    }

    pub async fn find_ingest_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::TokenHash.eq(token_hash))
            .one(&self.conn)
            .await
    }

    pub async fn touch_ingest_token(
        &self,
        id: &str,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            last_used_at: Set(Some(at)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn add_throttled_log_records(
        &self,
        id: &str,
        count: u64,
    ) -> Result<(), DbErr> {
        Entity::update_many()
            .col_expr(
                Column::ThrottledLogRecords,
                Expr::col(Column::ThrottledLogRecords)
                    .add(i64::try_from(count).unwrap_or(i64::MAX)),
            )
            .filter(Column::Id.eq(id))
            .exec(&self.conn)
            .await
            .map(|_| ())
    }

    pub async fn delete_ingest_token(&self, id: &IngestTokenId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}
