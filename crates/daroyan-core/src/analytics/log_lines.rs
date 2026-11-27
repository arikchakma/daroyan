use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use daroyan_protocol::log::{
    ACCESS_SERVICE, ACCESS_STREAM, LogBatch, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN,
};
use duckdb::types::Value as SqlValue;
use duckdb::{Connection, params, params_from_iter};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::common::v1::AnyValue;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::logs::v1::LogRecord;
use serde::Serialize;
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::alerts::message::format_bytes;
use crate::analytics::archive::quote_identifier;
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
const VOLUME_CHUNK: usize = 1_000;
pub const MINUTE_MS: i64 = 60_000;
pub const OTLP_SERVER_ID: &str = "otlp";
pub const OTLP_SERVER_NAME: &str = "OpenTelemetry";
const LEVEL_KEYS: [&str; 4] = ["level", "severity", "status", "log.level"];
const STATUS_KEYS: [&str; 3] =
    ["http.status", "http.response.status_code", "http.status_code"];
const METHOD_KEYS: [&str; 2] = ["http.method", "http.request.method"];
const PATH_KEYS: [&str; 2] = ["url.path", "http.target"];
const CLIENT_IP_KEYS: [&str; 2] = ["client.ip", "client.address"];

#[derive(Default)]
pub struct HttpFields {
    status: Option<u16>,
    method: Option<String>,
    path: Option<String>,
    client_ip: Option<String>,
}

impl From<&Map<String, Value>> for HttpFields {
    fn from(attributes: &Map<String, Value>) -> Self {
        let text = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| attributes.get(*key).and_then(Value::as_str))
                .map(|value| cut(value, MAX_NAME_BYTES))
        };
        let status = STATUS_KEYS.iter().find_map(|key| match attributes.get(*key)? {
            Value::Number(number) => number.as_u64()?.try_into().ok(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        });

        Self {
            status,
            method: text(&METHOD_KEYS),
            path: text(&PATH_KEYS),
            client_ip: text(&CLIENT_IP_KEYS),
        }
    }
}

pub struct LogRow {
    pub(crate) server_id: String,
    pub(crate) at: i64,
    pub(crate) service: String,
    pub(crate) stream: &'static str,
    severity: u8,
    message: String,
    pub(crate) attributes: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    raw: Option<String>,
    resource: Option<Arc<str>>,
    http: HttpFields,
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
            let mut raw = None;
            if !is_json {
                let parsed = parse_text_line(&line.message, line.format);
                at = parsed.at.unwrap_or(at);
                line.severity = parsed.severity.unwrap_or(line.severity);
                line.attributes = parsed.attributes;

                let original = std::mem::replace(&mut line.message, parsed.message);
                raw =
                    (original != line.message).then(|| cut(&original, MAX_MESSAGE_BYTES));
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
            let http = HttpFields::from(&line.attributes);
            LogRow {
                server_id: server_id.to_string(),
                at: clamp_future_ms(at, now_ms),
                service: if line.access {
                    ACCESS_SERVICE.to_string()
                } else {
                    cut(&line.service, MAX_NAME_BYTES)
                },
                stream: if line.access { ACCESS_STREAM } else { line.stream.as_str() },
                severity: line.severity,
                message: cut(&line.message, MAX_MESSAGE_BYTES),
                attributes: (!line.attributes.is_empty())
                    .then(|| Value::Object(line.attributes).to_string()),
                trace_id: line.trace_id.map(|id| cut(&id, 64)),
                span_id: line.span_id.map(|id| cut(&id, 32)),
                raw,
                resource: None,
                http,
            }
        })
        .collect();
    let dropped = batch.dropped + over;
    if dropped > 0 {
        let message = format!("Dropped {dropped} log lines over the budget");
        rows.push(make_agent_warning(server_id, now_ms, message, None));
    }

    for drop in batch.over_budget {
        let service = cut(&drop.service, MAX_NAME_BYTES);
        let message = format!(
            "Dropped {} {service} lines over its {} a minute log budget",
            drop.lines,
            format_bytes(drop.budget)
        );
        let attributes = serde_json::json!({ "log.source": service }).to_string();
        rows.push(make_agent_warning(server_id, now_ms, message, Some(attributes)));
    }
    rows
}

