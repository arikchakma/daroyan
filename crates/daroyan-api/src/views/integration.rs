use chrono::{DateTime, Utc};
use daroyan_core::alerts::integrations::IntegrationConfig;
use daroyan_core::db::entities::integrations::Model as Integration;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Integration)]
pub struct IntegrationView {
    pub id: String,
    pub name: String,
    #[schema(value_type = String, example = "slack")]
    pub kind: String,
    pub target: String,
    #[schema(value_type = Option<String>, example = "ok")]
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    #[serde(with = "daroyan_core::time::option")]
    pub last_sent_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

impl From<Integration> for IntegrationView {
    fn from(integration: Integration) -> Self {
        let target = serde_json::from_value::<IntegrationConfig>(integration.config)
            .map_or_else(
                |_| "Unreadable settings".to_string(),
                |config| config.describe_target(),
            );
        Self {
            id: integration.id,
            name: integration.name,
            kind: integration.kind,
            target,
            last_status: integration.last_status,
            last_error: integration.last_error,
            last_sent_at: integration.last_sent_at,
            created_at: integration.created_at,
        }
    }
}
