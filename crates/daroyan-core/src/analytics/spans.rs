use std::sync::Arc;

use duckdb::{Connection, OptionalExt, params};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::trace::v1::span::Event;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::analytics::{Analytics, clamp_future_us};
use crate::error::Result;

const MAX_TEXT_CHARS: usize = 4096;
const MAX_STACKTRACE_CHARS: usize = 16_384;
const UNKNOWN_SERVICE: &str = "unknown_service";
const EXCEPTION_EVENT: &str = "exception";

pub struct SpanRow {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: Arc<str>,
    pub name: String,
    pub kind: &'static str,
    pub started_at: i64,
    pub duration_us: i64,
    pub status: &'static str,
    pub status_message: Option<String>,
    pub http_method: Option<String>,
    pub http_route: Option<String>,
    pub http_status: Option<i32>,
    pub attributes: Option<String>,
    pub resource: Option<Arc<str>>,
    pub events: Option<String>,
    pub received_at: i64,
}

pub struct ExceptionRow {
    pub trace_id: String,
    pub span_id: String,
    pub service: Arc<str>,
    pub fingerprint: String,
    pub kind: String,
    pub message: String,
    pub stacktrace: Option<String>,
    pub occurred_at: i64,
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

fn to_json_value(value: &AnyValue) -> Value {
    match &value.value {
        Some(AnyValueKind::StringValue(text)) => json!(truncate(text, MAX_TEXT_CHARS)),
        Some(AnyValueKind::BoolValue(flag)) => json!(flag),
        Some(AnyValueKind::IntValue(number)) => json!(number),
        Some(AnyValueKind::DoubleValue(number)) => json!(number),
        Some(AnyValueKind::ArrayValue(array)) => {
            Value::Array(array.values.iter().map(to_json_value).collect())
        }
        Some(AnyValueKind::KvlistValue(list)) => to_json_map(&list.values),
        Some(AnyValueKind::BytesValue(bytes)) => json!(hex::encode(bytes)),
        _ => Value::Null,
    }
}

fn to_json_map(attributes: &[KeyValue]) -> Value {
    Value::Object(
        attributes
            .iter()
            .map(|attribute| {
                let value =
                    attribute.value.as_ref().map(to_json_value).unwrap_or(Value::Null);
                (attribute.key.clone(), value)
            })
            .collect::<Map<String, Value>>(),
    )
}

fn find<'a>(attributes: &'a [KeyValue], keys: &[&str]) -> Option<&'a AnyValue> {
    keys.iter().find_map(|key| {
        attributes
            .iter()
            .find(|attribute| attribute.key == *key)
            .and_then(|attribute| attribute.value.as_ref())
    })
}

fn find_text(attributes: &[KeyValue], keys: &[&str]) -> Option<String> {
    match &find(attributes, keys)?.value {
        Some(AnyValueKind::StringValue(text)) if !text.is_empty() => {
            Some(truncate(text, MAX_TEXT_CHARS))
        }
        Some(AnyValueKind::IntValue(number)) => Some(number.to_string()),
        _ => None,
    }
}

fn find_number(attributes: &[KeyValue], keys: &[&str]) -> Option<i64> {
    match &find(attributes, keys)?.value {
        Some(AnyValueKind::IntValue(number)) => Some(*number),
        Some(AnyValueKind::StringValue(text)) => text.parse().ok(),
        Some(AnyValueKind::DoubleValue(number)) => Some(*number as i64),
        _ => None,
    }
}

fn to_hex(bytes: &[u8]) -> Option<String> {
    (!bytes.is_empty() && bytes.iter().any(|byte| *byte != 0)).then(|| hex::encode(bytes))
}

fn read_kind(kind: i32) -> &'static str {
    match kind {
        1 => "internal",
        2 => "server",
        3 => "client",
        4 => "producer",
        5 => "consumer",
        _ => "unspecified",
    }
}

fn read_status(code: i32) -> &'static str {
    match code {
        1 => "ok",
        2 => "error",
        _ => "unset",
    }
}

fn to_micros(nanos: u64) -> i64 {
    i64::try_from(nanos / 1_000).unwrap_or(i64::MAX)
}

