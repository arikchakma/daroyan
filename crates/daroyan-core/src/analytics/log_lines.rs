use std::collections::HashMap;
use std::sync::Arc;

use daroyan_protocol::log::{LogBatch, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN};
use duckdb::types::Value as SqlValue;
use duckdb::{Connection, params, params_from_iter};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::AnyValue;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::logs::v1::LogRecord;
use serde::Serialize;
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::analytics::log_parse::{
    normalize_json_line, parse_text_line, read_level_value, read_level_word,
    take_json_time,
};
use crate::analytics::spans::{
    UNKNOWN_SERVICE, find_text, to_hex, to_json_map, to_json_value,
};
use crate::analytics::{Analytics, clamp_future_ms};
use crate::error::Result;
use crate::servers::hosts::HostIndex;

const MAX_LINES: usize = 5_000;
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_NAME_BYTES: usize = 200;
const AGENT_SERVICE: &str = "Daroyan agent";
const LINE_OVERHEAD: u64 = 64;
pub const OTLP_SERVER_ID: &str = "otlp";
const LEVEL_KEYS: [&str; 4] = ["level", "severity", "status", "log.level"];

pub struct LogRow {
    server_id: String,
    at: i64,
    service: String,
    stream: &'static str,
    severity: u8,
    message: String,
    attributes: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    raw: Option<String>,
    resource: Option<Arc<str>>,
}

