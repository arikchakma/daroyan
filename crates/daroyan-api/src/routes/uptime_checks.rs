use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use daroyan_core::analytics::uptime_results::{
    CheckStats, DAY_MS, ResponsePoint, ResultItem, delete_check_results,
    list_recent_results, list_response_points, read_check_stats, summarize_checks,
};
use daroyan_core::db::entities::uptime_checks::Model as UptimeCheck;
use daroyan_core::db::ids::UptimeCheckId;
use daroyan_core::db::queries::uptime_checks::UptimeCheckFields;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::uptime::Url;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::uptime_check::{UptimeCheckView, UptimeOutageView};

const LIST_DAYS: i64 = 30;
const DETAIL_DAYS: i64 = 90;
const RECENT_LIMIT: u32 = 20;
const OUTAGE_LIMIT: u64 = 20;
const POINT_BUCKET_MS: i64 = 10 * 60 * 1000;
const METHODS: [&str; 3] = ["GET", "HEAD", "POST"];
const MAX_NAME_LENGTH: usize = 100;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_uptime_checks, create_uptime_check))
        .routes(routes!(get_uptime_check, update_uptime_check, delete_uptime_check))
        .routes(routes!(run_uptime_check))
        .routes(routes!(get_uptime_check_stats))
}

#[derive(Deserialize, ToSchema)]
struct UptimeCheckBody {
    name: String,
    #[schema(example = "http")]
    kind: String,
    target: String,
    #[serde(default = "default_method")]
    method: String,
    expected_status: Option<i32>,
    body_contains: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_secs: i32,
    #[serde(default = "default_interval")]
    interval_secs: i32,
    #[serde(default)]
    paused: bool,
}

#[derive(Serialize, ToSchema)]
#[schema(as = UptimeCheckStats)]
struct StatsView {
    stats: CheckStats,
    points: Vec<ResponsePoint>,
    recent: Vec<ResultItem>,
    outages: Vec<UptimeOutageView>,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_timeout() -> i32 {
    10
}

fn default_interval() -> i32 {
    60
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorName::ValidationError, message)
}

fn read_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn read_target(kind: &str, target: &str) -> Result<String> {
    let target = target.trim();
    match kind {
        "http" => {
            let url = Url::parse(target).map_err(|_| {
                invalid("Enter a full URL, like https://example.com/health")
            })?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err(invalid("The URL must start with http:// or https://"));
            }
            Ok(url.to_string())
        }
        "tcp" => {
            let (host, port) = target.rsplit_once(':').ok_or_else(|| {
                invalid("Enter a host and port, like smtp.example.com:587")
            })?;
            if host.is_empty() || port.parse::<u16>().map_or(true, |port| port == 0) {
                return Err(invalid("Enter a host and port, like smtp.example.com:587"));
            }
            Ok(target.to_string())
        }
        _ => Err(invalid("The kind must be http or tcp")),
    }
}

async fn list_alerting_checks(state: &AppState) -> Result<HashSet<String>> {
    Ok(state
        .database
        .list_firing_alerts()
        .await?
        .into_iter()
        .filter_map(|alert| alert.check_id)
        .collect())
}

fn read_fields(body: UptimeCheckBody) -> Result<UptimeCheckFields> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(invalid("Add a name of up to 100 characters"));
    }
    let target = read_target(&body.kind, &body.target)?;
    let method = body.method.to_uppercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(invalid("The method must be GET, HEAD or POST"));
    }
    if body.expected_status.is_some_and(|code| !(100..=599).contains(&code)) {
        return Err(invalid("The status must be between 100 and 599"));
    }
    let body_contains = read_text(body.body_contains);
    if body_contains.as_ref().is_some_and(|text| text.len() > 200) {
        return Err(invalid("The body text can be at most 200 characters"));
    }
    if !(30..=86_400).contains(&body.interval_secs) {
        return Err(invalid("The interval must be between 30 seconds and 24 hours"));
    }
    if !(1..=60).contains(&body.timeout_secs) || body.timeout_secs >= body.interval_secs {
        return Err(invalid(
            "The timeout must be 1 to 60 seconds and shorter than the interval",
        ));
    }
    let is_http = body.kind == "http";
    Ok(UptimeCheckFields {
        name,
        kind: body.kind,
        target,
        method,
        expected_status: body.expected_status.filter(|_| is_http),
        body_contains: body_contains.filter(|_| is_http),
        timeout_secs: body.timeout_secs,
        interval_secs: body.interval_secs,
        paused: body.paused,
    })
}

