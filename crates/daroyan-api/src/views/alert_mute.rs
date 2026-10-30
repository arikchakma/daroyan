use chrono::{DateTime, Utc};
use daroyan_core::db::entities::alert_mutes::Model as AlertMute;
use serde::Serialize;
use utoipa::ToSchema;

use crate::views::user::UserView;

#[derive(Serialize, ToSchema)]
#[schema(as = AlertMute)]
pub struct AlertMuteView {
    pub id: String,
    pub rule_id: String,
    pub server_id: Option<String>,
    pub check_id: Option<String>,
    pub cron_id: Option<String>,
    pub subject: Option<String>,
    #[serde(with = "daroyan_core::time::option")]
    pub ends_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub creator_id: String,
    pub creator: Option<UserView>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

impl AlertMuteView {
    pub fn new(mute: AlertMute, creator: Option<UserView>) -> Self {
        Self {
            id: mute.id,
            rule_id: mute.rule_id,
            server_id: mute.server_id,
            check_id: mute.check_id,
            cron_id: mute.cron_id,
            subject: mute.subject,
            ends_at: mute.ends_at,
            reason: mute.reason,
            creator_id: mute.creator_id,
            creator,
            created_at: mute.created_at,
        }
    }
}
