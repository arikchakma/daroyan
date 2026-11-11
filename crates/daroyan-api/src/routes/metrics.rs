use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use daroyan_core::analytics::metrics::query::{
    Aggregation, GroupBy, MetricAttribute, MetricFilter, MetricQuery, MetricResult,
    MetricSummary, list_metric_attributes, list_metrics, query_metric,
};
use daroyan_core::error::{Error, ErrorName, Result};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

const CATALOG_WINDOW_MS: i64 = 30 * 86_400_000;
const QUERY_WINDOW_MS: i64 = 3_600_000;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_metrics))
        .routes(routes!(get_metric_attributes))
        .routes(routes!(get_metric_query))
}

#[derive(Deserialize)]
struct CatalogQuery {
    since: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/metrics",
    params(("since" = Option<i64>, Query, description = "Only metrics seen at or after this time in ms. Defaults to the last 30 days")),
    responses((status = 200, body = Vec<MetricSummary>))
)]
async fn get_metrics(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<Vec<MetricSummary>>> {
    let since = query
        .since
        .unwrap_or_else(|| Utc::now().timestamp_millis() - CATALOG_WINDOW_MS);

    Ok(Json(list_metrics(&state.analytics, since).await?))
}

#[utoipa::path(
    get,
    path = "/v1/metrics/{name}/attributes",
    params(("name" = String, Path, description = "Metric name, like http.server.request.duration")),
    responses((status = 200, body = Vec<MetricAttribute>))
)]
async fn get_metric_attributes(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(name): Path<String>,
) -> Result<Json<Vec<MetricAttribute>>> {
    Ok(Json(list_metric_attributes(&state.analytics, name).await?))
}

#[derive(Deserialize)]
struct MetricQueryParams {
    agg: Option<Aggregation>,
    service: Option<String>,
    server_id: Option<String>,
    filter: Option<String>,
    group_by: Option<String>,
    since: Option<i64>,
    until: Option<i64>,
    step_ms: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/metrics/{name}/query",
    params(
        ("name" = String, Path, description = "Metric name, like http.server.request.duration"),
        ("agg" = Option<Aggregation>, Query, description = "Counters: rate (per second), rate_per_minute or increase. Gauges: last (summed across series), avg, min or max. Histograms: p50, p90, p95, p99, avg or count_rate. Defaults to rate_per_minute, last or p95"),
        ("service" = Option<String>, Query),
        ("server_id" = Option<String>, Query),
        ("filter" = Option<String>, Query, description = "Attribute filters separated by commas, like route=/checkout,status!=200"),
        ("group_by" = Option<String>, Query, description = "service, server or an attribute key. At most 50 groups, largest first"),
        ("since" = Option<i64>, Query, description = "Time in ms. Defaults to the last hour"),
        ("until" = Option<i64>, Query, description = "Time in ms. Defaults to now"),
        ("step_ms" = Option<i64>, Query, description = "Bucket size in ms. Defaults to about 120 buckets")
    ),
    responses((status = 200, body = MetricResult))
)]
async fn get_metric_query(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(name): Path<String>,
    Query(params): Query<MetricQueryParams>,
) -> Result<Json<MetricResult>> {
    let until = params.until.unwrap_or_else(|| Utc::now().timestamp_millis());
    let since = params.since.unwrap_or(until - QUERY_WINDOW_MS);
    if since >= until {
        return Err(Error::new(ErrorName::ValidationError, "since must be before until"));
    }

    let query = MetricQuery {
        name,
        aggregation: params.agg,
        service: params.service.filter(|service| !service.is_empty()),
        server_id: params.server_id.filter(|server_id| !server_id.is_empty()),
        filters: params
            .filter
            .as_deref()
            .map(MetricFilter::read_all)
            .unwrap_or_default(),
        group_by: params.group_by.as_deref().and_then(GroupBy::read),
        since,
        until,
        step_ms: params.step_ms.filter(|step| *step > 0),
    };

    Ok(Json(query_metric(&state.analytics, query).await?))
}
