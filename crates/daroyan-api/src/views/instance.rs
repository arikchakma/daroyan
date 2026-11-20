use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Instance)]
pub struct InstanceView {
    pub app_url: Option<String>,
    pub retention_days: u32,
    pub log_retention_days: u32,
    pub archive: Option<ArchiveView>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = Archive)]
pub struct ArchiveView {
    pub url: String,
    pub retention_days: u32,
    pub files: u64,
    pub bytes: u64,
    pub oldest_at: Option<i64>,
    pub exported_at: Option<i64>,
    pub last_error: Option<String>,
    pub upkeep_error: Option<String>,
}
