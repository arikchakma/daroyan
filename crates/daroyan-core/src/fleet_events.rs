use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::agents::Agents;
use crate::alerts::rules::format_duration;
use crate::analytics::Analytics;
use crate::analytics::server_events::{FleetEventRow, list_fleet_events};
use crate::commands::{STATUS_FAILED, STATUS_SENT};
use crate::db::Database;
use crate::db::entities::alerts::Model as Alert;
use crate::db::entities::{alerts, cron_failures, uptime_outages};
use crate::error::{Error, Result};

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
const OFFLINE_AFTER_SECS: i64 = 60;
const DUPLICATE_CONDITIONS: [&str; 4] =
    ["offline", "uptime_down", "cron_failed", "cron_missed"];

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
    agents: &Agents,
    now: DateTime<Utc>,
) -> Result<FleetEvents> {
    let now_ms = now.timestamp_millis();
    let first_bucket = (now_ms / BUCKET_MS - (BUCKETS - 1)) * BUCKET_MS;
    let since = DateTime::<Utc>::from_timestamp_millis(first_bucket).unwrap_or(now);
    let stored = async {
        Ok::<_, Error>(tokio::try_join!(
            database.list_alert_rules(),
            database.list_servers(),
            database.list_uptime_checks(),
            database.list_crons(),
            database.list_users(),
            database.list_open_or_since::<alerts::Entity>(
                alerts::Column::StartedAt,
                alerts::Column::ResolvedAt,
                since,
            ),
            database.list_open_or_since::<uptime_outages::Entity>(
                uptime_outages::Column::StartedAt,
                uptime_outages::Column::ResolvedAt,
                since,
            ),
            database.list_open_or_since::<cron_failures::Entity>(
                cron_failures::Column::StartedAt,
                cron_failures::Column::ResolvedAt,
                since,
            ),
            database.list_commands_since(since),
        )?)
    };
    let (
        (rules, servers, checks, crons, users, alert_rows, outages, failures, commands),
        rows,
    ) = tokio::try_join!(
        stored,
        list_fleet_events(analytics, first_bucket, &SERVER_EVENT_KINDS)
    )?;
    let offline_now = servers
        .iter()
        .filter(|server| {
            !agents.is_connected(&server.id)
                && server
                    .last_seen_at
                    .is_some_and(|seen| (now - seen).num_seconds() > OFFLINE_AFTER_SECS)
        })
        .count();
    let duplicate_rules: HashSet<String> = rules
        .iter()
        .filter(|rule| {
            rule.condition["kind"]
                .as_str()
                .is_some_and(|kind| DUPLICATE_CONDITIONS.contains(&kind))
        })
        .map(|rule| rule.id.clone())
        .collect();
    let names = Names {
        rules: to_names(rules, |rule| (rule.id, rule.name)),
        servers: to_names(servers, |server| (server.id, server.name)),
        checks: to_names(checks, |check| (check.id, check.name)),
        crons: to_names(crons, |cron| (cron.id, cron.name)),
        users: to_names(users, |user| (user.id, user.name)),
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

    for alert in alert_rows {
        if duplicate_rules.contains(&alert.rule_id) {
            continue;
        }
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
    for outage in outages {
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
    for failure in failures {
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

    for command in commands {
        let Some(server) = names.servers.get(&command.server_id) else { continue };
        let is_pending = command.status == STATUS_SENT;
        let label = match (command.action.as_str(), is_pending) {
            ("start_process", false) => "Started",
            ("start_process", true) => "Start sent",
            ("stop_process", false) => "Stopped",
            ("stop_process", true) => "Stop sent",
            (_, false) => "Restarted",
            (_, true) => "Restart sent",
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

    for row in &rows {
        let Some(server) = names.servers.get(&row.server_id) else { continue };
        let at = DateTime::<Utc>::from_timestamp_millis(row.at).unwrap_or(now);
        push_process_event(row, server, at, &mut push);
    }
    for outage in find_outages(&rows, now_ms, |id| agents.is_connected(id)) {
        let Some(server) = names.servers.get(&outage.server_id) else { continue };
        let from = DateTime::<Utc>::from_timestamp_millis(outage.from).unwrap_or(now);
        push(
            from,
            Outcome::Problem,
            "Offline",
            server.clone(),
            "agent disconnected".to_string(),
        );
        if let Some(to) = outage.to {
            let to = DateTime::<Utc>::from_timestamp_millis(to).unwrap_or(now);
            push(
                to,
                Outcome::Recovery,
                "Online",
                server.clone(),
                format!("back after {}", lasted(from, to)),
            );
        }
    }
    open += u32::try_from(offline_now).unwrap_or(u32::MAX);

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

#[derive(Debug, PartialEq, Eq)]
struct Outage {
    server_id: String,
    from: i64,
    to: Option<i64>,
}

fn find_outages(
    rows: &[FleetEventRow],
    now_ms: i64,
    is_connected: impl Fn(&str) -> bool,
) -> Vec<Outage> {
    let grace_ms = OFFLINE_AFTER_SECS * 1000;
    let mut offline_since: HashMap<&str, i64> = HashMap::new();
    let mut outages = Vec::new();
    for row in rows {
        match row.kind.as_str() {
            AGENT_DISCONNECTED => {
                offline_since.entry(&row.server_id).or_insert(row.at);
            }
            AGENT_CONNECTED => {
                if let Some(from) = offline_since.remove(row.server_id.as_str())
                    && row.at - from > grace_ms
                {
                    outages.push(Outage {
                        server_id: row.server_id.clone(),
                        from,
                        to: Some(row.at),
                    });
                }
            }
            _ => {}
        }
    }
    outages.extend(
        offline_since
            .into_iter()
            .filter(|&(id, from)| now_ms - from > grace_ms && !is_connected(id))
            .map(|(id, from)| Outage { server_id: id.to_string(), from, to: None }),
    );
    outages
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, at_secs: i64) -> FleetEventRow {
        FleetEventRow {
            server_id: "srv".to_string(),
            at: at_secs * 1000,
            kind: kind.to_string(),
            data: serde_json::Value::Null,
        }
    }

    #[test]
    fn ignores_a_short_reconnect() {
        let rows = [row(AGENT_DISCONNECTED, 0), row(AGENT_CONNECTED, 1)];
        assert!(find_outages(&rows, 120_000, |_| true).is_empty());
    }

    #[test]
    fn reports_a_long_outage() {
        let rows = [row(AGENT_DISCONNECTED, 0), row(AGENT_CONNECTED, 90)];
        let outage = Outage { server_id: "srv".to_string(), from: 0, to: Some(90_000) };
        assert_eq!(find_outages(&rows, 120_000, |_| true), [outage]);
    }

    #[test]
    fn reports_an_open_outage_only_after_the_grace_period() {
        let rows = [row(AGENT_DISCONNECTED, 0)];
        assert!(find_outages(&rows, 30_000, |_| false).is_empty());
        let outage = Outage { server_id: "srv".to_string(), from: 0, to: None };
        assert_eq!(find_outages(&rows, 120_000, |_| false), [outage]);
    }
}
