use chrono::{DateTime, Utc};
use daroyan_core::analytics::Backup;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Backup)]
pub struct BackupView {
    pub name: String,
    pub size_bytes: u64,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

impl From<Backup> for BackupView {
    fn from(backup: Backup) -> Self {
        Self {
            name: backup.name,
            size_bytes: backup.size_bytes,
            created_at: backup.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = BackupStatus)]
pub struct BackupStatusView {
    pub is_enabled: bool,
    pub last_backup: Option<BackupView>,
}
