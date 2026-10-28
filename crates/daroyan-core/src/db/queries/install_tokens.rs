use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, DbErr, EntityTrait,
    QueryFilter, QueryOrder,
};

use crate::db::Database;
use crate::db::entities::install_tokens::{ActiveModel, Column, Entity, Model};
use crate::db::ids::InstallTokenId;

pub struct NewInstallToken {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub hint: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
}

fn usable(now: DateTime<Utc>) -> Condition {
    Condition::all()
        .add(
            Condition::any()
                .add(Column::ExpiresAt.is_null())
                .add(Column::ExpiresAt.gt(now)),
        )
        .add(
            Condition::any()
                .add(Column::MaxUses.is_null())
                .add(Expr::col(Column::Uses).lt(Expr::col(Column::MaxUses))),
        )
}

impl Database {
    pub async fn create_install_token(
        &self,
        token: NewInstallToken,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(token.id),
            name: Set(token.name),
            token_hash: Set(token.token_hash),
            hint: Set(token.hint),
            expires_at: Set(token.expires_at),
            max_uses: Set(token.max_uses),
            uses: Set(0),
            created_at: Set(Utc::now()),
            last_used_at: Set(None),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_install_tokens(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(usable(Utc::now()))
            .order_by_desc(Column::CreatedAt)
            .all(&self.conn)
            .await
    }

    pub async fn claim_install_token(&self, token_hash: &str) -> Result<bool, DbErr> {
        let now = Utc::now();
        let claimed = Entity::update_many()
            .col_expr(Column::Uses, Expr::col(Column::Uses).add(1))
            .col_expr(Column::LastUsedAt, Expr::value(now))
            .filter(Column::TokenHash.eq(token_hash))
            .filter(usable(now))
            .exec(&self.conn)
            .await?;
        Ok(claimed.rows_affected > 0)
    }

    pub async fn delete_install_token(&self, id: &InstallTokenId) -> Result<bool, DbErr> {
        let deleted = Entity::delete_by_id(id.as_str()).exec(&self.conn).await?;
        Ok(deleted.rows_affected > 0)
    }
}
