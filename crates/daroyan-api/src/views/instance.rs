use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = Instance)]
pub struct InstanceView {
    pub app_url: Option<String>,
    pub retention_days: u32,
    pub log_retention_days: u32,
}