pub fn fingerprint(
    service: &str,
    kind: &str,
    message: &str,
    stacktrace: Option<&str>,
) -> String {
    let frame = stacktrace.and_then(|stacktrace| {
        stacktrace.lines().map(str::trim).find(|line| {
            line.starts_with("at ") || line.starts_with("File ") || line.contains(".rs:")
        })
    });
    let anchor = frame.unwrap_or(message);
    let anchor: String = anchor.chars().filter(|char| !char.is_ascii_digit()).collect();
    hex::encode(&Sha256::digest(format!("{service}\n{kind}\n{anchor}").as_bytes())[..8])
}

fn to_exception(
    service: &Arc<str>,
    trace_id: &str,
    span_id: &str,
    event: &Event,
) -> Option<ExceptionRow> {
    if event.name != EXCEPTION_EVENT {
        return None;
    }
    let kind = find_text(&event.attributes, &["exception.type"])
        .unwrap_or_else(|| "Error".to_string());
    let message =
        find_text(&event.attributes, &["exception.message"]).unwrap_or_default();
    let stacktrace = match &find(&event.attributes, &["exception.stacktrace"])?.value {
        Some(AnyValueKind::StringValue(text)) => {
            Some(truncate(text, MAX_STACKTRACE_CHARS))
        }
        _ => None,
    };
    Some(ExceptionRow {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        service: service.clone(),
        fingerprint: fingerprint(service, &kind, &message, stacktrace.as_deref()),
        kind,
        message,
        stacktrace,
        occurred_at: to_micros(event.time_unix_nano),
    })
}

pub fn to_span_rows(
    request: &ExportTraceServiceRequest,
    received_at: i64,
) -> (Vec<SpanRow>, Vec<ExceptionRow>) {
    let mut spans = Vec::new();
    let mut exceptions = Vec::new();
    for resource_spans in &request.resource_spans {
        let resource_attributes = resource_spans
            .resource
            .as_ref()
            .map(|resource| resource.attributes.as_slice())
            .unwrap_or_default();
        let service: Arc<str> = find_text(resource_attributes, &["service.name"])
            .unwrap_or_else(|| UNKNOWN_SERVICE.to_string())
            .into();
        let resource: Option<Arc<str>> =
            serde_json::to_string(&to_json_map(resource_attributes))
                .ok()
                .map(Into::into);
        for scope_spans in &resource_spans.scope_spans {
            for span in &scope_spans.spans {
                let Some(trace_id) = to_hex(&span.trace_id) else { continue };
                let Some(span_id) = to_hex(&span.span_id) else { continue };
                let started_at = to_micros(span.start_time_unix_nano);
                let duration_us = to_micros(
                    span.end_time_unix_nano.saturating_sub(span.start_time_unix_nano),
                );
                let status = span.status.as_ref();
                let events: Vec<Value> = span
                    .events
                    .iter()
                    .map(|event| {
                        json!({
                            "name": event.name,
                            "at": to_micros(event.time_unix_nano),
                            "attributes": to_json_map(&event.attributes),
                        })
                    })
                    .collect();
                exceptions.extend(span.events.iter().filter_map(|event| {
                    to_exception(&service, &trace_id, &span_id, event)
                }));
                spans.push(SpanRow {
                    parent_span_id: to_hex(&span.parent_span_id),
                    service: service.clone(),
                    name: truncate(&span.name, MAX_TEXT_CHARS),
                    kind: read_kind(span.kind),
                    started_at,
                    duration_us,
                    status: read_status(
                        status.map(|status| status.code).unwrap_or_default(),
                    ),
                    status_message: status
                        .map(|status| status.message.clone())
                        .filter(|message| !message.is_empty()),
                    http_method: find_text(
                        &span.attributes,
                        &["http.request.method", "http.method"],
                    ),
                    http_route: find_text(
                        &span.attributes,
                        &["http.route", "url.path", "http.target"],
                    ),
                    http_status: find_number(
                        &span.attributes,
                        &["http.response.status_code", "http.status_code"],
                    )
                    .and_then(|status| i32::try_from(status).ok()),
                    attributes: serde_json::to_string(&to_json_map(&span.attributes))
                        .ok(),
                    resource: resource.clone(),
                    events: (!events.is_empty())
                        .then(|| serde_json::to_string(&events).ok())
                        .flatten(),
                    received_at,
                    trace_id: trace_id.clone(),
                    span_id,
                });
            }
        }
    }
    let now_us = received_at.saturating_mul(1_000);
    for span in &mut spans {
        span.started_at = clamp_future_us(span.started_at, now_us);
    }
    for exception in &mut exceptions {
        exception.occurred_at = clamp_future_us(exception.occurred_at, now_us);
    }
    (spans, exceptions)
}

