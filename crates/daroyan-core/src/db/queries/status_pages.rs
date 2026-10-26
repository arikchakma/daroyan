use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::Database;
use crate::db::entities::status_pages::{ActiveModel, Column, Entity, Model};
use crate::db::ids::StatusPageId;

pub struct StatusPageFields {
    pub name: String,
    pub slug: String,
    pub logo_url: Option<String>,
    pub is_public: bool,
    pub sections: serde_json::Value,
}

impl Database {
    pub async fn create_status_page(
        &self,
        id: String,
        fields: StatusPageFields,
    ) -> Result<Model, DbErr> {
        to_active_model(id, fields, Set(Utc::now())).insert(&self.conn).await
    }

    pub async fn list_status_pages(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn find_status_page(
        &self,
        id: &StatusPageId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn find_status_page_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find().filter(Column::Slug.eq(slug)).one(&self.conn).await
    }

    pub async fn update_status_page(
        &self,
        id: &StatusPageId,
        fields: StatusPageFields,
    ) -> Result<Model, DbErr> {
        to_active_model(id.to_string(), fields, sea_orm::ActiveValue::NotSet)
            .update(&self.conn)
            .await
    }

    pub async fn delete_status_page(&self, id: &StatusPageId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}

fn to_active_model(
    id: String,
    fields: StatusPageFields,
    created_at: sea_orm::ActiveValue<chrono::DateTime<Utc>>,
) -> ActiveModel {
    ActiveModel {
        id: Set(id),
        name: Set(fields.name),
        slug: Set(fields.slug),
        logo_url: Set(fields.logo_url),
        is_public: Set(fields.is_public),
        sections: Set(fields.sections),
        created_at,
        updated_at: Set(Utc::now()),
    }
}
