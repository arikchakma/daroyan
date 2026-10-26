use chrono::{DateTime, Utc};
use daroyan_core::alerts::engine::parse_rule;
use daroyan_core::alerts::rules::{Condition, RuleOptions, Scope, describe_rule};
use daroyan_core::db::entities::alert_rules::Model as AlertRule;
use daroyan_core::error::{Error, Result};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = AlertRule)]
pub struct AlertRuleView {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition: Condition,
    pub for_secs: u32,
    pub scope: Scope,
    pub integration_ids: Vec<String>,
    pub options: RuleOptions,
    pub declare_incident: bool,
    #[schema(example = "major")]
    pub declare_severity: Option<String>,
    pub declare_hidden_page_ids: Vec<String>,
    pub summary: String,
    #[serde(with = "daroyan_core::time::option")]
    pub last_fired_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub quiet_until: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

impl AlertRuleView {
    pub fn new(rule: AlertRule) -> Result<Self> {
        let parsed = parse_rule(&rule).ok_or_else(|| {
            Error::Internal(anyhow::anyhow!("alert rule {} is unreadable", rule.id))
        })?;
        let for_secs = u32::try_from(rule.for_secs).unwrap_or_default();
        Ok(Self {
            summary: describe_rule(&parsed.condition, &parsed.options, for_secs),
            id: rule.id,
            name: rule.name,
            enabled: rule.enabled,
            condition: parsed.condition,
            for_secs,
            scope: parsed.scope,
            integration_ids: parsed.integration_ids,
            options: parsed.options,
            declare_incident: rule.declare_incident,
            declare_severity: rule.declare_severity,
            declare_hidden_page_ids: serde_json::from_value(rule.declare_hidden_page_ids)
                .unwrap_or_default(),
            last_fired_at: rule.last_fired_at,
            quiet_until: rule.quiet_until,
            created_at: rule.created_at,
        })
    }
}
