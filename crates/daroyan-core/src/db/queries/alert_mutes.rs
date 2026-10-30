use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::Database;
use crate::db::entities::alert_mutes::{ActiveModel, Column, Entity, Model};
use crate::db::ids::AlertMuteId;

pub struct NewAlertMute {
    pub id: String,
    pub rule_id: String,
    pub server_id: Option<String>,
    pub check_id: Option<String>,
    pub cron_id: Option<String>,
    pub subject: Option<String>,
    pub ends_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub creator_id: String,
}

impl Model {
    pub fn covers(&self, rule_id: &str, target_id: &str, subject: Option<&str>) -> bool {
        self.rule_id == rule_id
            && self.subject.as_deref().is_none_or(|muted| subject == Some(muted))
            && match (&self.server_id, &self.check_id, &self.cron_id) {
                (None, None, None) => true,
                (server_id, check_id, cron_id) => {
                    server_id.as_deref() == Some(target_id)
                        || check_id.as_deref() == Some(target_id)
                        || cron_id.as_deref() == Some(target_id)
                }
            }
    }
}

impl Database {
    pub async fn create_alert_mute(&self, mute: NewAlertMute) -> Result<Model, DbErr> {
        let mut replaced =
            Entity::delete_many().filter(Column::RuleId.eq(mute.rule_id.as_str()));
        replaced = match &mute.server_id {
            Some(id) => replaced.filter(Column::ServerId.eq(id.as_str())),
            None => replaced.filter(Column::ServerId.is_null()),
        };
        replaced = match &mute.check_id {
            Some(id) => replaced.filter(Column::CheckId.eq(id.as_str())),
            None => replaced.filter(Column::CheckId.is_null()),
        };
        replaced = match &mute.cron_id {
            Some(id) => replaced.filter(Column::CronId.eq(id.as_str())),
            None => replaced.filter(Column::CronId.is_null()),
        };
        replaced = match &mute.subject {
            Some(subject) => replaced.filter(Column::Subject.eq(subject.as_str())),
            None => replaced.filter(Column::Subject.is_null()),
        };
        replaced.exec(&self.conn).await?;
        ActiveModel {
            id: Set(mute.id),
            rule_id: Set(mute.rule_id),
            server_id: Set(mute.server_id),
            check_id: Set(mute.check_id),
            cron_id: Set(mute.cron_id),
            subject: Set(mute.subject),
            ends_at: Set(mute.ends_at),
            reason: Set(mute.reason),
            creator_id: Set(mute.creator_id),
            created_at: Set(Utc::now()),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_alert_mutes(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().order_by_asc(Column::CreatedAt).all(&self.conn).await
    }

    pub async fn delete_alert_mute(&self, id: &AlertMuteId) -> Result<bool, DbErr> {
        Ok(Entity::delete_by_id(id.as_str())
            .exec(&self.conn)
            .await?
            .rows_affected
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mute(server_id: Option<&str>, subject: Option<&str>) -> Model {
        Model {
            id: "mute_1".to_string(),
            rule_id: "rule_1".to_string(),
            server_id: server_id.map(str::to_string),
            check_id: None,
            cron_id: None,
            subject: subject.map(str::to_string),
            ends_at: None,
            reason: None,
            creator_id: "usr_1".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn a_subject_mute_covers_only_that_subject() {
        let mute = make_mute(Some("otlp"), Some("inboxjot-api"));
        assert!(mute.covers("rule_1", "otlp", Some("inboxjot-api")));
        assert!(!mute.covers("rule_1", "otlp", Some("inboxjot-worker")));
        assert!(!mute.covers("rule_1", "otlp", None));
        assert!(!mute.covers("rule_1", "srv_1", Some("inboxjot-api")));
    }

    #[test]
    fn a_target_mute_covers_every_subject() {
        let mute = make_mute(Some("otlp"), None);
        assert!(mute.covers("rule_1", "otlp", Some("inboxjot-api")));
        assert!(mute.covers("rule_1", "otlp", None));
        assert!(!mute.covers("rule_2", "otlp", None));
    }
}
