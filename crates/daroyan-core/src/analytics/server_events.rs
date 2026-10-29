use chrono::Utc;
use daroyan_protocol::report::{Event, EventKind};
use duckdb::{Connection, OptionalExt, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::analytics::{Analytics, clamp_future_ms};
use crate::db::ids::ServerId;
use crate::error::Result;

pub struct EventRow {
    server_id: String,
    at: i64,
    kind: String,
    data: String,
}

pub fn to_event_rows(server_id: &str, events: &[Event]) -> Vec<EventRow> {
    events
        .iter()
        .filter(|event| !matches!(event.kind, EventKind::Unknown))
        .filter_map(|event| {
            let Ok(Value::Object(mut data)) = serde_json::to_value(&event.kind) else {
                return None;
            };
            let kind = data.remove("kind")?.as_str()?.to_string();
            Some(EventRow {
                server_id: server_id.to_string(),
                at: clamp_future_ms(
                    i64::try_from(event.at).unwrap_or(i64::MAX),
                    Utc::now().timestamp_millis(),
                ),
                kind,
                data: Value::Object(data).to_string(),
            })
        })
        .collect()
}

pub fn to_event_row(server_id: &str, at: i64, kind: &str) -> EventRow {
    EventRow {
        server_id: server_id.to_string(),
        at,
        kind: kind.to_string(),
        data: "{}".to_string(),
    }
}

pub fn append_events(conn: &Connection, rows: &[EventRow]) -> duckdb::Result<()> {
    let mut appender = conn.appender("server_events")?;
    for row in rows {
        appender.append_row(params![row.server_id, row.at, row.kind, row.data])?;
    }
    appender.flush()
}

#[derive(Serialize, ToSchema, Debug)]
pub struct EventItem {
    pub at: i64,
    pub kind: String,
    #[schema(value_type = Object)]
    pub data: Value,
}

pub async fn list_recent_events(
    analytics: &Analytics,
    server_id: &ServerId,
    limit: u32,
) -> Result<Vec<EventItem>> {
    let server_id = server_id.to_string();
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT occurred_at, kind, CAST(data AS VARCHAR) FROM server_events \
                 WHERE server_id = ? ORDER BY occurred_at DESC LIMIT ?",
            )?;
            statement
                .query_map(params![server_id, limit], |row| {
                    let data: String = row.get(2)?;
                    Ok(EventItem {
                        at: row.get(0)?,
                        kind: row.get(1)?,
                        data: serde_json::from_str(&data).unwrap_or(Value::Null),
                    })
                })?
                .collect()
        })
        .await
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProcessExit {
    pub code: Option<i32>,
    pub signal: Option<String>,
    pub oom: bool,
    pub stopped: bool,
    pub ran_secs: Option<u64>,
    pub restarts: Option<u32>,
}

pub async fn find_last_exit(
    analytics: &Analytics,
    server_id: String,
    name: String,
    since_ms: i64,
) -> Result<Option<ProcessExit>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT CAST(data AS VARCHAR) FROM server_events \
                 WHERE server_id = ? AND kind = 'process_exited' AND json_extract_string(data, '$.name') = ? \
                 AND occurred_at >= ? ORDER BY occurred_at DESC LIMIT 1",
            )?;

            let data: Option<String> = statement
                .query_row(params![server_id, name, since_ms], |row| row.get(0))
                .optional()?;

            Ok(data.and_then(|data| serde_json::from_str(&data).ok()))
        })
        .await
}

#[derive(Debug)]
pub struct FleetEventRow {
    pub server_id: String,
    pub at: i64,
    pub kind: String,
    pub data: Value,
}

pub async fn list_fleet_events(
    analytics: &Analytics,
    since_ms: i64,
    kinds: &'static [&'static str],
) -> Result<Vec<FleetEventRow>> {
    analytics
        .read(move |conn| {
            let placeholders = vec!["?"; kinds.len()].join(", ");
            let mut statement = conn.prepare(&format!(
                "SELECT server_id, occurred_at, kind, CAST(data AS VARCHAR) FROM server_events \
                 WHERE occurred_at >= ? AND kind IN ({placeholders}) ORDER BY occurred_at"
            ))?;
            let mut values: Vec<&dyn duckdb::ToSql> = vec![&since_ms];
            values.extend(kinds.iter().map(|kind| kind as &dyn duckdb::ToSql));
            statement
                .query_map(values.as_slice(), |row| {
                    let data: String = row.get(3)?;
                    Ok(FleetEventRow {
                        server_id: row.get(0)?,
                        at: row.get(1)?,
                        kind: row.get(2)?,
                        data: serde_json::from_str(&data).unwrap_or(Value::Null),
                    })
                })?
                .collect()
        })
        .await
}