async fn find_uptime_check(state: &AppState, id: &UptimeCheckId) -> Result<UptimeCheck> {
    state
        .database
        .find_uptime_check(id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Uptime check not found"))
}

async fn to_view(
    state: &AppState,
    check: UptimeCheck,
    days: i64,
) -> Result<UptimeCheckView> {
    let mut summaries =
        summarize_checks(&state.analytics, Utc::now().timestamp_millis(), days).await?;
    let summary = summaries.remove(&check.id).unwrap_or_default();
    let alerting = list_alerting_checks(state).await?;
    Ok(UptimeCheckView::new(check, summary, &alerting))
}

#[utoipa::path(get, path = "/v1/uptime-checks", responses((status = 200, body = Vec<UptimeCheckView>)))]
async fn list_uptime_checks(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<UptimeCheckView>>> {
    let checks = state.database.list_uptime_checks().await?;
    let alerting = list_alerting_checks(&state).await?;
    let mut summaries =
        summarize_checks(&state.analytics, Utc::now().timestamp_millis(), LIST_DAYS)
            .await?;
    Ok(Json(
        checks
            .into_iter()
            .map(|check| {
                let summary = summaries.remove(&check.id).unwrap_or_default();
                UptimeCheckView::new(check, summary, &alerting)
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/uptime-checks",
    request_body = UptimeCheckBody,
    responses((status = 201, body = UptimeCheckView))
)]
async fn create_uptime_check(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<UptimeCheckBody>,
) -> Result<(StatusCode, Json<UptimeCheckView>)> {
    let fields = read_fields(body)?;
    let check = state
        .database
        .create_uptime_check(new_id(Prefix::UptimeCheck), fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_view(&state, check, LIST_DAYS).await?)))
}

#[utoipa::path(
    get,
    path = "/v1/uptime-checks/{check_id}",
    params(("check_id" = String, Path)),
    responses((status = 200, body = UptimeCheckView))
)]
async fn get_uptime_check(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(check_id): Path<UptimeCheckId>,
) -> Result<Json<UptimeCheckView>> {
    let check = find_uptime_check(&state, &check_id).await?;
    Ok(Json(to_view(&state, check, DETAIL_DAYS).await?))
}

#[utoipa::path(
    put,
    path = "/v1/uptime-checks/{check_id}",
    params(("check_id" = String, Path)),
    request_body = UptimeCheckBody,
    responses((status = 200, body = UptimeCheckView))
)]
async fn update_uptime_check(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(check_id): Path<UptimeCheckId>,
    Json(body): Json<UptimeCheckBody>,
) -> Result<Json<UptimeCheckView>> {
    find_uptime_check(&state, &check_id).await?;
    let fields = read_fields(body)?;
    let check = state.database.update_uptime_check(&check_id, fields).await?;
    Ok(Json(to_view(&state, check, DETAIL_DAYS).await?))
}

#[utoipa::path(
    delete,
    path = "/v1/uptime-checks/{check_id}",
    params(("check_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_uptime_check(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(check_id): Path<UptimeCheckId>,
) -> Result<StatusCode> {
    if !state.database.delete_uptime_check(&check_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Uptime check not found"));
    }
    state.database.remove_target_from_rules(check_id.as_str()).await?;
    delete_check_results(&state.analytics, check_id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/uptime-checks/{check_id}/run",
    params(("check_id" = String, Path)),
    responses((status = 200, body = ResultItem))
)]
async fn run_uptime_check(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(check_id): Path<UptimeCheckId>,
) -> Result<Json<ResultItem>> {
    let check = find_uptime_check(&state, &check_id).await?;
    Ok(Json(ResultItem::from(state.uptime.run_check_now(&check).await?)))
}

#[utoipa::path(
    get,
    path = "/v1/uptime-checks/{check_id}/stats",
    params(("check_id" = String, Path)),
    responses((status = 200, body = StatsView))
)]
async fn get_uptime_check_stats(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(check_id): Path<UptimeCheckId>,
) -> Result<Json<StatsView>> {
    find_uptime_check(&state, &check_id).await?;
    let now = Utc::now().timestamp_millis();
    let id = check_id.to_string();
    let (stats, _) = read_check_stats(&state.analytics, id.clone(), now, 1).await?;
    let points =
        list_response_points(&state.analytics, id.clone(), now - DAY_MS, POINT_BUCKET_MS)
            .await?;
    let recent = list_recent_results(&state.analytics, id, RECENT_LIMIT).await?;
    let outages = state.database.list_uptime_outages(&check_id, OUTAGE_LIMIT).await?;
    Ok(Json(StatsView {
        stats,
        points,
        recent,
        outages: outages.into_iter().map(UptimeOutageView::from).collect(),
    }))
}
