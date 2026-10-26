use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
};

use crate::db::Database;
use crate::db::entities::install_tokens::{ActiveModel, Column, Entity, Model};
use crate::db::ids::InstallTokenId;

pub struct NewInstallToken {
    pub id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub max_uses: i32,
}

impl Database {
    pub async fn create_install_token(
        &self,
        token: NewInstallToken,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(token.id),
            token_hash: Set(token.token_hash),
            expires_at: Set(token.expires_at),
            created_at: Set(Utc::now()),
            max_uses: Set(token.max_uses),
            uses: Set(0),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn claim_install_token(&self, token_hash: &str) -> Result<bool, DbErr> {
        let claimed = Entity::update_many()
            .col_expr(Column::Uses, Expr::col(Column::Uses).add(1))
            .filter(Column::TokenHash.eq(token_hash))
            .filter(Column::ExpiresAt.gt(Utc::now()))
            .filter(Expr::col(Column::Uses).lt(Expr::col(Column::MaxUses)))
            .exec(&self.conn)
            .await?;
        Ok(claimed.rows_affected > 0)
    }

    pub async fn delete_install_token(&self, id: &InstallTokenId) -> Result<bool, DbErr> {
        let deleted = Entity::delete_by_id(id.as_str()).exec(&self.conn).await?;
        Ok(deleted.rows_affected > 0)
    }
}
