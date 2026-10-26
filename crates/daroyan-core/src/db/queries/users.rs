use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DbErr, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder,
};

use crate::db::Database;
use crate::db::entities::users::{ActiveModel, Column, Entity, Model};
use crate::db::entities::{mcp_tokens, sessions};

pub struct GoogleUser<'a> {
    pub id: String,
    pub google_id: &'a str,
    pub email: &'a str,
    pub name: &'a str,
    pub avatar_url: Option<&'a str>,
}

pub(crate) async fn revoke_user<C: ConnectionTrait>(
    conn: &C,
    user_id: &str,
) -> Result<(), DbErr> {
    sessions::Entity::delete_many()
        .filter(sessions::Column::UserId.eq(user_id))
        .exec(conn)
        .await?;
    mcp_tokens::Entity::delete_many()
        .filter(mcp_tokens::Column::UserId.eq(user_id))
        .exec(conn)
        .await?;
    Ok(())
}

impl Database {
    pub async fn save_google_user(&self, user: GoogleUser<'_>) -> Result<Model, DbErr> {
        let by_google = Entity::find()
            .filter(Column::GoogleId.eq(user.google_id))
            .one(&self.conn)
            .await?;
        let by_email = Entity::find()
            .filter(Column::Email.eq(user.email))
            .one(&self.conn)
            .await?;
        if let Some(stale) =
            by_email.as_ref().filter(|row| row.google_id != user.google_id)
        {
            revoke_user(&self.conn, &stale.id).await?;
        }
        let existing = by_google.or(by_email);
        let now = Utc::now();
        let is_new = existing.is_none();
        let mut model = match existing {
            Some(existing) => existing.into_active_model(),
            None => ActiveModel {
                id: Set(user.id),
                created_at: Set(now),
                ..Default::default()
            },
        };
        model.google_id = Set(user.google_id.to_owned());
        model.email = Set(user.email.to_owned());
        model.name = Set(user.name.to_owned());
        model.avatar_url = Set(user.avatar_url.map(str::to_owned));
        model.updated_at = Set(now);
        if is_new {
            model.insert(&self.conn).await
        } else {
            model.update(&self.conn).await
        }
    }

    pub async fn list_users(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().order_by_asc(Column::Name).all(&self.conn).await
    }
}