impl LogRow {
    pub fn size(&self) -> u64 {
        [
            Some(self.message.as_str()),
            self.attributes.as_deref(),
            self.raw.as_deref(),
            self.resource.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(|text| text.len() as u64)
        .sum::<u64>()
            + LINE_OVERHEAD
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub fn to_log_rows(server_id: &str, batch: LogBatch, now_ms: i64) -> Vec<LogRow> {
    let over = batch.lines.len().saturating_sub(MAX_LINES) as u64;
    let mut rows: Vec<LogRow> = batch
        .lines
        .into_iter()
        .take(MAX_LINES)
        .map(|mut line| {
            let mut at = i64::try_from(line.at).unwrap_or(now_ms);
            let is_json = line.raw.is_some() || !line.attributes.is_empty();
            let original = line.raw.take().unwrap_or_else(|| line.message.clone());
            if !is_json {
                let parsed = parse_text_line(&line.message, line.format);
                at = parsed.at.unwrap_or(at);
                line.severity = parsed.severity.unwrap_or(line.severity);
                line.message = parsed.message;
                line.attributes = parsed.attributes;
            } else {
                at = take_json_time(&mut line.attributes).unwrap_or(at);
                let mut severity = Some(line.severity);
                normalize_json_line(
                    &mut line.attributes,
                    &mut line.message,
                    &mut severity,
                );
                line.severity = severity.unwrap_or(line.severity);
                if !line.format.is_auto() {
                    line.attributes
                        .insert("log.format".into(), line.format.as_str().into());
                }
            }
            LogRow {
                server_id: server_id.to_string(),
                at: clamp_future_ms(at, now_ms),
                service: cut(&line.service, MAX_NAME_BYTES),
                stream: line.stream.as_str(),
                severity: line.severity,
                message: cut(&line.message, MAX_MESSAGE_BYTES),
                attributes: (!line.attributes.is_empty())
                    .then(|| Value::Object(line.attributes).to_string()),
                trace_id: line.trace_id.map(|id| cut(&id, 64)),
                span_id: line.span_id.map(|id| cut(&id, 32)),
                raw: (original != line.message)
                    .then(|| cut(&original, MAX_MESSAGE_BYTES)),
                resource: None,
            }
        })
        .collect();
    let dropped = batch.dropped + over;
    if dropped > 0 {
        rows.push(LogRow {
            server_id: server_id.to_string(),
            at: now_ms,
            service: AGENT_SERVICE.to_string(),
            stream: "stderr",
            severity: SEVERITY_WARN,
            message: format!("Dropped {dropped} log lines over the budget"),
            attributes: None,
            trace_id: None,
            span_id: None,
            raw: None,
            resource: None,
        });
    }
    rows
}

pub fn count_otlp_records(request: &ExportLogsServiceRequest) -> usize {
    request
        .resource_logs
        .iter()
        .flat_map(|resource_logs| &resource_logs.scope_logs)
        .map(|scope_logs| scope_logs.log_records.len())
        .sum()
}

pub fn to_otlp_log_rows(
    request: &ExportLogsServiceRequest,
    hosts: &HostIndex,
    now_ms: i64,
) -> Vec<LogRow> {
    let mut rows = Vec::new();
    for resource_logs in &request.resource_logs {
        let resource = resource_logs
            .resource
            .as_ref()
            .map(|resource| resource.attributes.as_slice())
            .unwrap_or_default();
        let service = find_text(resource, &["service.name"])
            .unwrap_or_else(|| UNKNOWN_SERVICE.to_string());
        let server_id = find_text(resource, &["host.name"])
            .and_then(|host_name| hosts.find(&host_name).map(str::to_string))
            .unwrap_or_else(|| OTLP_SERVER_ID.to_string());
        // Resource attributes are the same on every record from a process, so
        // they are kept apart and shared instead of repeated in each line.
        let resource: Option<Arc<str>> =
            (!resource.is_empty()).then(|| to_json_map(resource).to_string().into());

        for scope_logs in &resource_logs.scope_logs {
            let scope = scope_logs.scope.as_ref().map(|scope| scope.name.as_str());
            for record in &scope_logs.log_records {
                let mut attributes = Map::new();
                if let Some(scope) = scope.filter(|scope| !scope.is_empty()) {
                    attributes.insert("otel.scope.name".into(), scope.into());
                }
                if !record.event_name.is_empty() {
                    attributes
                        .insert("event.name".into(), record.event_name.clone().into());
                }
                if let Value::Object(own) = to_json_map(&record.attributes) {
                    attributes.extend(own);
                }

                let mut row =
                    to_otlp_log_row(record, &server_id, &service, attributes, now_ms);
                row.resource = resource.clone();
                rows.push(row);
            }
        }
    }

    rows
}

fn to_otlp_log_row(
    record: &LogRecord,
    server_id: &str,
    service: &str,
    attributes: Map<String, Value>,
    now_ms: i64,
) -> LogRow {
    let at = [record.time_unix_nano, record.observed_time_unix_nano]
        .into_iter()
        .find(|nanos| *nanos > 0)
        .and_then(|nanos| i64::try_from(nanos / 1_000_000).ok())
        .unwrap_or(now_ms);

    LogRow {
        server_id: server_id.to_string(),
        at: clamp_future_ms(at, now_ms),
        service: cut(service, MAX_NAME_BYTES),
        stream: "otlp",
        severity: read_otlp_severity(record, &attributes),
        message: cut(&read_body(record.body.as_ref()), MAX_MESSAGE_BYTES),
        attributes: (!attributes.is_empty())
            .then(|| Value::Object(attributes).to_string()),
        trace_id: to_hex(&record.trace_id),
        span_id: to_hex(&record.span_id),
        raw: serde_json::to_string(record)
            .ok()
            .map(|raw| cut(&raw, MAX_MESSAGE_BYTES)),
        resource: None,
    }
}

fn read_otlp_severity(record: &LogRecord, attributes: &Map<String, Value>) -> u8 {
    match record.severity_number {
        1..=24 => return record.severity_number as u8,
        25.. => return SEVERITY_ERROR,
        _ => {}
    }

    read_level_word(record.severity_text.trim())
        .or_else(|| {
            LEVEL_KEYS
                .iter()
                .find_map(|key| attributes.get(*key).and_then(read_level_value))
        })
        .unwrap_or(SEVERITY_INFO)
}

fn read_body(body: Option<&AnyValue>) -> String {
    let Some(body) = body else { return String::new() };
    match &body.value {
        Some(AnyValueKind::StringValue(text)) => text.clone(),
        _ => match to_json_value(body) {
            Value::Null => String::new(),
            value => value.to_string(),
        },
    }
}

pub fn append_logs(conn: &Connection, rows: &[LogRow]) -> duckdb::Result<()> {
    let mut appender = conn.appender("log_lines")?;
    for row in rows {
        appender.append_row(params![
            row.server_id,
            row.at,
            row.service,
            row.stream,
            row.severity,
            row.message,
            row.attributes,
            row.trace_id,
            row.span_id,
            row.raw,
            row.resource.as_deref(),
        ])?;
    }
    appender.flush()
}

pub fn read_level(severity: u8) -> &'static str {
    match severity {
        0..SEVERITY_INFO => "debug",
        SEVERITY_INFO..SEVERITY_WARN => "info",
        SEVERITY_WARN..17 => "warn",
        _ => "error",
    }
}

#[derive(Default)]
pub struct LogFilter {
    pub values: Vec<String>,
    pub negate: bool,
}

pub struct SearchTerm {
    pub key: Option<String>,
    pub value: String,
    pub negate: bool,
}

#[derive(Default)]
pub struct LogQuery {
    pub servers: LogFilter,
    pub services: LogFilter,
    pub methods: LogFilter,
    pub status_classes: LogFilter,
    pub min_severity: Option<u8>,
    pub terms: Vec<SearchTerm>,
    pub since: Option<i64>,
    pub until: i64,
    pub offset: u32,
    pub limit: u32,
}

const METHOD_COLUMN: &str = "json_extract_string(attributes, '$.\"http.method\"')";
const STATUS_CLASS_COLUMN: &str =
    "left(json_extract_string(attributes, '$.\"http.status\"'), 1)";

fn push_condition(sql: &mut String, condition: &str, negate: bool) {
    let not = if negate { "NOT " } else { "" };
    sql.push_str(&format!(" AND {not}coalesce({condition}, false)"));
}

fn push_filter(
    sql: &mut String,
    values: &mut Vec<SqlValue>,
    column: &str,
    filter: LogFilter,
) {
    if filter.values.is_empty() {
        return;
    }
    let marks = vec!["?"; filter.values.len()].join(", ");
    push_condition(sql, &format!("{column} IN ({marks})"), filter.negate);
    values.extend(filter.values.into_iter().map(SqlValue::Text));
}

fn push_term(sql: &mut String, values: &mut Vec<SqlValue>, term: SearchTerm) {
    let Some(key) = term.key else {
        push_condition(
            sql,
            "contains(lower(message), lower(?)) OR contains(lower(CAST(attributes AS VARCHAR)), lower(?))",
            term.negate,
        );
        values.push(SqlValue::Text(term.value.clone()));
        values.push(SqlValue::Text(term.value));
        return;
    };
    let column = match key.as_str() {
        "service" | "source" => "service".to_string(),
        "trace_id" | "trace" => "trace_id".to_string(),
        _ => {
            let path = format!("$.\"{}\"", key.replace('"', ""));
            values.extend([SqlValue::Text(path.clone()), SqlValue::Text(path)]);
            "coalesce(json_extract_string(attributes, ?), json_extract_string(resource, ?))"
                .to_string()
        }
    };
    if term.value.contains('*') {
        let pattern = term
            .value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('*', "%");
        push_condition(sql, &format!("{column} LIKE ? ESCAPE '\\'"), term.negate);
        values.push(SqlValue::Text(pattern));
    } else {
        push_condition(sql, &format!("{column} = ?"), term.negate);
        values.push(SqlValue::Text(term.value));
    }
}

#[derive(Serialize, ToSchema, Debug)]
#[schema(as = LogLine)]
pub struct LogItem {
    pub at: i64,
    pub server_id: String,
    pub service: String,
    pub stream: String,
    pub level: String,
    pub severity: u8,
    pub message: String,
    #[schema(value_type = Option<Object>)]
    pub attributes: Option<Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub raw: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub resource: Option<Value>,
}

pub async fn list_logs(analytics: &Analytics, query: LogQuery) -> Result<Vec<LogItem>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT logged_at, server_id, service, stream, severity, message, CAST(attributes AS VARCHAR), trace_id, span_id, raw, CAST(resource AS VARCHAR) \
                 FROM log_lines WHERE logged_at <= ?",
            );
            let mut values: Vec<SqlValue> = vec![SqlValue::BigInt(query.until)];
            if let Some(since) = query.since {
                sql.push_str(" AND logged_at >= ?");
                values.push(SqlValue::BigInt(since));
            }
            push_filter(&mut sql, &mut values, "server_id", query.servers);
            push_filter(&mut sql, &mut values, "service", query.services);
            push_filter(&mut sql, &mut values, METHOD_COLUMN, query.methods);
            push_filter(&mut sql, &mut values, STATUS_CLASS_COLUMN, query.status_classes);
            if let Some(min_severity) = query.min_severity {
                sql.push_str(" AND severity >= ?");
                values.push(SqlValue::UTinyInt(min_severity));
            }
            for term in query.terms {
                push_term(&mut sql, &mut values, term);
            }
            sql.push_str(" ORDER BY logged_at DESC LIMIT ? OFFSET ?");
            values.push(SqlValue::UInt(query.limit));
            values.push(SqlValue::UInt(query.offset));
            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), read_item)?
                .collect()
        })
        .await
}