fn make_agent_warning(
    server_id: &str,
    now_ms: i64,
    message: String,
    attributes: Option<String>,
) -> LogRow {
    LogRow {
        server_id: server_id.to_string(),
        at: now_ms,
        service: AGENT_SERVICE.to_string(),
        stream: "stderr",
        severity: SEVERITY_WARN,
        message,
        attributes,
        trace_id: None,
        span_id: None,
        raw: None,
        resource: None,
        http: HttpFields::default(),
    }
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
    let http = HttpFields::from(&attributes);

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
        raw: None,
        resource: None,
        http,
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

pub fn append_logs(
    conn: &Connection,
    rows: &[LogRow],
    ingested_at: i64,
) -> duckdb::Result<()> {
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
            ingested_at,
            row.http.status,
            row.http.method,
            row.http.path,
            row.http.client_ip,
        ])?;
    }
    appender.flush()
}

pub fn append_volume(conn: &Connection, rows: &[LogRow]) -> duckdb::Result<()> {
    let mut counts: HashMap<(i64, &str, &str, u8), i64> = HashMap::new();
    for row in rows {
        let minute = row.at.div_euclid(MINUTE_MS) * MINUTE_MS;
        *counts
            .entry((minute, &row.server_id, &row.service, row.severity))
            .or_default() += 1;
    }

    let counts: Vec<_> = counts.into_iter().collect();
    for chunk in counts.chunks(VOLUME_CHUNK) {
        let marks = vec!["(?, ?, ?, ?, ?)"; chunk.len()].join(", ");
        let values =
            chunk
                .iter()
                .flat_map(|((minute, server_id, service, severity), lines)| {
                    [
                        SqlValue::BigInt(*minute),
                        SqlValue::Text(server_id.to_string()),
                        SqlValue::Text(service.to_string()),
                        SqlValue::UTinyInt(*severity),
                        SqlValue::BigInt(*lines),
                    ]
                });
        conn.execute(
            &format!(
                "INSERT INTO log_volume (minute, server_id, service, severity, lines) VALUES {marks} \
                 ON CONFLICT DO UPDATE SET lines = lines + excluded.lines"
            ),
            params_from_iter(values),
        )?;
    }

    Ok(())
}

pub fn read_level(severity: u8) -> &'static str {
    match severity {
        0..SEVERITY_INFO => "debug",
        SEVERITY_INFO..SEVERITY_WARN => "info",
        SEVERITY_WARN..17 => "warn",
        _ => "error",
    }
}

