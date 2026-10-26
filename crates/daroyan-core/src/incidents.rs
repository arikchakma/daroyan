use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use utoipa::ToSchema;

use crate::alerts::engine::deliver_notice;
use crate::alerts::notify::Notice;
use crate::db::Database;
use crate::db::entities::alert_rules::Model as AlertRule;
use crate::db::entities::incidents::Model as Incident;
use crate::db::ids::IncidentId;
use crate::db::queries::incidents::{IncidentFields, NewIncidentUpdate};
use crate::error::Result;
use crate::id::{Prefix, new_id};
use crate::rich_text::{paragraph, to_plain_text};

const TICK: Duration = Duration::from_secs(30);
const REMIND_BEFORE_SECS: i64 = 3600;

pub const KIND_INCIDENT: &str = "incident";
pub const KIND_MAINTENANCE: &str = "maintenance";

pub const INVESTIGATING: &str = "investigating";
pub const IDENTIFIED: &str = "identified";
pub const MONITORING: &str = "monitoring";
pub const RESOLVED: &str = "resolved";
pub const SCHEDULED: &str = "scheduled";
pub const IN_PROGRESS: &str = "in_progress";
pub const COMPLETED: &str = "completed";

pub const INCIDENT_STATUSES: [&str; 4] =
    [INVESTIGATING, IDENTIFIED, MONITORING, RESOLVED];
pub const MAINTENANCE_STATUSES: [&str; 3] = [SCHEDULED, IN_PROGRESS, COMPLETED];

pub const SEVERITIES: [&str; 3] = ["minor", "major", "critical"];

pub const DEGRADED: &str = "degraded";
pub const PARTIAL: &str = "partial";
pub const OUTAGE: &str = "outage";
pub const MAINTENANCE: &str = "maintenance";
pub const COMPONENT_STATUSES: [&str; 3] = [DEGRADED, PARTIAL, OUTAGE];

pub const UPDATE: &str = "update";
pub const NOTE: &str = "note";
pub const AUTOMATIC: &str = "automatic";

pub const RELATED_KINDS: [&str; 4] = ["rule", "server", "check", "cron"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = IncidentComponentRef)]
pub struct AffectedComponent {
    #[schema(example = "check")]
    pub kind: String,
    pub id: String,
    #[schema(example = "outage")]
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = IncidentRelatedRef)]
pub struct RelatedItem {
    #[schema(example = "server")]
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = IncidentResponderRef)]
pub struct Responder {
    pub user_id: String,
    #[schema(example = "Responder")]
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = IncidentFollowUp)]
pub struct FollowUp {
    pub id: String,
    pub body: String,
    pub owner_id: Option<String>,
    pub is_done: bool,
}

pub fn is_closed(status: &str) -> bool {
    status == RESOLVED || status == COMPLETED
}

pub fn statuses_of(kind: &str) -> &'static [&'static str] {
    if kind == KIND_MAINTENANCE { &MAINTENANCE_STATUSES } else { &INCIDENT_STATUSES }
}

pub fn rank(status: &str) -> u8 {
    match status {
        DEGRADED => 1,
        PARTIAL => 2,
        OUTAGE => 3,
        _ => 0,
    }
}

pub fn merge_status<'a>(
    monitor: &'a str,
    incident: Option<&'a str>,
    is_maintenance: bool,
) -> &'a str {
    match incident {
        Some(incident) if rank(incident) >= rank(monitor) => incident,
        Some(_) => monitor,
        None if is_maintenance => MAINTENANCE,
        None => monitor,
    }
}

pub fn describe_component_status(status: &str) -> &'static str {
    match status {
        DEGRADED => "Degraded Performance",
        PARTIAL => "Partial Outage",
        OUTAGE => "Major Outage",
        MAINTENANCE => "Under Maintenance",
        _ => "Operational",
    }
}

pub fn read_json<T: serde::de::DeserializeOwned + Default>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

pub fn describe_status(status: &str) -> &'static str {
    match status {
        INVESTIGATING => "Investigating",
        IDENTIFIED => "Identified",
        MONITORING => "Monitoring",
        RESOLVED => "Resolved",
        SCHEDULED => "Scheduled",
        IN_PROGRESS => "In Progress",
        COMPLETED => "Completed",
        _ => "Updated",
    }
}