fn read_item(row: &duckdb::Row<'_>) -> duckdb::Result<LogItem> {
    let severity: u8 = row.get(4)?;
    let attributes: Option<String> = row.get(6)?;
    Ok(LogItem {
        at: row.get(0)?,
        server_id: row.get(1)?,
        service: row.get(2)?,
        stream: row.get(3)?,
        level: read_level(severity).to_string(),
        severity,
        message: row.get(5)?,
        attributes: attributes.and_then(|text| serde_json::from_str(&text).ok()),
        trace_id: row.get(7)?,
        span_id: row.get(8)?,
        raw: row.get(9)?,
        resource: row
            .get::<_, Option<String>>(10)?
            .and_then(|text| serde_json::from_str(&text).ok()),
    })
}

pub async fn list_log_context(
    analytics: &Analytics,
    server_id: String,
    service: String,
    at: i64,
    limit: u32,
) -> Result<Vec<LogItem>> {
    analytics
        .read(move |conn| {
            let columns = "SELECT logged_at, server_id, service, stream, severity, message, CAST(attributes AS VARCHAR), trace_id, span_id, raw, CAST(resource AS VARCHAR) FROM log_lines WHERE server_id = ? AND service = ?";
            let mut before = conn.prepare(&format!(
                "{columns} AND logged_at <= ? ORDER BY logged_at DESC LIMIT ?"
            ))?;
            let mut lines: Vec<LogItem> = before
                .query_map(params![server_id, service, at, limit], read_item)?
                .collect::<duckdb::Result<_>>()?;
            lines.reverse();
            let mut after = conn.prepare(&format!(
                "{columns} AND logged_at > ? ORDER BY logged_at LIMIT ?"
            ))?;
            lines.extend(
                after
                    .query_map(params![server_id, service, at, limit], read_item)?
                    .collect::<duckdb::Result<Vec<_>>>()?,
            );
            Ok(lines)
        })
        .await
}