#[derive(Clone, Debug, Default)]
pub struct LogFilter {
    pub values: Vec<String>,
    pub negate: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchField {
    Message,
    Any,
    Key(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchMatch {
    Text(String),
    OneOf(Vec<String>),
    Regex(String),
    Above(f64),
    Below(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchTerm {
    pub field: SearchField,
    pub matches: SearchMatch,
    pub negate: bool,
}

impl SearchTerm {
    pub fn message(text: &str, negate: bool) -> Self {
        Self {
            field: SearchField::Message,
            matches: SearchMatch::Text(text.to_string()),
            negate,
        }
    }

    pub fn key(key: &str, text: &str, negate: bool) -> Self {
        Self {
            field: SearchField::Key(key.to_string()),
            matches: SearchMatch::Text(text.to_string()),
            negate,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LogSearch {
    pub servers: LogFilter,
    pub services: LogFilter,
    pub methods: LogFilter,
    pub status_classes: LogFilter,
    pub min_severity: Option<u8>,
    pub terms: Vec<SearchTerm>,
    pub since: Option<i64>,
    pub until: i64,
}

#[derive(Clone, Debug, Default)]
pub struct LogQuery {
    pub search: LogSearch,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogCursor {
    pub at: i64,
    pub skip: u32,
}

impl LogSearch {
    pub fn can_use_rollup(&self) -> bool {
        self.methods.values.is_empty()
            && self.status_classes.values.is_empty()
            && self.terms.is_empty()
    }
}

impl LogCursor {
    pub fn after(lines: &[LogItem], until: i64, offset: u32) -> Option<Self> {
        let last = lines.last()?.at;
        let same = lines.iter().filter(|line| line.at == last).count() as u32;
        let skip = if last == until { offset + same } else { same };

        Some(Self { at: last, skip })
    }
}

impl fmt::Display for LogCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.at, self.skip)
    }
}

impl FromStr for LogCursor {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || format!("The cursor {text} is not valid");
        let (at, skip) = text.split_once(':').ok_or_else(invalid)?;

        Ok(Self {
            at: at.parse().map_err(|_| invalid())?,
            skip: skip.parse().map_err(|_| invalid())?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = LogScan)]
pub struct LogScan {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Default)]
pub struct LogPage {
    pub lines: Vec<LogItem>,
    pub has_more: bool,
    pub next: Option<LogCursor>,
    pub scan: Option<LogScan>,
}

impl LogPage {
    pub fn new(
        mut lines: Vec<LogItem>,
        query: &LogQuery,
        stopped_at: Option<i64>,
        scan: Option<LogScan>,
    ) -> Self {
        let limit = query.limit as usize;
        let is_full = lines.len() > limit;
        lines.truncate(limit);

        let next = if is_full {
            LogCursor::after(&lines, query.search.until, query.offset)
        } else {
            stopped_at.map(|at| LogCursor { at, skip: 0 })
        };

        Self { has_more: next.is_some(), lines, next, scan }
    }
}

#[derive(Clone, Debug, Default)]
pub enum LogColumns {
    #[default]
    Hot,
    Cold(HashSet<String>),
}

impl LogColumns {
    fn attribute(&self, key: &str, values: &mut Vec<SqlValue>) -> String {
        let path = format!("$.\"{}\"", key.replace('"', ""));
        let Self::Cold(names) = self else {
            values.extend([SqlValue::Text(path.clone()), SqlValue::Text(path)]);
            return "coalesce(json_extract_string(attributes, ?), json_extract_string(resource, ?))"
                .to_string();
        };

        let mut sources = Vec::new();
        for (prefix, rest) in [("a.", "_rest_attributes"), ("r.", "_rest_resource")] {
            let name = format!("{prefix}{key}");
            if names.contains(&name) {
                sources.push(format!("CAST({} AS VARCHAR)", quote_identifier(&name)));
            }
            if names.contains(rest) {
                values.push(SqlValue::Text(path.clone()));
                sources.push(format!("json_extract_string({rest}, ?)"));
            }
        }

        match sources.len() {
            0 => "NULL".to_string(),
            1 => sources.remove(0),
            _ => format!("coalesce({})", sources.join(", ")),
        }
    }

    fn http(&self, column: &str, key: &str, values: &mut Vec<SqlValue>) -> String {
        let fallback = match self {
            Self::Hot => {
                values.push(SqlValue::Text(format!("$.\"{key}\"")));
                "json_extract_string(attributes, ?)".to_string()
            }
            Self::Cold(_) => self.attribute(key, values),
        };

        format!("coalesce({column}, {fallback})")
    }

    fn method(&self, values: &mut Vec<SqlValue>) -> String {
        self.http("http_method", "http.method", values)
    }

    fn status(&self, values: &mut Vec<SqlValue>) -> String {
        self.http("CAST(http_status AS VARCHAR)", "http.status", values)
    }

    fn path(&self, values: &mut Vec<SqlValue>) -> String {
        self.http("url_path", "url.path", values)
    }

    fn client_ip(&self, values: &mut Vec<SqlValue>) -> String {
        self.http("client_ip", "client.ip", values)
    }

    fn message(&self, values: &mut Vec<SqlValue>) -> String {
        let method = self.method(values);
        let path = self.path(values);
        let status = self.status(values);

        format!(
            "CASE WHEN message = '' THEN concat_ws(' ', {method}, {path}, {status}) ELSE message END"
        )
    }

    fn any(&self, values: &mut Vec<SqlValue>) -> String {
        let message = self.message(values);

        match self {
            Self::Hot => {
                format!(
                    "(({message}) || ' ' || coalesce(CAST(attributes AS VARCHAR), ''))"
                )
            }
            Self::Cold(names) if names.is_empty() => format!("({message})"),
            Self::Cold(_) => {
                format!("concat_ws(' ', ({message}), *COLUMNS('^(a|r)\\.|^_rest_'))")
            }
        }
    }
}

fn push_condition(sql: &mut String, condition: &str, negate: bool) {
    let not = if negate { "NOT " } else { "" };
    sql.push_str(&format!(" AND {not}coalesce({condition}, false)"));
}

fn push_filter(
    sql: &mut String,
    values: &mut Vec<SqlValue>,
    column: &str,
    filter: &LogFilter,
) {
    if filter.values.is_empty() {
        return;
    }
    let marks = vec!["?"; filter.values.len()].join(", ");
    push_condition(sql, &format!("{column} IN ({marks})"), filter.negate);
    values.extend(filter.values.iter().cloned().map(SqlValue::Text));
}

fn push_column(
    columns: &LogColumns,
    values: &mut Vec<SqlValue>,
    field: &SearchField,
) -> String {
    match field {
        SearchField::Message => format!("({})", columns.message(values)),
        SearchField::Any => columns.any(values),
        SearchField::Key(key) => match key.as_str() {
            "service" | "source" => "service".to_string(),
            "trace_id" | "trace" => "trace_id".to_string(),
            "http.status" => format!("({})", columns.status(values)),
            "http.method" => format!("({})", columns.method(values)),
            "url.path" => format!("({})", columns.path(values)),
            "client.ip" => format!("({})", columns.client_ip(values)),
            _ => columns.attribute(key, values),
        },
    }
}

fn to_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .replace('*', "%")
}

fn push_term(
    sql: &mut String,
    values: &mut Vec<SqlValue>,
    columns: &LogColumns,
    term: &SearchTerm,
) {
    let is_free_text = matches!(term.field, SearchField::Message | SearchField::Any);
    let column = push_column(columns, values, &term.field);

    let condition = match &term.matches {
        SearchMatch::Text(text) if is_free_text => {
            values.push(SqlValue::Text(text.clone()));
            format!("contains(lower({column}), lower(?))")
        }
        SearchMatch::Text(text) if text.contains('*') => {
            values.push(SqlValue::Text(to_like_pattern(text)));
            format!("{column} ILIKE ? ESCAPE '\\'")
        }
        SearchMatch::Text(text) => {
            values.push(SqlValue::Text(text.clone()));
            format!("lower({column}) = lower(?)")
        }
        SearchMatch::OneOf(items) => {
            let marks = vec!["lower(?)"; items.len()].join(", ");
            values.extend(items.iter().cloned().map(SqlValue::Text));
            format!("lower({column}) IN ({marks})")
        }
        SearchMatch::Regex(pattern) => {
            values.push(SqlValue::Text(pattern.clone()));
            format!("regexp_matches({column}, ?, 'i')")
        }
        SearchMatch::Above(number) => {
            values.push(SqlValue::Double(*number));
            format!("TRY_CAST({column} AS DOUBLE) > ?")
        }
        SearchMatch::Below(number) => {
            values.push(SqlValue::Double(*number));
            format!("TRY_CAST({column} AS DOUBLE) < ?")
        }
    };

    push_condition(sql, &condition, term.negate);
}

pub fn push_search(
    sql: &mut String,
    values: &mut Vec<SqlValue>,
    columns: &LogColumns,
    search: &LogSearch,
) {
    sql.push_str(" WHERE logged_at <= ?");
    values.push(SqlValue::BigInt(search.until));
    if let Some(since) = search.since {
        sql.push_str(" AND logged_at >= ?");
        values.push(SqlValue::BigInt(since));
    }
    push_filter(sql, values, "server_id", &search.servers);
    push_filter(sql, values, "service", &search.services);

    let mut method_values = Vec::new();
    let method = columns.method(&mut method_values);
    if !search.methods.values.is_empty() {
        values.extend(method_values);
        push_filter(sql, values, &method, &search.methods);
    }

    let mut status_values = Vec::new();
    let status = columns.status(&mut status_values);
    if !search.status_classes.values.is_empty() {
        values.extend(status_values);
        push_filter(sql, values, &format!("left({status}, 1)"), &search.status_classes);
    }

    if let Some(min_severity) = search.min_severity {
        sql.push_str(" AND severity >= ?");
        values.push(SqlValue::UTinyInt(min_severity));
    }
    for term in &search.terms {
        push_term(sql, values, columns, term);
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub resource: Option<Value>,
}

pub async fn list_logs(analytics: &Analytics, query: LogQuery) -> Result<Vec<LogItem>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT logged_at, server_id, service, stream, severity, message, CAST(attributes AS VARCHAR), trace_id, span_id, NULL, NULL \
                 FROM log_lines",
            );
            let mut values = Vec::new();
            push_search(&mut sql, &mut values, &LogColumns::Hot, &query.search);
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

pub async fn count_logs(analytics: &Analytics, search: LogSearch) -> Result<u64> {
    analytics
        .read(move |conn| {
            let mut sql = String::from("SELECT count(*) FROM log_lines");
            let mut values = Vec::new();
            push_search(&mut sql, &mut values, &LogColumns::Hot, &search);

            conn.query_row(&sql, params_from_iter(values), |row| row.get(0))
        })
        .await
}

pub async fn read_oldest_log(analytics: &Analytics, since: i64) -> Result<Option<i64>> {
    analytics
        .read(move |conn| {
            conn.query_row(
                "SELECT min(logged_at) FROM log_lines WHERE logged_at >= ?",
                [since],
                |row| row.get(0),
            )
        })
        .await
}

pub fn read_item(row: &duckdb::Row<'_>) -> duckdb::Result<LogItem> {
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

#[derive(Serialize, ToSchema, Debug, Default, Clone, PartialEq, Eq)]
#[schema(as = LogBucket)]
pub struct LogBucket {
    pub at: i64,
    pub info: u64,
    pub warn: u64,
    pub error: u64,
}

pub async fn count_log_buckets(
    analytics: &Analytics,
    search: LogSearch,
    bucket_ms: i64,
) -> Result<Vec<LogBucket>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT logged_at // ? * ? AS bucket, \
                 count(*) FILTER (WHERE severity < ?), \
                 count(*) FILTER (WHERE severity >= ? AND severity < ?), \
                 count(*) FILTER (WHERE severity >= ?) \
                 FROM log_lines",
            );
            let mut values = vec![
                SqlValue::BigInt(bucket_ms),
                SqlValue::BigInt(bucket_ms),
                SqlValue::UTinyInt(SEVERITY_WARN),
                SqlValue::UTinyInt(SEVERITY_WARN),
                SqlValue::UTinyInt(SEVERITY_ERROR),
                SqlValue::UTinyInt(SEVERITY_ERROR),
            ];
            push_search(&mut sql, &mut values, &LogColumns::Hot, &search);
            sql.push_str(" GROUP BY bucket ORDER BY bucket");

            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok(LogBucket {
                        at: row.get(0)?,
                        info: row.get(1)?,
                        warn: row.get(2)?,
                        error: row.get(3)?,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn count_rolled_buckets(
    analytics: &Analytics,
    search: LogSearch,
    bucket_ms: i64,
) -> Result<Vec<LogBucket>> {
    analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT minute // ? * ? AS bucket, \
                 coalesce(sum(lines) FILTER (WHERE severity < ?), 0), \
                 coalesce(sum(lines) FILTER (WHERE severity >= ? AND severity < ?), 0), \
                 coalesce(sum(lines) FILTER (WHERE severity >= ?), 0) \
                 FROM log_volume WHERE minute <= ?",
            );
            let mut values = vec![
                SqlValue::BigInt(bucket_ms),
                SqlValue::BigInt(bucket_ms),
                SqlValue::UTinyInt(SEVERITY_WARN),
                SqlValue::UTinyInt(SEVERITY_WARN),
                SqlValue::UTinyInt(SEVERITY_ERROR),
                SqlValue::UTinyInt(SEVERITY_ERROR),
                SqlValue::BigInt(search.until),
            ];
            if let Some(since) = search.since {
                sql.push_str(" AND minute >= ?");
                values.push(SqlValue::BigInt(since.div_euclid(MINUTE_MS) * MINUTE_MS));
            }
            push_filter(&mut sql, &mut values, "server_id", &search.servers);
            push_filter(&mut sql, &mut values, "service", &search.services);
            if let Some(min_severity) = search.min_severity {
                sql.push_str(" AND severity >= ?");
                values.push(SqlValue::UTinyInt(min_severity));
            }
            sql.push_str(" GROUP BY bucket ORDER BY bucket");

            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok(LogBucket {
                        at: row.get(0)?,
                        info: u64::try_from(row.get::<_, i128>(1)?).unwrap_or_default(),
                        warn: u64::try_from(row.get::<_, i128>(2)?).unwrap_or_default(),
                        error: u64::try_from(row.get::<_, i128>(3)?).unwrap_or_default(),
                    })
                })?
                .collect()
        })
        .await
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

#[derive(Clone)]
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

pub async fn count_logs_by_source(
    analytics: &Analytics,
    matcher: LogMatch,
    since: i64,
) -> Result<HashMap<String, Vec<(String, u64)>>> {
    let rows: Vec<(String, String, u64)> = analytics
        .read(move |conn| {
            let mut sql = String::from(
                "SELECT server_id, service, count(*) FROM log_lines WHERE logged_at >= ?",
            );
            let mut values = vec![SqlValue::BigInt(since)];
            push_match(&mut sql, &mut values, &matcher);
            sql.push_str(" GROUP BY server_id, service ORDER BY service");
            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect()
        })
        .await?;

    let mut counts: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    for (server_id, service, found) in rows {
        counts.entry(server_id).or_default().push((service, found));
    }
    Ok(counts)
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
        LogBatch, LogDrop, LogFormat, LogLine, LogStream, SEVERITY_ERROR,
    };

    use super::{
        HttpFields, LogBucket, LogFilter, LogQuery, LogSearch, SearchField, SearchMatch,
        SearchTerm, count_log_buckets, count_rolled_buckets, list_logs, read_level,
        to_log_rows,
    };
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
            access: false,
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
            LogBatch { lines: vec![empty, mongo], ..LogBatch::default() },
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
            ..LogBatch::default()
        };
        let rows = to_log_rows("srv_1", batch, 50);
        assert_eq!(rows[0].message, "209.71.83.120 · d9f7326e");
        assert_eq!(rows[0].raw.as_deref(), Some(raw));
        assert_eq!(rows[1].raw, None);
    }