fn describe_kind(incident: &Incident) -> String {
    if incident.kind == KIND_MAINTENANCE {
        return "Maintenance".to_string();
    }
    let severity = incident.severity.as_deref().unwrap_or("minor");
    let mut chars = severity.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub struct EntryInput {
    pub kind: &'static str,
    pub status: Option<String>,
    pub body: Value,
    pub is_public: bool,
    pub creator_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn add_entry(
    database: &Database,
    incident_id: &str,
    entry: EntryInput,
) -> Result<crate::db::entities::incident_updates::Model> {
    Ok(database
        .create_incident_update(NewIncidentUpdate {
            id: new_id(Prefix::IncidentUpdate),
            incident_id: incident_id.to_string(),
            kind: entry.kind,
            status: entry.status,
            body: entry.body,
            is_public: entry.is_public,
            creator_id: entry.creator_id,
            created_at: entry.created_at,
        })
        .await?)
}

pub async fn notify_incident(
    database: &Database,
    app_url: Option<&str>,
    incident: &Incident,
    status: &str,
    message: &str,
) -> Result<()> {
    let channel_ids: Vec<String> = read_json(&incident.channel_ids);
    if channel_ids.is_empty() {
        return Ok(());
    }
    let notice = Notice {
        event: if is_closed(status) { "incident.resolved" } else { "incident.updated" },
        title: format!(
            "[{}] {} — {}",
            describe_kind(incident),
            incident.title,
            describe_status(status)
        ),
        body: message.to_string(),
        url: app_url.map(|url| {
            format!("{}/incidents/{}", url.trim_end_matches('/'), incident.id)
        }),
        data: json!({
            "id": incident.id,
            "kind": incident.kind,
            "title": incident.title,
            "status": status,
            "severity": incident.severity,
        }),
    };
    for integration in database
        .list_integrations()
        .await?
        .into_iter()
        .filter(|integration| channel_ids.contains(&integration.id))
    {
        let database = database.clone();
        let notice = notice.clone();
        tokio::spawn(async move {
            if let Err(error) =
                deliver_notice(&database, &integration, &notice, None).await
            {
                tracing::warn!(integration_id = %integration.id, error = %error, "send an incident update");
            }
        });
    }
    Ok(())
}

pub async fn notify_update(
    database: &Database,
    app_url: Option<&str>,
    incident: &Incident,
    status: &str,
    body: &Value,
) -> Result<()> {
    notify_incident(database, app_url, incident, status, &to_plain_text(body)).await
}

pub struct RuleFire<'a> {
    pub rule: &'a AlertRule,
    pub target_kind: &'static str,
    pub target_id: &'a str,
    pub target_name: &'a str,
    pub integration_ids: &'a [String],
}

pub async fn declare_from_rule(
    database: &Database,
    app_url: Option<&str>,
    fire: RuleFire<'_>,
) -> Result<()> {
    let rule = fire.rule;
    if !rule.declare_incident {
        return Ok(());
    }
    let rule_item = RelatedItem { kind: "rule".to_string(), id: rule.id.clone() };
    let is_declared = database.list_open_incidents().await?.iter().any(|incident| {
        incident.kind == KIND_INCIDENT
            && read_json::<Vec<RelatedItem>>(&incident.related).contains(&rule_item)
    });
    if is_declared {
        return Ok(());
    }
    let now = Utc::now();
    let components: Vec<AffectedComponent> = ["check", "cron"]
        .contains(&fire.target_kind)
        .then(|| AffectedComponent {
            kind: fire.target_kind.to_string(),
            id: fire.target_id.to_string(),
            status: OUTAGE.to_string(),
        })
        .into_iter()
        .collect();
    let related = vec![
        rule_item,
        RelatedItem {
            kind: fire.target_kind.to_string(),
            id: fire.target_id.to_string(),
        },
    ];
    let incident = database
        .create_incident(
            new_id(Prefix::Incident),
            KIND_INCIDENT,
            INVESTIGATING,
            IncidentFields {
                title: format!("{} on {}", rule.name, fire.target_name),
                severity: Some(
                    rule.declare_severity.clone().unwrap_or_else(|| "major".to_string()),
                ),
                lead_id: None,
                started_at: now,
                starts_at: None,
                ends_at: None,
                auto_start: true,
                auto_finish: true,
                remind_before: false,
                components: serde_json::to_value(components).unwrap_or_default(),
                related: serde_json::to_value(related).unwrap_or_default(),
                responders: json!([]),
                channel_ids: serde_json::to_value(fire.integration_ids)
                    .unwrap_or_default(),
                hidden_page_ids: rule.declare_hidden_page_ids.clone(),
            },
        )
        .await?;
    let message = format!(
        "{} fired on {}. Daroyan declared this incident.",
        rule.name, fire.target_name
    );
    add_entry(
        database,
        &incident.id,
        EntryInput {
            kind: AUTOMATIC,
            status: None,
            body: paragraph(&message),
            is_public: false,
            creator_id: None,
            created_at: now,
        },
    )
    .await?;
    tracing::info!(incident_id = %incident.id, rule_id = %rule.id, "incident declared from a rule");
    notify_incident(database, app_url, &incident, INVESTIGATING, &message).await
}

pub fn list_maintained_targets(incidents: &[Incident]) -> Vec<String> {
    incidents
        .iter()
        .filter(|incident| {
            incident.kind == KIND_MAINTENANCE && incident.status == IN_PROGRESS
        })
        .flat_map(|incident| read_json::<Vec<AffectedComponent>>(&incident.components))
        .map(|component| component.id)
        .collect()
}

pub async fn run_maintenance(
    database: &Database,
    app_url: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    for incident in database.list_open_incidents().await? {
        if incident.kind != KIND_MAINTENANCE {
            continue;
        }
        let id = IncidentId::from(incident.id.as_str());
        let (Some(starts_at), Some(ends_at)) = (incident.starts_at, incident.ends_at)
        else {
            continue;
        };
        let change = match incident.status.as_str() {
            SCHEDULED if incident.auto_start && starts_at <= now => {
                Some((IN_PROGRESS, "Maintenance started.", Some(now), None))
            }
            IN_PROGRESS if incident.auto_finish && ends_at <= now => {
                Some((COMPLETED, "Maintenance completed.", None, Some(now)))
            }
            _ => None,
        };
        if let Some((status, message, started_at, resolved_at)) = change {
            let incident = database
                .set_incident_status(&id, status, started_at, resolved_at)
                .await?;
            add_entry(
                database,
                &incident.id,
                EntryInput {
                    kind: UPDATE,
                    status: Some(status.to_string()),
                    body: paragraph(message),
                    is_public: true,
                    creator_id: None,
                    created_at: now,
                },
            )
            .await?;
            notify_incident(database, app_url, &incident, status, message).await?;
            continue;
        }
        let is_due = incident.status == SCHEDULED
            && incident.remind_before
            && incident.reminded_at.is_none()
            && (starts_at - now).num_seconds() <= REMIND_BEFORE_SECS
            && starts_at > now;
        if is_due {
            let incident = database.mark_incident_reminded(&id, Some(now)).await?;
            let message = format!(
                "Starts at {} UTC, in about {} minutes.",
                starts_at.format("%H:%M"),
                ((starts_at - now).num_seconds() + 59) / 60
            );
            notify_incident(database, app_url, &incident, SCHEDULED, &message).await?;
        }
    }
    Ok(())
}

pub fn spawn_incident_scheduler(
    database: Database,
    app_url: Option<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            if let Err(error) =
                run_maintenance(&database, app_url.as_deref(), Utc::now()).await
            {
                tracing::error!(error = ?error, "run maintenance");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_worse_of_the_incident_and_the_monitor() {
        assert_eq!(merge_status("operational", None, false), "operational");
        assert_eq!(merge_status("operational", Some(DEGRADED), false), DEGRADED);
        assert_eq!(merge_status(OUTAGE, Some(DEGRADED), false), OUTAGE);
        assert_eq!(merge_status(DEGRADED, Some(PARTIAL), false), PARTIAL);
        assert_eq!(merge_status(OUTAGE, None, true), MAINTENANCE);
        assert_eq!(merge_status("operational", Some(OUTAGE), true), OUTAGE);
    }
}
