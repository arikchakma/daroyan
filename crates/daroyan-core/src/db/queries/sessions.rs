use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
};

use crate::db::Database;
use crate::db::entities::sessions::{ActiveModel, Column, Entity, Model};
use crate::db::entities::users;

pub struct NewSession<'a> {
    pub id: String,
    pub user_id: &'a str,
    pub token_hash: String,
    pub user_agent: Option<&'a str>,
    pub ip_address: Option<&'a str>,
    pub expires_at: DateTime<Utc>,
}

impl Database {
    pub async fn create_session(&self, session: NewSession<'_>) -> Result<Model, DbErr> {
        let now = Utc::now();
        Entity::delete_many()
            .filter(Column::UserId.eq(session.user_id))
            .filter(Column::ExpiresAt.lte(now))
            .exec(&self.conn)
            .await?;
        ActiveModel {
            id: Set(session.id),
            user_id: Set(session.user_id.to_owned()),
            token_hash: Set(session.token_hash),
            user_agent: Set(session.user_agent.map(str::to_owned)),
            ip_address: Set(session.ip_address.map(str::to_owned)),
            created_at: Set(now),
            expires_at: Set(session.expires_at),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn find_active_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<(Model, users::Model)>, DbErr> {
        let found = Entity::find()
            .filter(Column::TokenHash.eq(token_hash))
            .filter(Column::ExpiresAt.gt(Utc::now()))
            .find_also_related(users::Entity)
            .one(&self.conn)
            .await?;
        Ok(found.and_then(|(session, user)| Some((session, user?))))
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<(), DbErr> {
        Entity::delete_many()
            .filter(Column::TokenHash.eq(token_hash))
            .exec(&self.conn)
            .await?;
        Ok(())
    }
}
