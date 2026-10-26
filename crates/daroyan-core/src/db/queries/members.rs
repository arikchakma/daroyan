use chrono::Utc;
use sea_orm::sea_query::{Expr, Query, SimpleExpr};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};

use crate::auth::Role;

use crate::db::Database;
use crate::db::entities::members::{ActiveModel, Column, Entity, Model};
use crate::db::entities::users;
use crate::db::ids::MemberId;
use crate::db::queries::users::revoke_user;

pub struct NewMember {
    pub id: String,
    pub email: String,
    pub role: String,
    pub inviter_id: String,
}

impl Database {
    pub async fn create_member(&self, member: NewMember) -> Result<Model, DbErr> {
        let txn = self.conn.begin().await?;
        let user = users::Entity::find()
            .filter(users::Column::Email.eq(&member.email))
            .one(&txn)
            .await?;
        if let Some(user) = user {
            revoke_user(&txn, &user.id).await?;
        }
        let created = ActiveModel {
            id: Set(member.id),
            email: Set(member.email),
            role: Set(member.role),
            inviter_id: Set(Some(member.inviter_id)),
            created_at: Set(Utc::now()),
        }
        .insert(&txn)
        .await?;
        txn.commit().await?;
        Ok(created)
    }

    pub async fn list_members(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().order_by_asc(Column::CreatedAt).all(&self.conn).await
    }

    pub async fn find_member(&self, id: &MemberId) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn find_member_by_email(
        &self,
        email: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find().filter(Column::Email.eq(email)).one(&self.conn).await
    }

    pub async fn update_member_role(
        &self,
        id: &MemberId,
        role: &str,
        actor: Option<&str>,
    ) -> Result<bool, DbErr> {
        let updated = Entity::update_many()
            .col_expr(Column::Role, Expr::value(role))
            .filter(Column::Id.eq(id.as_str()))
            .filter(is_still_admin(actor))
            .exec(&self.conn)
            .await?;
        Ok(updated.rows_affected > 0)
    }

    pub async fn delete_member(
        &self,
        member: &Model,
        actor: Option<&str>,
    ) -> Result<bool, DbErr> {
        let txn = self.conn.begin().await?;
        let deleted = Entity::delete_many()
            .filter(Column::Id.eq(member.id.as_str()))
            .filter(is_still_admin(actor))
            .exec(&txn)
            .await?;
        if deleted.rows_affected == 0 {
            return Ok(false);
        }
        let user = users::Entity::find()
            .filter(users::Column::Email.eq(&member.email))
            .one(&txn)
            .await?;
        if let Some(user) = user {
            revoke_user(&txn, &user.id).await?;
        }
        txn.commit().await?;
        Ok(true)
    }
}

fn is_still_admin(actor: Option<&str>) -> SimpleExpr {
    let Some(email) = actor else {
        return Expr::value(true);
    };
    Expr::exists(
        Query::select()
            .expr(Expr::value(1))
            .from(Entity)
            .and_where(Column::Email.eq(email))
            .and_where(Column::Role.eq(Role::Admin.as_str()))
            .to_owned(),
    )
}
