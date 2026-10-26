use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use daroyan_core::analytics::spans::{
    IssueEvent, IssueSummary, ServiceSummary, SpanItem, TraceQuery, TraceSummary,
    get_trace, list_issue_events, list_issues, list_services, list_traces,
    read_last_span_at,
};
use daroyan_core::error::{Error, ErrorName, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

const DEFAULT_WINDOW_MS: i64 = 24 * 3_600_000;
const ISSUE_WINDOW_MS: i64 = 14 * 86_400_000;
const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;
const ISSUE_EVENT_LIMIT: u32 = 50;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_traces))
        .routes(routes!(get_trace_status))
        .routes(routes!(get_trace_services))
        .routes(routes!(get_trace_spans))
        .routes(routes!(get_issues))
        .routes(routes!(get_issue_events))
}

#[derive(Deserialize)]
struct TracesQuery {
    since: Option<i64>,
    until: Option<i64>,
    service: Option<String>,
    status: Option<String>,
    min_duration_ms: Option<i64>,
    q: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

fn since_or(since: Option<i64>, window_ms: i64) -> i64 {
    since.unwrap_or_else(|| Utc::now().timestamp_millis() - window_ms) * 1_000
}

#[utoipa::path(
    get,
    path = "/v1/traces",
    params(
        ("since" = Option<i64>, Query, description = "Only traces with a span at or after this time in ms. Defaults to the last 24 hours"),
        ("until" = Option<i64>, Query, description = "Only traces with a span before this time in ms"),
        ("service" = Option<String>, Query),
        ("status" = Option<String>, Query, description = "`error` keeps traces with an error span"),
        ("min_duration_ms" = Option<i64>, Query),
        ("q" = Option<String>, Query, description = "Matches a span name, an HTTP route or a trace id"),
        ("limit" = Option<u32>, Query),
        ("offset" = Option<u32>, Query)
    ),
    responses((status = 200, body = Vec<TraceSummary>))
)]
async fn get_traces(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<TracesQuery>,
) -> Result<Json<Vec<TraceSummary>>> {
    let search = query.q.map(|q| q.trim().to_string()).filter(|q| !q.is_empty());
    let traces = list_traces(
        &state.analytics,
        TraceQuery {
            since: since_or(query.since, DEFAULT_WINDOW_MS),
            until: query.until.map(|until| until * 1_000),
            service: query.service.filter(|service| !service.is_empty()),
            is_error: query.status.as_deref() == Some("error"),
            min_duration_us: query.min_duration_ms.map(|ms| ms * 1_000),
            search,
            limit: query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: query.offset.unwrap_or_default(),
        },
    )
    .await?;
    Ok(Json(traces))
}

#[derive(Serialize, ToSchema)]
#[schema(as = TraceStatus)]
struct TraceStatusView {
    last_span_at: Option<i64>,
}

#[utoipa::path(get, path = "/v1/traces/status", responses((status = 200, body = TraceStatusView)))]
async fn get_trace_status(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<TraceStatusView>> {
    Ok(Json(TraceStatusView {
        last_span_at: read_last_span_at(&state.analytics).await?,
    }))
}

#[derive(Deserialize)]
struct SinceQuery {
    since: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/traces/services",
    params(("since" = Option<i64>, Query, description = "Time in ms. Defaults to the last 24 hours")),
    responses((status = 200, body = Vec<ServiceSummary>))
)]
async fn get_trace_services(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<SinceQuery>,
) -> Result<Json<Vec<ServiceSummary>>> {
    Ok(Json(
        list_services(&state.analytics, since_or(query.since, DEFAULT_WINDOW_MS)).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/traces/{trace_id}",
    params(("trace_id" = String, Path)),
    responses((status = 200, body = Vec<SpanItem>))
)]
async fn get_trace_spans(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(trace_id): Path<String>,
) -> Result<Json<Vec<SpanItem>>> {
    let spans = get_trace(&state.analytics, trace_id.to_ascii_lowercase()).await?;
    if spans.is_empty() {
        return Err(Error::new(ErrorName::NotFound, "Trace not found"));
    }
    Ok(Json(spans))
}

#[utoipa::path(
    get,
    path = "/v1/issues",
    params(("since" = Option<i64>, Query, description = "Time in ms. Defaults to the last 14 days")),
    responses((status = 200, body = Vec<IssueSummary>))
)]
async fn get_issues(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<SinceQuery>,
) -> Result<Json<Vec<IssueSummary>>> {
    Ok(Json(list_issues(&state.analytics, since_or(query.since, ISSUE_WINDOW_MS)).await?))
}

#[utoipa::path(
    get,
    path = "/v1/issues/{fingerprint}/events",
    params(("fingerprint" = String, Path)),
    responses((status = 200, body = Vec<IssueEvent>))
)]
async fn get_issue_events(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(fingerprint): Path<String>,
) -> Result<Json<Vec<IssueEvent>>> {
    Ok(Json(list_issue_events(&state.analytics, fingerprint, ISSUE_EVENT_LIMIT).await?))
}