#[derive(Serialize, ToSchema, Debug)]
#[schema(as = LogSource)]
pub struct LogSource {
    pub server_id: String,
    pub service: String,
}

pub async fn list_log_sources(analytics: &Analytics) -> Result<Vec<LogSource>> {
    analytics
        .read(|conn| {
            let mut statement = conn.prepare(
                "SELECT DISTINCT server_id, service FROM log_lines ORDER BY service, server_id",
            )?;
            statement
                .query_map([], |row| {
                    Ok(LogSource { server_id: row.get(0)?, service: row.get(1)? })
                })?
                .collect()
        })
        .await
}

pub struct LogMatch {
    pub min_severity: u8,
    pub service: Option<String>,
    pub text: Option<String>,
}

fn push_match(sql: &mut String, values: &mut Vec<SqlValue>, matcher: &LogMatch) {
    sql.push_str(" AND severity >= ?");
    values.push(SqlValue::UTinyInt(matcher.min_severity));
    if let Some(service) = &matcher.service {
        sql.push_str(" AND service = ?");
        values.push(SqlValue::Text(service.clone()));
    }
    if let Some(text) = &matcher.text {
        sql.push_str(" AND contains(lower(message), lower(?))");
        values.push(SqlValue::Text(text.clone()));
    }
}

