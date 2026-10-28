use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::alerts::rules::format_duration;
use crate::analytics::Analytics;
use crate::analytics::server_events::{FleetEventRow, list_fleet_events};
use crate::commands::STATUS_FAILED;
use crate::db::Database;
use crate::db::entities::alerts::Model as Alert;
use crate::error::Result;

pub const AGENT_CONNECTED: &str = "agent_connected";
pub const AGENT_DISCONNECTED: &str = "agent_disconnected";

const SERVER_EVENT_KINDS: [&str; 7] = [
    "process_exited",
    "process_changed",
    "process_restarted",
    "process_restart_failed",
    "process_restart_gave_up",
    AGENT_CONNECTED,
    AGENT_DISCONNECTED,
];
const BUCKET_MS: i64 = 30 * 60 * 1000;
const BUCKETS: i64 = 48;
const MAX_EVENTS: usize = 100;

#[derive(Serialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Problem,
    Recovery,
    Neutral,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct FleetEvent {
    pub at: i64,
    pub outcome: Outcome,
    pub label: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize, ToSchema, Debug, Default)]
pub struct EventBucket {
    pub at: i64,
    pub problems: u32,
    pub recoveries: u32,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct FleetEvents {
    pub problems: u32,
    pub recoveries: u32,
    pub open: u32,
    pub buckets: Vec<EventBucket>,
    pub events: Vec<FleetEvent>,
}

struct Names {
    rules: HashMap<String, String>,
    servers: HashMap<String, String>,
    checks: HashMap<String, String>,
    crons: HashMap<String, String>,
    users: HashMap<String, String>,
}

impl Names {
    fn of<'a>(map: &'a HashMap<String, String>, id: &'a str) -> &'a str {
        map.get(id).map_or(id, String::as_str)
    }

    fn alert_target<'a>(&'a self, alert: &'a Alert) -> Option<&'a str> {
        match (&alert.check_id, &alert.cron_id, &alert.server_id) {
            (Some(id), _, _) => Some(Self::of(&self.checks, id)),
            (_, Some(id), _) => Some(Self::of(&self.crons, id)),
            (_, _, Some(id)) => Some(Self::of(&self.servers, id)),
            _ => None,
        }
    }
}

fn lasted(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    format_duration(u32::try_from((to - from).num_seconds()).unwrap_or_default())
}

pub async fn list_events(
    database: &Database,
    analytics: &Analytics,
    now: DateTime<Utc>,
) -> Result<FleetEvents> {
    let now_ms = now.timestamp_millis();
    let first_bucket = (now_ms / BUCKET_MS - (BUCKETS - 1)) * BUCKET_MS;
    let since = DateTime::<Utc>::from_timestamp_millis(first_bucket).unwrap_or(now);
    let names = Names {
        rules: to_names(database.list_alert_rules().await?, |rule| (rule.id, rule.name)),
        servers: to_names(database.list_servers().await?, |server| {
            (server.id, server.name)
        }),
        checks: to_names(database.list_uptime_checks().await?, |check| {
            (check.id, check.name)
        }),
        crons: to_names(database.list_crons().await?, |cron| (cron.id, cron.name)),
        users: to_names(database.list_users().await?, |user| (user.id, user.name)),
    };

    let mut events = Vec::new();
    let mut open = 0;
    let mut push =
        |at: DateTime<Utc>, outcome, label: &str, title: String, detail: String| {
            if at >= since {
                events.push(FleetEvent {
                    at: at.timestamp_millis(),
                    outcome,
                    label: label.to_string(),
                    title,
                    detail,
                });
            }
        };

    for alert in database.list_alerts_since(since).await? {
        let rule = Names::of(&names.rules, &alert.rule_id);
        let title = match names.alert_target(&alert) {
            Some(target) => format!("{rule} on {target}"),
            None => rule.to_string(),
        };
        push(
            alert.started_at,
            Outcome::Problem,
            "Firing",
            title.clone(),
            alert.value.clone(),
        );
        match alert.resolved_at {
            Some(at) => push(
                at,
                Outcome::Recovery,
                "Resolved",
                title,
                format!("lasted {}", lasted(alert.started_at, at)),
            ),
            None => open += 1,
        }
    }
    for outage in database.list_uptime_outages_since(since).await? {
        let title = Names::of(&names.checks, &outage.check_id).to_string();
        push(
            outage.started_at,
            Outcome::Problem,
            "Down",
            title.clone(),
            outage.cause.clone(),
        );
        match outage.resolved_at {
            Some(at) => push(
                at,
                Outcome::Recovery,
                "Up",
                title,
                format!("back after {}", lasted(outage.started_at, at)),
            ),
            None => open += 1,
        }
    }
    for failure in database.list_cron_failures_since(since).await? {
        let title = Names::of(&names.crons, &failure.cron_id).to_string();
        push(
            failure.started_at,
            Outcome::Problem,
            "Failed",
            title.clone(),
            failure.cause.clone(),
        );
        match failure.resolved_at {
            Some(at) => push(
                at,
                Outcome::Recovery,
                "Recovered",
                title,
                format!("after {}", lasted(failure.started_at, at)),
            ),
            None => open += 1,
        }
    }

    for command in database.list_commands_since(since).await? {
        let Some(server) = names.servers.get(&command.server_id) else { continue };
        let label = match command.action.as_str() {
            "start_process" => "Started",
            "stop_process" => "Stopped",
            _ => "Restarted",
        };
        let by = command
            .creator_id
            .as_deref()
            .map_or("from the dashboard".to_string(), |id| {
                format!("by {}", Names::of(&names.users, id))
            });
        let detail = match (command.status.as_str(), &command.error) {
            (STATUS_FAILED, Some(error)) => format!("{by}, failed: {error}"),
            (STATUS_FAILED, None) => format!("{by}, failed"),
            _ => by,
        };
        push(
            command.created_at,
            Outcome::Neutral,
            label,
            format!("{} on {server}", command.name),
            detail,
        );
    }

    let rows = list_fleet_events(analytics, first_bucket, &SERVER_EVENT_KINDS).await?;
    let mut offline_since: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let Some(server) = names.servers.get(&row.server_id) else { continue };
        let at = DateTime::<Utc>::from_timestamp_millis(row.at).unwrap_or(now);
        match row.kind.as_str() {
            AGENT_DISCONNECTED => {
                offline_since.insert(row.server_id.clone(), row.at);
                push(
                    at,
                    Outcome::Problem,
                    "Offline",
                    server.clone(),
                    "agent disconnected".to_string(),
                );
            }
            AGENT_CONNECTED => {
                if let Some(since_ms) = offline_since.remove(&row.server_id) {
                    let from =
                        DateTime::<Utc>::from_timestamp_millis(since_ms).unwrap_or(at);
                    push(
                        at,
                        Outcome::Recovery,
                        "Online",
                        server.clone(),
                        format!("back after {}", lasted(from, at)),
                    );
                }
            }
            _ => push_process_event(&row, server, at, &mut push),
        }
    }
    open += u32::try_from(offline_since.len()).unwrap_or(u32::MAX);

