use chrono::{DateTime, Utc};
use daroyan_core::db::entities::incident_updates::Model as IncidentUpdate;
use daroyan_core::incidents::FollowUp;
use serde::Serialize;
use utoipa::ToSchema;

use crate::views::user::UserView;

#[derive(Clone, Serialize, ToSchema)]
#[schema(as = IncidentPerson)]
pub struct PersonView {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IncidentComponent)]
pub struct ComponentView {
    #[schema(example = "check")]
    pub kind: String,
    pub id: String,
    pub name: String,
    #[schema(example = "outage")]
    pub status: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IncidentRelated)]
pub struct RelatedView {
    #[schema(example = "server")]
    pub kind: String,
    pub id: String,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IncidentResponder)]
pub struct ResponderView {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IncidentStatusPage)]
pub struct StatusPageRefView {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub is_shown: bool,
}

#[derive(Serialize, ToSchema)]
#[schema(as = Incident)]
pub struct IncidentView {
    pub id: String,
    #[schema(example = "incident")]
    pub kind: String,
    pub title: String,
    #[schema(example = "investigating")]
    pub status: String,
    #[schema(example = "major")]
    pub severity: Option<String>,
    pub lead: Option<PersonView>,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub ends_at: Option<DateTime<Utc>>,
    pub auto_start: bool,
    pub auto_finish: bool,
    pub remind_before: bool,
    pub components: Vec<ComponentView>,
    pub related: Vec<RelatedView>,
    pub responders: Vec<ResponderView>,
    pub channel_ids: Vec<String>,
    pub status_pages: Vec<StatusPageRefView>,
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    pub postmortem: Option<serde_json::Value>,
    pub follow_ups: Vec<FollowUp>,
    #[serde(with = "daroyan_core::time::option")]
    pub postmortem_published_at: Option<DateTime<Utc>>,
    pub public_update_count: u64,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IncidentUpdate)]
pub struct IncidentUpdateView {
    pub id: String,
    #[schema(example = "update")]
    pub kind: String,
    #[schema(example = "identified")]
    pub status: Option<String>,
    #[schema(value_type = std::collections::HashMap<String, serde_json::Value>)]
    pub body: serde_json::Value,
    pub is_public: bool,
    pub creator_id: Option<String>,
    pub creator: Option<UserView>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub edited_at: Option<DateTime<Utc>>,
}

impl IncidentUpdateView {
    pub fn new(update: IncidentUpdate, creator: Option<UserView>) -> Self {
        Self {
            id: update.id,
            kind: update.kind,
            status: update.status,
            body: update.body,
            is_public: update.is_public,
            creator_id: update.creator_id,
            creator,
            created_at: update.created_at,
            edited_at: update.edited_at,
        }
    }
}