pub fn append_spans(
    conn: &Connection,
    spans: &[SpanRow],
    exceptions: &[ExceptionRow],
) -> duckdb::Result<()> {
    if spans.is_empty() && exceptions.is_empty() {
        return Ok(());
    }
    let mut appender = conn.appender("spans")?;
    for row in spans {
        appender.append_row(params![
            row.trace_id,
            row.span_id,
            row.parent_span_id,
            &*row.service,
            row.name,
            row.kind,
            row.started_at,
            row.duration_us,
            row.status,
            row.status_message,
            row.http_method,
            row.http_route,
            row.http_status,
            row.attributes,
            row.resource.as_deref(),
            row.events,
            row.received_at,
        ])?;
    }
    appender.flush()?;
    let mut appender = conn.appender("span_exceptions")?;
    for row in exceptions {
        appender.append_row(params![
            row.trace_id,
            row.span_id,
            &*row.service,
            row.fingerprint,
            row.kind,
            row.message,
            row.stacktrace,
            row.occurred_at,
        ])?;
    }
    appender.flush()
}

#[derive(Clone, Debug, Default)]
pub struct TraceQuery {
    pub since: i64,
    pub until: Option<i64>,
    pub service: Option<String>,
    pub is_error: bool,
    pub min_duration_us: Option<i64>,
    pub search: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = TraceSummary)]
pub struct TraceSummary {
    pub trace_id: String,
    pub service: String,
    pub name: String,
    pub started_at: i64,
    pub duration_us: i64,
    pub span_count: i64,
    pub error_count: i64,
    pub http_method: Option<String>,
    pub http_route: Option<String>,
    pub http_status: Option<i32>,
    pub services: Vec<String>,
    pub host: Option<String>,
}

pub async fn list_traces(
    analytics: &Analytics,
    query: TraceQuery,
) -> Result<Vec<TraceSummary>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT trace_id,
                        arg_min(service, (parent_span_id IS NOT NULL, started_at)),
                        arg_min(name, (parent_span_id IS NOT NULL, started_at)),
                        min(started_at),
                        max(started_at + duration_us) - min(started_at),
                        count(*),
                        count(*) FILTER (WHERE status = 'error'),
                        arg_min(http_method, (parent_span_id IS NOT NULL, started_at)),
                        arg_min(http_route, (parent_span_id IS NOT NULL, started_at)),
                        arg_min(http_status, (parent_span_id IS NOT NULL, started_at)),
                        string_agg(DISTINCT service, '\n' ORDER BY service),
                        arg_min(json_extract_string(resource, '$.\"host.name\"'), (parent_span_id IS NOT NULL, started_at))
                 FROM spans
                 WHERE trace_id IN (
                     SELECT DISTINCT trace_id FROM spans
                     WHERE started_at >= ?1
                       AND (?2 IS NULL OR started_at < ?2)
                       AND (?3 IS NULL OR service = ?3)
                       AND (?4 IS NULL OR name ILIKE '%' || ?4 || '%' OR http_route ILIKE '%' || ?4 || '%' OR trace_id = ?4)
                 )
                 GROUP BY trace_id
                 HAVING (NOT ?5 OR count(*) FILTER (WHERE status = 'error') > 0)
                    AND (?6 IS NULL OR max(started_at + duration_us) - min(started_at) >= ?6)
                 ORDER BY min(started_at) DESC, trace_id
                 LIMIT ?7 OFFSET ?8",
            )?;
            let rows = statement.query_map(
                params![
                    query.since,
                    query.until,
                    query.service,
                    query.search,
                    query.is_error,
                    query.min_duration_us,
                    query.limit,
                    query.offset,
                ],
                |row| {
                    Ok(TraceSummary {
                        trace_id: row.get(0)?,
                        service: row.get(1)?,
                        name: row.get(2)?,
                        started_at: row.get(3)?,
                        duration_us: row.get(4)?,
                        span_count: row.get(5)?,
                        error_count: row.get(6)?,
                        http_method: row.get(7)?,
                        http_route: row.get(8)?,
                        http_status: row.get(9)?,
                        services: row
                            .get::<_, Option<String>>(10)?
                            .map(|services| services.split('\n').map(str::to_string).collect())
                            .unwrap_or_default(),
                        host: row.get(11)?,
                    })
                },
            )?;
            rows.collect()
        })
        .await
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = TraceSpan)]
pub struct SpanItem {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub name: String,
    pub kind: String,
    pub started_at: i64,
    pub duration_us: i64,
    pub status: String,
    pub status_message: Option<String>,
    pub http_method: Option<String>,
    pub http_route: Option<String>,
    pub http_status: Option<i32>,
    #[schema(value_type = Object)]
    pub attributes: Value,
    #[schema(value_type = Object)]
    pub resource: Value,
    #[schema(value_type = Vec<Object>)]
    pub events: Value,
}