pub async fn count_logs_by_server(
    analytics: &Analytics,
    matcher: LogMatch,
    since: i64,
) -> Result<HashMap<String, u64>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT server_id, count(*) FROM log_lines WHERE logged_at >= ?",
            );
            let mut values = vec![SqlValue::BigInt(since)];
            push_match(&mut sql, &mut values, &matcher);
            sql.push_str(" GROUP BY server_id");
            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect()
        })
        .await
}

pub async fn list_busy_windows(
    analytics: &Analytics,
    server_id: String,
    matcher: LogMatch,
    since: i64,
    window_ms: i64,
    min_count: u64,
) -> Result<Vec<(i64, u64)>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT logged_at // ? AS bucket, count(*) AS found FROM log_lines WHERE server_id = ? AND logged_at >= ?",
            );
            let mut values =
                vec![SqlValue::BigInt(window_ms), SqlValue::Text(server_id), SqlValue::BigInt(since)];
            push_match(&mut sql, &mut values, &matcher);
            sql.push_str(" GROUP BY bucket HAVING count(*) >= ? ORDER BY bucket");
            values.push(SqlValue::UBigInt(min_count));
            let mut statement = conn.prepare(&sql)?;
            let buckets: Vec<(i64, u64)> = statement
                .query_map(params_from_iter(values), |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<duckdb::Result<_>>()?;
            let mut runs = Vec::new();
            let mut previous: Option<i64> = None;
            for (bucket, found) in buckets {
                if previous != Some(bucket - 1) {
                    runs.push((bucket * window_ms, found));
                }
                previous = Some(bucket);
            }
            Ok(runs)
        })
        .await
}

pub async fn list_top_messages(
    analytics: &Analytics,
    server_id: String,
    matcher: LogMatch,
    since: i64,
    limit: u32,
) -> Result<Vec<(String, u64)>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT message, count(*) AS found FROM log_lines WHERE server_id = ? AND logged_at >= ?",
            );
            let mut values = vec![SqlValue::Text(server_id), SqlValue::BigInt(since)];
            push_match(&mut sql, &mut values, &matcher);
            sql.push_str(" GROUP BY message ORDER BY found DESC, max(logged_at) DESC LIMIT ?");
            values.push(SqlValue::UInt(limit));

            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })
        .await
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogRun {
    pub total: u64,
    pub peak: u64,
    pub last_at: Option<i64>,
}

