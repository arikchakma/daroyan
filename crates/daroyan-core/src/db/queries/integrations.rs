use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DbErr, EntityTrait, QueryOrder};
use serde_json::Value;

use crate::db::Database;
use crate::db::entities::integrations::{ActiveModel, Column, Entity, Model};
use crate::db::ids::IntegrationId;

pub struct NewIntegration {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: Value,
}

pub struct IntegrationChanges {
    pub name: Option<String>,
    pub kind_and_config: Option<(String, Value)>,
}

impl Database {
    pub async fn create_integration(
        &self,
        integration: NewIntegration,
    ) -> Result<Model, DbErr> {
        let now = Utc::now();
        ActiveModel {
            id: Set(integration.id),
            name: Set(integration.name),
            kind: Set(integration.kind),
            config: Set(integration.config),
            last_status: Set(None),
            last_error: Set(None),
            last_sent_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_integrations(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn find_integration(
        &self,
        id: &IntegrationId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn update_integration(
        &self,
        id: &IntegrationId,
        changes: IntegrationChanges,
    ) -> Result<Model, DbErr> {
        let mut integration = ActiveModel {
            id: Set(id.to_string()),
            updated_at: Set(Utc::now()),
            ..Default::default()
        };
        if let Some(name) = changes.name {
            integration.name = Set(name);
        }
        if let Some((kind, config)) = changes.kind_and_config {
            integration.kind = Set(kind);
            integration.config = Set(config);
        }
        integration.update(&self.conn).await
    }

    pub async fn record_integration_delivery(
        &self,
        id: &IntegrationId,
        error: Option<String>,
    ) -> Result<(), DbErr> {
        let now = Utc::now();
        ActiveModel {
            id: Set(id.to_string()),
            last_status: Set(Some(
                if error.is_none() { "ok" } else { "failed" }.to_string(),
            )),
            last_error: Set(error),
            last_sent_at: Set(Some(now)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn delete_integration(&self, id: &IntegrationId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}