    #[test]
    fn stores_json_lines_once() {
        let mut json = line(1, "api", 9, "request completed");
        json.raw = Some(
            r#"{"level":"info","msg":"request completed","route":"/v1/links"}"#
                .to_string(),
        );
        json.attributes = serde_json::json!({ "route": "/v1/links" })
            .as_object()
            .cloned()
            .unwrap();
        let rows = to_log_rows(
            "srv_1",
            LogBatch { lines: vec![json], ..LogBatch::default() },
            50,
        );

        assert_eq!(rows[0].message, "request completed");
        assert!(rows[0].attributes.as_deref().unwrap().contains("/v1/links"));
        assert_eq!(rows[0].raw, None);
    }

    #[test]
    fn promotes_http_fields_from_attributes() {
        let attributes = serde_json::json!({
            "http.response.status_code": "404",
            "http.request.method": "GET",
            "url.path": "/v1/links",
            "client.address": "10.0.0.1",
        });
        let http = HttpFields::from(attributes.as_object().unwrap());
        assert_eq!(http.status, Some(404));
        assert_eq!(http.method.as_deref(), Some("GET"));
        assert_eq!(http.path.as_deref(), Some("/v1/links"));
        assert_eq!(http.client_ip.as_deref(), Some("10.0.0.1"));

        let odd = serde_json::json!({ "http.status": 70_000, "http.method": 1 });
        let http = HttpFields::from(odd.as_object().unwrap());
        assert_eq!((http.status, http.method), (None, None));
    }