pub async fn summarize_log_run(
    analytics: &Analytics,
    server_id: String,
    matcher: LogMatch,
    since: i64,
    window_ms: i64,
) -> Result<LogRun> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "WITH matched AS (SELECT logged_at FROM log_lines WHERE server_id = ? AND logged_at >= ?",
            );
            let mut values = vec![SqlValue::Text(server_id), SqlValue::BigInt(since)];
            push_match(&mut sql, &mut values, &matcher);
            sql.push_str(
                ") SELECT count(*), max(logged_at), \
                 (SELECT max(found) FROM (SELECT count(*) AS found FROM matched GROUP BY logged_at // ?)) \
                 FROM matched",
            );
            values.push(SqlValue::BigInt(window_ms));

            conn.query_row(&sql, params_from_iter(values), |row| {
                Ok(LogRun {
                    total: row.get(0)?,
                    last_at: row.get(1)?,
                    peak: row.get::<_, Option<u64>>(2)?.unwrap_or_default(),
                })
            })
        })
        .await
}

pub async fn list_last_lines(
    analytics: &Analytics,
    server_id: String,
    service: String,
    since: i64,
    limit: u32,
) -> Result<Vec<(i64, u8, String)>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT logged_at, severity, message FROM log_lines \
                 WHERE server_id = ? AND service = ? AND logged_at >= ? \
                 ORDER BY logged_at DESC LIMIT ?",
            )?;

            let mut lines: Vec<(i64, u8, String)> = statement
                .query_map(params![server_id, service, since, limit], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<duckdb::Result<_>>()?;

            lines.reverse();
            Ok(lines)
        })
        .await
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::{
        LogBatch, LogFormat, LogLine, LogStream, SEVERITY_ERROR,
    };

    use super::{LogFilter, LogQuery, SearchTerm, list_logs, read_level, to_log_rows};
    use crate::analytics::Analytics;

    fn line(at: u64, service: &str, severity: u8, message: &str) -> LogLine {
        LogLine {
            at,
            service: service.to_string(),
            stream: LogStream::Stdout,
            severity,
            message: message.to_string(),
            attributes: serde_json::Map::default(),
            trace_id: None,
            span_id: None,
            raw: None,
            format: LogFormat::Auto,
        }
    }

    #[test]
    fn maps_severity_to_levels() {
        assert_eq!(read_level(5), "debug");
        assert_eq!(read_level(9), "info");
        assert_eq!(read_level(13), "warn");
        assert_eq!(read_level(17), "error");
        assert_eq!(read_level(21), "error");
    }

    #[test]
    fn reads_agent_json_lines_without_reparsing_them() {
        let mut empty = line(1, "api", 9, "hello");
        empty.raw = Some(r#"{"level":"info","msg":"hello"}"#.to_string());
        empty.format = LogFormat::Json;
        let mut mongo = line(1, "db", 9, "Slow query");
        mongo.raw = Some("{}".to_string());
        mongo.attributes = serde_json::json!({
            "t": { "$date": "2026-10-28T01:50:02.123+00:00" },
            "s": "W",
        })
        .as_object()
        .cloned()
        .unwrap();
        let rows = to_log_rows(
            "srv_1",
            LogBatch { lines: vec![empty, mongo], dropped: 0 },
            2_000_000_000_000,
        );
        let attributes = rows[0].attributes.as_deref().unwrap();
        assert!(!attributes.contains("parse_failed"));
        assert!(attributes.contains(r#""log.format":"json""#));
        assert_eq!(rows[1].at, 1_793_152_202_123);
        assert_eq!(rows[1].severity, 13);
    }

    #[test]
    fn keeps_the_raw_line_only_when_parsing_changed_it() {
        let raw = "2026-10-16T01:42:01.546Z [maily-api] INFO: 209.71.83.120 d9f7326e POST /v1/emails 202 84ms";
        let batch = LogBatch {
            lines: vec![line(1, "api", 9, raw), line(2, "api", 9, "plain text line")],
            dropped: 0,
        };
        let rows = to_log_rows("srv_1", batch, 50);
        assert_eq!(rows[0].message, "209.71.83.120 · d9f7326e");
        assert_eq!(rows[0].raw.as_deref(), Some(raw));
        assert_eq!(rows[1].raw, None);
    }

    #[test]
    fn records_dropped_lines() {
        let batch = LogBatch { lines: vec![line(1, "api", 9, "ok")], dropped: 3 };
        let rows = to_log_rows("srv_1", batch, 50);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].message, "Dropped 3 log lines over the budget");
        assert_eq!(rows[1].at, 50);
    }

    #[tokio::test]
    async fn filters_and_pages_newest_first() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-logs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics.run(create_tables).unwrap();
        let batch = LogBatch {
            lines: vec![
                line(1, "api", 9, "GET /health 200"),
                line(2, "api", SEVERITY_ERROR, "SES SendEmail failed"),
                line(3, "worker", 13, "slow job"),
                line(4, "api", 9, "GET /v1/servers 200"),
            ],
            dropped: 0,
        };
        analytics.buffer_logs("srv_1", batch);
        analytics.flush().await.unwrap();

        let all =
            list_logs(&analytics, LogQuery { until: 10, limit: 2, ..Default::default() })
                .await
                .unwrap();
        assert_eq!(all.iter().map(|item| item.at).collect::<Vec<_>>(), [4, 3]);
        let next = list_logs(
            &analytics,
            LogQuery {
                until: 10,
                limit: 2,
                offset: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(next.iter().map(|item| item.at).collect::<Vec<_>>(), [2, 1]);

        let recent = list_logs(
            &analytics,
            LogQuery {
                since: Some(3),
                until: 10,
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(recent.iter().map(|item| item.at).collect::<Vec<_>>(), [4, 3]);

        let errors = list_logs(
            &analytics,
            LogQuery {
                until: 10,
                limit: 50,
                min_severity: Some(13),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(errors.len(), 2);
        let search = list_logs(
            &analytics,
            LogQuery {
                until: 10,
                limit: 50,
                terms: vec![SearchTerm {
                    key: None,
                    value: "sendemail".into(),
                    negate: false,
                }],
                services: LogFilter { values: vec!["api".into()], negate: false },
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].level, "error");
        let excluded = list_logs(
            &analytics,
            LogQuery {
                until: 10,
                limit: 50,
                terms: vec![
                    SearchTerm { key: None, value: "/health".into(), negate: true },
                    SearchTerm {
                        key: Some("service".into()),
                        value: "work*".into(),
                        negate: true,
                    },
                ],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(excluded.iter().map(|item| item.at).collect::<Vec<_>>(), [4, 2]);
        std::fs::remove_dir_all(dir).ok();
    }

    fn create_tables(conn: &duckdb::Connection) -> duckdb::Result<()> {
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0001_server_states.sql"
        ))?;
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0002_log_lines.sql"
        ))?;
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0003_log_raw.sql"
        ))?;
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0011_log_resource.sql"
        ))
    }

    #[tokio::test]
    async fn counts_matching_lines_and_busy_windows() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-log-count-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics.run(create_tables).unwrap();
        let mut lines: Vec<LogLine> = (0..6)
            .map(|i| line(1_000 + i, "api", SEVERITY_ERROR, "SES failed"))
            .collect();
        lines.push(line(1_500, "api", 9, "SES failed"));
        lines.push(line(9_000, "api", SEVERITY_ERROR, "SES failed"));
        analytics.buffer_logs("srv_1", LogBatch { lines, dropped: 0 });
        analytics.flush().await.unwrap();

        let matcher = || super::LogMatch {
            min_severity: 17,
            service: Some("api".into()),
            text: Some("ses".into()),
        };
        let counts = super::count_logs_by_server(&analytics, matcher(), 0).await.unwrap();
        assert_eq!(counts.get("srv_1"), Some(&7));
        let windows =
            super::list_busy_windows(&analytics, "srv_1".into(), matcher(), 0, 1_000, 5)
                .await
                .unwrap();
        assert_eq!(windows, [(1_000, 6)]);
        std::fs::remove_dir_all(dir).ok();
    }
}