    events.sort_by_key(|event| std::cmp::Reverse(event.at));
    let mut buckets: Vec<EventBucket> = (0..BUCKETS)
        .map(|index| EventBucket {
            at: first_bucket + index * BUCKET_MS,
            ..Default::default()
        })
        .collect();
    for event in &events {
        let index =
            usize::try_from((event.at - first_bucket) / BUCKET_MS).unwrap_or_default();
        if let Some(bucket) = buckets.get_mut(index) {
            match event.outcome {
                Outcome::Problem => bucket.problems += 1,
                Outcome::Recovery => bucket.recoveries += 1,
                Outcome::Neutral => {}
            }
        }
    }
    let count = |outcome| {
        u32::try_from(events.iter().filter(|event| event.outcome == outcome).count())
            .unwrap_or(u32::MAX)
    };
    let problems = count(Outcome::Problem);
    let recoveries = count(Outcome::Recovery);
    events.truncate(MAX_EVENTS);
    Ok(FleetEvents { problems, recoveries, open, buckets, events })
}

fn push_process_event(
    row: &FleetEventRow,
    server: &str,
    at: DateTime<Utc>,
    push: &mut impl FnMut(DateTime<Utc>, Outcome, &str, String, String),
) {
    let name = row.data["name"].as_str().unwrap_or("A process");
    let title = format!("{name} on {server}");
    match row.kind.as_str() {
        "process_exited" => {
            let detail = match row.data["code"].as_i64() {
                Some(code) => format!("exit code {code}"),
                None => "stopped".to_string(),
            };
            push(at, Outcome::Problem, "Exited", title, detail);
        }
        "process_changed" => {
            let old_pid = row.data["old_pid"].as_u64().unwrap_or_default();
            let new_pid = row.data["new_pid"].as_u64().unwrap_or_default();
            push(
                at,
                Outcome::Neutral,
                "Changed",
                title,
                format!("PID {old_pid} → {new_pid}"),
            );
        }
        "process_restarted" => {
            push(at, Outcome::Recovery, "Restarted", title, "auto-restart".to_string());
        }
        "process_restart_failed" => push(
            at,
            Outcome::Problem,
            "Restart failed",
            title,
            "the restart did not start it".to_string(),
        ),
        "process_restart_gave_up" => {
            let attempts = row.data["attempts"].as_u64().unwrap_or_default();
            push(
                at,
                Outcome::Problem,
                "Gave up",
                title,
                format!("still down after {attempts} restarts"),
            );
        }
        _ => {}
    }
}

fn to_names<T>(
    rows: Vec<T>,
    pair: impl Fn(T) -> (String, String),
) -> HashMap<String, String> {
    rows.into_iter().map(pair).collect()
}
