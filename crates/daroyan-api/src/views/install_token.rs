use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = InstallToken)]
pub struct InstallTokenView {
    pub id: String,
    pub token: String,
    #[serde(with = "daroyan_core::time")]
    pub expires_at: DateTime<Utc>,
    pub max_uses: i32,
}
