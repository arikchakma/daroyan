use chrono::Utc;
use daroyan_protocol::report::Event;
use duckdb::{Connection, params};
use serde::Serialize;
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
