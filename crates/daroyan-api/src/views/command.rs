use chrono::{DateTime, Utc};
use daroyan_core::db::entities::commands::Model as StoredCommand;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Command)]
pub struct CommandView {
    pub id: String,
    pub server_id: String,
    #[schema(value_type = String, example = "restart_process")]
    pub action: String,
    pub name: String,
    #[schema(value_type = String, example = "sent")]
    pub status: String,
    pub error: Option<String>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time")]
    pub updated_at: DateTime<Utc>,
}

impl From<StoredCommand> for CommandView {
    fn from(command: StoredCommand) -> Self {
        Self {
            id: command.id,
            server_id: command.server_id,
            action: command.action,
            name: command.name,
            status: command.status,
            error: command.error,
            created_at: command.created_at,
            updated_at: command.updated_at,
        }
    }
}
