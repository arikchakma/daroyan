use chrono::{DateTime, Utc};
use daroyan_core::db::entities::install_tokens::Model as InstallToken;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = InstallToken)]
pub struct InstallTokenView {
    pub id: String,
    pub name: String,
    pub hint: String,
    #[serde(with = "daroyan_core::time::option")]
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub uses: i32,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub last_used_at: Option<DateTime<Utc>>,
}

impl From<InstallToken> for InstallTokenView {
    fn from(token: InstallToken) -> Self {
        Self {
            id: token.id,
            name: token.name,
            hint: token.hint,
            expires_at: token.expires_at,
            max_uses: token.max_uses,
            uses: token.uses,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = CreatedInstallToken)]
pub struct CreatedInstallTokenView {
    pub token: String,
    #[serde(flatten)]
    pub install_token: InstallTokenView,
}
