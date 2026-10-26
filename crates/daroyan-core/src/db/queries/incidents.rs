use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, ColumnTrait, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};

use crate::db::Database;
use crate::db::entities::incident_updates;
use crate::db::entities::incidents::{ActiveModel, Column, Entity, Model};
use crate::db::ids::{IncidentId, IncidentUpdateId};

pub struct IncidentFields {
    pub title: String,
    pub severity: Option<String>,
    pub lead_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub auto_start: bool,
    pub auto_finish: bool,
    pub remind_before: bool,
    pub components: serde_json::Value,
    pub related: serde_json::Value,
    pub responders: serde_json::Value,
    pub channel_ids: serde_json::Value,
    pub hidden_page_ids: serde_json::Value,
}

pub struct PostmortemFields {
    pub postmortem: Option<serde_json::Value>,
    pub follow_ups: serde_json::Value,
}

pub struct NewIncidentUpdate {
    pub id: String,
    pub incident_id: String,
    pub kind: &'static str,
    pub status: Option<String>,
    pub body: serde_json::Value,
    pub is_public: bool,
    pub creator_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Database {
    pub async fn create_incident(
        &self,
        id: String,
        kind: &str,
        status: &str,
        fields: IncidentFields,
    ) -> Result<Model, DbErr> {
        let now = Utc::now();
        let mut model = to_active_model(id, fields);
        model.kind = Set(kind.to_string());
        model.status = Set(status.to_string());
        model.resolved_at = Set(None);
        model.follow_ups = Set(serde_json::json!([]));
        model.postmortem = Set(None);
        model.postmortem_published_at = Set(None);
        model.reminded_at = Set(None);
        model.created_at = Set(now);
        model.insert(&self.conn).await
    }

    pub async fn list_incidents(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_desc(Column::StartedAt)
            .order_by_desc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn list_open_incidents(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::ResolvedAt.is_null())
            .all(&self.conn)
            .await
    }

    pub async fn find_incident(&self, id: &IncidentId) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn update_incident(
        &self,
        id: &IncidentId,
        fields: IncidentFields,
    ) -> Result<Model, DbErr> {
        to_active_model(id.to_string(), fields).update(&self.conn).await
    }

    pub async fn set_incident_status(
        &self,
        id: &IncidentId,
        status: &str,
        started_at: Option<DateTime<Utc>>,
        resolved_at: Option<DateTime<Utc>>,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            status: Set(status.to_string()),
            started_at: started_at.map_or(NotSet, Set),
            resolved_at: Set(resolved_at),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }

    pub async fn save_postmortem(
        &self,
        id: &IncidentId,
        fields: PostmortemFields,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            postmortem: Set(fields.postmortem),
            follow_ups: Set(fields.follow_ups),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }

    pub async fn mark_incident_reminded(
        &self,
        id: &IncidentId,
        at: Option<DateTime<Utc>>,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            reminded_at: Set(at),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }

    pub async fn publish_postmortem(
        &self,
        id: &IncidentId,
        at: Option<DateTime<Utc>>,
    ) -> Result<Model, DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            postmortem_published_at: Set(at),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }

    pub async fn delete_incident(&self, id: &IncidentId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }

    pub async fn create_incident_update(
        &self,
        update: NewIncidentUpdate,
    ) -> Result<incident_updates::Model, DbErr> {
        incident_updates::ActiveModel {
            id: Set(update.id),
            incident_id: Set(update.incident_id),
            kind: Set(update.kind.to_string()),
            status: Set(update.status),
            body: Set(update.body),
            is_public: Set(update.is_public),
            creator_id: Set(update.creator_id),
            created_at: Set(update.created_at),
            edited_at: Set(None),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_incident_updates(
        &self,
        incident_id: &IncidentId,
    ) -> Result<Vec<incident_updates::Model>, DbErr> {
        incident_updates::Entity::find()
            .filter(incident_updates::Column::IncidentId.eq(incident_id.as_str()))
            .order_by_desc(incident_updates::Column::CreatedAt)
            .order_by_desc(incident_updates::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn list_public_updates(
        &self,
        incident_ids: Vec<String>,
    ) -> Result<Vec<incident_updates::Model>, DbErr> {
        incident_updates::Entity::find()
            .filter(incident_updates::Column::IncidentId.is_in(incident_ids))
            .filter(incident_updates::Column::IsPublic.eq(true))
            .order_by_desc(incident_updates::Column::CreatedAt)
            .order_by_desc(incident_updates::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn count_public_updates(
        &self,
        incident_ids: Vec<String>,
    ) -> Result<Vec<(String, i64)>, DbErr> {
        incident_updates::Entity::find()
            .select_only()
            .column(incident_updates::Column::IncidentId)
            .column_as(incident_updates::Column::Id.count(), "count")
            .filter(incident_updates::Column::IncidentId.is_in(incident_ids))
            .filter(incident_updates::Column::IsPublic.eq(true))
            .group_by(incident_updates::Column::IncidentId)
            .into_tuple()
            .all(&self.conn)
            .await
    }

    pub async fn find_incident_update(
        &self,
        id: &IncidentUpdateId,
    ) -> Result<Option<incident_updates::Model>, DbErr> {
        incident_updates::Entity::find_by_id(id.as_str())
            .one(&self.conn)
            .await
    }

    pub async fn edit_incident_update(
        &self,
        id: &IncidentUpdateId,
        body: serde_json::Value,
    ) -> Result<incident_updates::Model, DbErr> {
        incident_updates::ActiveModel {
            id: Set(id.to_string()),
            body: Set(body),
            edited_at: Set(Some(Utc::now())),
            ..Default::default()
        }
        .update(&self.conn)
        .await
    }
}

fn to_active_model(id: String, fields: IncidentFields) -> ActiveModel {
    ActiveModel {
        id: Set(id),
        title: Set(fields.title),
        severity: Set(fields.severity),
        lead_id: Set(fields.lead_id),
        started_at: Set(fields.started_at),
        starts_at: Set(fields.starts_at),
        ends_at: Set(fields.ends_at),
        auto_start: Set(fields.auto_start),
        auto_finish: Set(fields.auto_finish),
        remind_before: Set(fields.remind_before),
        components: Set(fields.components),
        related: Set(fields.related),
        responders: Set(fields.responders),
        channel_ids: Set(fields.channel_ids),
        hidden_page_ids: Set(fields.hidden_page_ids),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
}