fn read_json(text: Option<String>) -> Value {
    text.and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

pub async fn get_trace(analytics: &Analytics, trace_id: String) -> Result<Vec<SpanItem>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT span_id, parent_span_id, service, name, kind, started_at, duration_us, status,
                        status_message, http_method, http_route, http_status,
                        attributes::VARCHAR, resource::VARCHAR, events::VARCHAR
                 FROM spans WHERE trace_id = ? ORDER BY started_at, span_id",
            )?;
            let rows = statement.query_map([trace_id], |row| {
                Ok(SpanItem {
                    span_id: row.get(0)?,
                    parent_span_id: row.get(1)?,
                    service: row.get(2)?,
                    name: row.get(3)?,
                    kind: row.get(4)?,
                    started_at: row.get(5)?,
                    duration_us: row.get(6)?,
                    status: row.get(7)?,
                    status_message: row.get(8)?,
                    http_method: row.get(9)?,
                    http_route: row.get(10)?,
                    http_status: row.get(11)?,
                    attributes: read_json(row.get(12)?),
                    resource: read_json(row.get(13)?),
                    events: read_json(row.get(14)?),
                })
            })?;
            rows.collect()
        })
        .await
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = TraceService)]
pub struct ServiceSummary {
    pub service: String,
    pub span_count: i64,
    pub error_count: i64,
    pub p95_us: Option<f64>,
    pub last_seen_at: i64,
}

pub async fn list_services(
    analytics: &Analytics,
    since: i64,
) -> Result<Vec<ServiceSummary>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT service, count(*), count(*) FILTER (WHERE status = 'error'),
                        quantile_cont(duration_us, 0.95) FILTER (WHERE kind = 'server' OR parent_span_id IS NULL),
                        max(started_at)
                 FROM spans WHERE started_at >= ? GROUP BY service ORDER BY service",
            )?;
            let rows = statement.query_map([since], |row| {
                Ok(ServiceSummary {
                    service: row.get(0)?,
                    span_count: row.get(1)?,
                    error_count: row.get(2)?,
                    p95_us: row.get(3)?,
                    last_seen_at: row.get(4)?,
                })
            })?;
            rows.collect()
        })
        .await
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = Issue)]
pub struct IssueSummary {
    pub fingerprint: String,
    pub service: String,
    pub kind: String,
    pub message: String,
    pub count: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub trace_id: String,
}

pub async fn list_issues(analytics: &Analytics, since: i64) -> Result<Vec<IssueSummary>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT fingerprint, arg_max(service, occurred_at), arg_max(type, occurred_at),
                        arg_max(message, occurred_at), count(*), min(occurred_at), max(occurred_at),
                        arg_max(trace_id, occurred_at)
                 FROM span_exceptions WHERE occurred_at >= ?
                 GROUP BY fingerprint ORDER BY max(occurred_at) DESC LIMIT 500",
            )?;
            let rows = statement.query_map([since], |row| {
                Ok(IssueSummary {
                    fingerprint: row.get(0)?,
                    service: row.get(1)?,
                    kind: row.get(2)?,
                    message: row.get(3)?,
                    count: row.get(4)?,
                    first_seen_at: row.get(5)?,
                    last_seen_at: row.get(6)?,
                    trace_id: row.get(7)?,
                })
            })?;
            rows.collect()
        })
        .await
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = IssueEvent)]
pub struct IssueEvent {
    pub trace_id: String,
    pub span_id: String,
    pub message: String,
    pub stacktrace: Option<String>,
    pub occurred_at: i64,
}