    #[test]
    fn records_dropped_lines() {
        let batch = LogBatch {
            lines: vec![line(1, "api", 9, "ok")],
            dropped: 3,
            ..LogBatch::default()
        };
        let rows = to_log_rows("srv_1", batch, 50);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].message, "Dropped 3 log lines over the budget");
        assert_eq!(rows[1].at, 50);
    }

    #[test]
    fn names_the_source_over_its_budget() {
        let batch = LogBatch {
            over_budget: vec![LogDrop {
                service: "Caddy".to_string(),
                lines: 120,
                budget: 128 * 1024,
            }],
            ..LogBatch::default()
        };
        let rows = to_log_rows("srv_1", batch, 50);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].service, "Daroyan agent");
        assert_eq!(
            rows[0].message,
            "Dropped 120 Caddy lines over its 128 KB a minute log budget"
        );
        assert_eq!(rows[0].attributes.as_deref(), Some(r#"{"log.source":"Caddy"}"#));
    }

    fn search(until: i64) -> LogSearch {
        LogSearch { until, ..Default::default() }
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
            ..LogBatch::default()
        };
        analytics.buffer_logs("srv_1", batch);
        analytics.flush().await.unwrap();

        let all = list_logs(
            &analytics,
            LogQuery { search: search(10), limit: 2, ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(all.iter().map(|item| item.at).collect::<Vec<_>>(), [4, 3]);
        let next =
            list_logs(&analytics, LogQuery { search: search(10), limit: 2, offset: 2 })
                .await
                .unwrap();
        assert_eq!(next.iter().map(|item| item.at).collect::<Vec<_>>(), [2, 1]);

        let recent = list_logs(
            &analytics,
            LogQuery {
                search: LogSearch { since: Some(3), until: 10, ..Default::default() },
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
                search: LogSearch {
                    until: 10,
                    min_severity: Some(13),
                    ..Default::default()
                },
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(errors.len(), 2);
        let found = list_logs(
            &analytics,
            LogQuery {
                search: LogSearch {
                    until: 10,
                    terms: vec![SearchTerm::message("sendemail", false)],
                    services: LogFilter { values: vec!["api".into()], negate: false },
                    ..Default::default()
                },
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].level, "error");
        let excluded = list_logs(
            &analytics,
            LogQuery {
                search: LogSearch {
                    until: 10,
                    terms: vec![
                        SearchTerm::message("/health", true),
                        SearchTerm::key("service", "work*", true),
                    ],
                    ..Default::default()
                },
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(excluded.iter().map(|item| item.at).collect::<Vec<_>>(), [4, 2]);

        let find = |term: SearchTerm| {
            let analytics = analytics.clone();
            async move {
                let search =
                    LogSearch { until: 10, terms: vec![term], ..Default::default() };
                list_logs(
                    &analytics,
                    LogQuery { search, limit: 50, ..Default::default() },
                )
                .await
                .unwrap()
                .iter()
                .map(|item| item.at)
                .collect::<Vec<_>>()
            }
        };
        let term = |field: SearchField, matches: SearchMatch, negate: bool| SearchTerm {
            field,
            matches,
            negate,
        };
        let status = || SearchField::Key("http.status".into());
        let path = || SearchField::Key("url.path".into());

        assert_eq!(find(SearchTerm::message("get /HEALTH", false)).await, [1]);
        assert_eq!(
            find(term(
                SearchField::Message,
                SearchMatch::Regex("send.*fail".into()),
                false
            ))
            .await,
            [2]
        );
        assert_eq!(
            find(term(SearchField::Any, SearchMatch::Text("v1/servers".into()), false))
                .await,
            [4]
        );
        assert_eq!(find(term(status(), SearchMatch::Above(199.0), false)).await, [4, 1]);
        assert_eq!(find(term(status(), SearchMatch::Below(200.0), false)).await, [0; 0]);
        assert_eq!(
            find(term(
                SearchField::Key("service".into()),
                SearchMatch::OneOf(vec!["Worker".into(), "web".into()]),
                false,
            ))
            .await,
            [3]
        );
        assert_eq!(find(SearchTerm::key("url.path", "*", false)).await, [4, 1]);
        assert_eq!(find(term(path(), SearchMatch::Text("*".into()), true)).await, [3, 2]);

        let rolled = count_rolled_buckets(&analytics, search(10), 60_000).await.unwrap();
        assert_eq!(rolled, [LogBucket { at: 0, info: 2, warn: 1, error: 1 }]);

        let buckets = count_log_buckets(&analytics, search(10), 2).await.unwrap();
        assert_eq!(
            buckets,
            [
                LogBucket { at: 0, info: 1, warn: 0, error: 0 },
                LogBucket { at: 2, info: 0, warn: 1, error: 1 },
                LogBucket { at: 4, info: 1, warn: 0, error: 0 },
            ]
        );
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
        ))?;
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0012_log_archive.sql"
        ))?;
        conn.execute_batch(include_str!(
            "../../../daroyan-migration/duckdb/0014_log_volume.sql"
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
        analytics.buffer_logs("srv_1", LogBatch { lines, ..LogBatch::default() });
        analytics.flush().await.unwrap();

        let matcher = || super::LogMatch {
            min_severity: 17,
            service: Some("api".into()),
            text: Some("ses".into()),
        };
        let counts = super::count_logs_by_source(&analytics, matcher(), 0).await.unwrap();
        assert_eq!(counts.get("srv_1"), Some(&vec![("api".to_string(), 7)]));
        let windows =
            super::list_busy_windows(&analytics, "srv_1".into(), matcher(), 0, 1_000, 5)
                .await
                .unwrap();
        assert_eq!(windows, [(1_000, 6)]);
        std::fs::remove_dir_all(dir).ok();
    }
}
