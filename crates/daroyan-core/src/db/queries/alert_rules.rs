use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use serde_json::Value;

use crate::alerts::rules::Scope;
use crate::db::Database;
use crate::db::entities::alert_rules::{ActiveModel, Column, Entity, Model};
use crate::db::entities::{alert_deliveries, alert_mutes, alerts};
use crate::db::ids::AlertRuleId;

pub struct NewAlertRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition: Value,
    pub for_secs: i64,
    pub scope: Value,
    pub integration_ids: Value,
    pub options: Value,
    pub declare: RuleDeclare,
}

#[derive(Clone, Default)]
pub struct RuleDeclare {
    pub is_on: bool,
    pub severity: Option<String>,
    pub hidden_page_ids: Value,
}

#[derive(Default)]
pub struct AlertRuleChanges {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub condition: Option<Value>,
    pub for_secs: Option<i64>,
    pub scope: Option<Value>,
    pub integration_ids: Option<Value>,
    pub options: Option<Value>,
    pub declare: Option<RuleDeclare>,
}

impl Database {
    pub async fn create_alert_rule(&self, rule: NewAlertRule) -> Result<Model, DbErr> {
        let now = Utc::now();
        ActiveModel {
            id: Set(rule.id),
            name: Set(rule.name),
            enabled: Set(rule.enabled),
            condition: Set(rule.condition),
            for_secs: Set(rule.for_secs),
            scope: Set(rule.scope),
            integration_ids: Set(rule.integration_ids),
            options: Set(rule.options),
            last_fired_at: Set(None),
            quiet_until: Set(None),
            declare_incident: Set(rule.declare.is_on),
            declare_severity: Set(rule.declare.severity),
            declare_hidden_page_ids: Set(rule.declare.hidden_page_ids),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.conn)
        .await
    }

    pub async fn list_alert_rules(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn find_alert_rule(
        &self,
        id: &AlertRuleId,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id.as_str()).one(&self.conn).await
    }

    pub async fn remove_target_from_rules(&self, target_id: &str) -> Result<(), DbErr> {
        for rule in self.list_alert_rules().await? {
            let Ok(scope) = serde_json::from_value::<Scope>(rule.scope.clone()) else {
                continue;
            };
            let id = AlertRuleId::from(rule.id.as_str());
            match scope.without(target_id) {
                Some(left) if left == scope => {}
                Some(left) => {
                    let changes = AlertRuleChanges {
                        scope: Some(serde_json::to_value(left).unwrap_or(rule.scope)),
                        ..Default::default()
                    };
                    self.update_alert_rule(&id, changes).await?;
                }
                None => {
                    self.delete_alert_rule(&id).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn update_alert_rule(
        &self,
        id: &AlertRuleId,
        changes: AlertRuleChanges,
    ) -> Result<Model, DbErr> {
        let mut rule = ActiveModel {
            id: Set(id.to_string()),
            updated_at: Set(Utc::now()),
            ..Default::default()
        };
        if let Some(name) = changes.name {
            rule.name = Set(name);
        }
        if let Some(enabled) = changes.enabled {
            rule.enabled = Set(enabled);
        }
        if let Some(condition) = changes.condition {
            rule.condition = Set(condition);
        }
        if let Some(for_secs) = changes.for_secs {
            rule.for_secs = Set(for_secs);
        }
        if let Some(scope) = changes.scope {
            rule.scope = Set(scope);
        }
        if let Some(integration_ids) = changes.integration_ids {
            rule.integration_ids = Set(integration_ids);
        }
        if let Some(options) = changes.options {
            rule.options = Set(options);
        }
        if let Some(declare) = changes.declare {
            rule.declare_incident = Set(declare.is_on);
            rule.declare_severity = Set(declare.severity);
            rule.declare_hidden_page_ids = Set(declare.hidden_page_ids);
        }
        rule.update(&self.conn).await
    }

    pub async fn mark_alert_rule_fired(
        &self,
        id: &AlertRuleId,
        at: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            last_fired_at: Set(Some(at)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn mark_alert_rule_quiet(
        &self,
        id: &AlertRuleId,
        until: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        ActiveModel {
            id: Set(id.to_string()),
            quiet_until: Set(Some(until)),
            ..Default::default()
        }
        .update(&self.conn)
        .await
        .map(|_| ())
    }

    pub async fn delete_alert_rule(&self, id: &AlertRuleId) -> Result<bool, DbErr> {
        let txn = self.conn.begin().await?;
        alert_deliveries::Entity::delete_many()
            .filter(alert_deliveries::Column::RuleId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        alerts::Entity::delete_many()
            .filter(alerts::Column::RuleId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        alert_mutes::Entity::delete_many()
            .filter(alert_mutes::Column::RuleId.eq(id.as_str()))
            .exec(&txn)
            .await?;
        let deleted = Entity::delete_by_id(id.as_str()).exec(&txn).await?;
        txn.commit().await?;
        Ok(deleted.rows_affected > 0)
    }
}