pub async fn list_issue_events(
    analytics: &Analytics,
    fingerprint: String,
    limit: u32,
) -> Result<Vec<IssueEvent>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT trace_id, span_id, message, stacktrace, occurred_at
                 FROM span_exceptions WHERE fingerprint = ? ORDER BY occurred_at DESC LIMIT ?",
            )?;
            let rows = statement.query_map(params![fingerprint, limit], |row| {
                Ok(IssueEvent {
                    trace_id: row.get(0)?,
                    span_id: row.get(1)?,
                    message: row.get(2)?,
                    stacktrace: row.get(3)?,
                    occurred_at: row.get(4)?,
                })
            })?;
            rows.collect()
        })
        .await
}

pub async fn list_error_counts(
    analytics: &Analytics,
    since_us: i64,
    bucket_us: i64,
) -> Result<Vec<crate::analytics::server_states::TrendPoint>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT (occurred_at // ?) * ? // 1000 AS at, count(*) FROM span_exceptions \
                 WHERE occurred_at >= ? GROUP BY 1 ORDER BY 1",
            )?;
            statement
                .query_map(params![bucket_us, bucket_us, since_us], |row| {
                    Ok(crate::analytics::server_states::TrendPoint {
                        at: row.get(0)?,
                        value: row.get::<_, i64>(1)? as f64,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn read_last_span_at(analytics: &Analytics) -> Result<Option<i64>> {
    analytics
        .read(|conn| {
            conn.query_row("SELECT max(received_at) FROM spans", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .optional()
            .map(Option::flatten)
        })
        .await
}

#[cfg(test)]
mod tests {
    use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{
        ResourceSpans, ScopeSpans, Span, Status,
    };

    use super::*;

    fn text(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(AnyValueKind::StringValue(value.to_string())),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn reads_spans_and_their_exceptions() {
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![text("service.name", "api")],
                    ..Default::default()
                }),
                scope_spans: vec![ScopeSpans {
                    spans: vec![Span {
                        trace_id: vec![1; 16],
                        span_id: vec![2; 8],
                        name: "GET /users/:id".to_string(),
                        kind: 2,
                        start_time_unix_nano: 1_000_000_000,
                        end_time_unix_nano: 1_250_000_000,
                        attributes: vec![
                            text("http.request.method", "GET"),
                            text("http.route", "/users/:id"),
                            KeyValue {
                                key: "http.response.status_code".to_string(),
                                value: Some(AnyValue {
                                    value: Some(AnyValueKind::IntValue(500)),
                                }),
                                ..Default::default()
                            },
                        ],
                        events: vec![Event {
                            name: "exception".to_string(),
                            time_unix_nano: 1_200_000_000,
                            attributes: vec![
                                text("exception.type", "TypeError"),
                                text("exception.message", "user 42 is undefined"),
                                text(
                                    "exception.stacktrace",
                                    "TypeError: boom\n    at load (src/users.ts:12:5)",
                                ),
                            ],
                            ..Default::default()
                        }],
                        status: Some(Status { code: 2, message: "boom".to_string() }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let (spans, exceptions) = to_span_rows(&request, 5);
        let span = &spans[0];
        assert_eq!(span.trace_id, "01010101010101010101010101010101");
        assert_eq!(span.parent_span_id, None);
        assert_eq!(&*span.service, "api");
        assert_eq!(span.kind, "server");
        assert_eq!((span.started_at, span.duration_us), (1_000_000, 250_000));
        assert_eq!(span.status, "error");
        assert_eq!(span.http_method.as_deref(), Some("GET"));
        assert_eq!(span.http_status, Some(500));
        assert_eq!(exceptions[0].kind, "TypeError");
        assert_eq!(
            exceptions[0].fingerprint,
            fingerprint(
                "api",
                "TypeError",
                "user 7 is undefined",
                Some("x\n at load (src/users.ts:99:1)")
            )
        );
    }

    #[test]
    fn shares_one_resource_across_its_spans() {
        let span = |id: u8| Span {
            trace_id: vec![1; 16],
            span_id: vec![id; 8],
            ..Default::default()
        };
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![
                        text("service.name", "api"),
                        text("process.command_line", &"x".repeat(100_000)),
                    ],
                    ..Default::default()
                }),
                scope_spans: vec![ScopeSpans {
                    spans: vec![span(2), span(3)],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let (spans, _) = to_span_rows(&request, 5);
        assert!(Arc::ptr_eq(&spans[0].service, &spans[1].service));
        assert!(Arc::ptr_eq(
            spans[0].resource.as_ref().unwrap(),
            spans[1].resource.as_ref().unwrap()
        ));
    }
}
