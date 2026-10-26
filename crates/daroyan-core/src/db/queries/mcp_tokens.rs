use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::Database;
use crate::db::entities::mcp_tokens::{ActiveModel, Column, Entity, Model};
use crate::db::entities::users;
use crate::db::ids::McpTokenId;

pub struct NewMcpToken {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub token_hash: String,
    pub hint: String,
}

impl Database {
    pub async fn create_mcp_token(&self, token: NewMcpToken) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(token.id),
            user_id: Set(token.user_id),
            name: Set(token.name),
            token_hash: Set(token.token_hash),
            hint: Set(token.hint),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_mcp_tokens(&self, user_id: &str) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::UserId.eq(user_id))
            .order_by_desc(Column::CreatedAt)
            .all(&self.conn)
            .await
    }

    pub async fn find_mcp_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<(Model, users::Model)>, DbErr> {
        let found = Entity::find()
            .filter(Column::TokenHash.eq(token_hash))
            .find_also_related(users::Entity)
            .one(&self.conn)
            .await?;
        Ok(found.and_then(|(token, user)| Some((token, user?))))
    }

    pub async fn touch_mcp_token(
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

    pub async fn delete_mcp_token(
        &self,
        id: &McpTokenId,
        user_id: &str,
    ) -> Result<bool, DbErr> {
        Ok(Entity::delete_many()
            .filter(Column::Id.eq(id.as_str()))
            .filter(Column::UserId.eq(user_id))
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}
