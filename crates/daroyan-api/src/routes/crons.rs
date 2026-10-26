use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::MethodFilter;
use axum::routing::on;
use axum::{Json, Router};
use chrono::Utc;
use daroyan_core::analytics::cron_runs::{
    CronPoint, CronRun, CronRunBar, CronStats, DAY_MS, delete_cron_runs,
    list_cron_points, list_recent_runs, read_cron_stats, summarize_crons,
};
use daroyan_core::crons::schedule::{
    KIND_CRON, KIND_EVERY, Schedule, Tz, parse_expression,
};
use daroyan_core::crons::{MAX_OUTPUT_BYTES, Ping, read_output, read_ping_action};
use daroyan_core::db::entities::cron_failures::Model as CronFailure;
use daroyan_core::db::entities::crons::Model as CronModel;
use daroyan_core::db::ids::{CronId, ServerId};
use daroyan_core::db::queries::crons::CronFields;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::token::generate_secret;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::cron::{CronFailureView, CronView};

const BAR_LIMIT: u32 = 90;
const RECENT_LIMIT: u32 = 20;
const FAILURE_LIMIT: u64 = 20;
const POINT_BUCKET_MS: i64 = 10 * 60 * 1000;
const PING_KEY_LENGTH: usize = 24;
const PREVIEW_RUNS: usize = 3;
const MAX_PING_BODY: usize = 100 * 1024;
const MAX_NAME_LENGTH: usize = 100;
const MAX_COMMAND_LENGTH: usize = 500;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_crons, create_cron))
        .routes(routes!(get_cron, update_cron, delete_cron))
        .routes(routes!(get_cron_stats))
        .routes(routes!(preview_cron_schedule))
}

pub fn ping_router() -> Router<AppState> {
    let methods = MethodFilter::GET.or(MethodFilter::HEAD).or(MethodFilter::POST);
    Router::new()
        .route("/p/{key}", on(methods, ping_cron))
        .route("/p/{key}/{action}", on(methods, ping_cron_with_action))
        .layer(DefaultBodyLimit::max(MAX_PING_BODY))
}

#[derive(Deserialize, ToSchema)]
struct CronBody {
    name: String,
    #[schema(example = "cron")]
    schedule_kind: String,
    expression: Option<String>,
    interval_secs: Option<i32>,
    #[serde(default = "default_time_zone")]
    time_zone: String,
    #[serde(default = "default_grace")]
    grace_secs: i32,
    max_duration_secs: Option<i32>,
    server_id: Option<String>,
    command: Option<String>,
    #[serde(default)]
    paused: bool,
}

#[derive(Serialize, ToSchema)]
#[schema(as = CronStatsView)]
struct StatsView {
    stats: CronStats,
    bars: Vec<CronRunBar>,
    points: Vec<CronPoint>,
    recent: Vec<CronRun>,
    failures: Vec<CronFailureView>,
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct PreviewQuery {
    expression: String,
    time_zone: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = CronPreview)]
struct PreviewView {
    next_runs: Vec<i64>,
}

#[derive(Deserialize)]
struct PingQuery {
    rid: Option<String>,
}

fn default_time_zone() -> String {
    "UTC".to_string()
}

fn default_grace() -> i32 {
    120
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorName::ValidationError, message)
}

fn read_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

async fn read_fields(state: &AppState, body: CronBody) -> Result<CronFields> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(invalid("Add a name of up to 100 characters"));
    }
    let (expression, interval_secs) = match body.schedule_kind.as_str() {
        KIND_CRON => {
            let expression = read_text(body.expression)
                .filter(|expression| {
                    parse_expression(expression).is_some_and(|cron| {
                        cron.find_next_occurrence(&Utc::now(), false).is_ok()
                    })
                })
                .ok_or_else(|| invalid("Enter a cron expression, like */5 * * * *"))?;
            (Some(expression), None)
        }
        KIND_EVERY => {
            let interval = body
                .interval_secs
                .filter(|secs| (60..=30 * 86_400).contains(secs))
                .ok_or_else(|| {
                    invalid("The interval must be between 1 minute and 30 days")
                })?;
            (None, Some(interval))
        }
        _ => return Err(invalid("The schedule must be cron or every")),
    };
    if body.time_zone.parse::<Tz>().is_err() {
        return Err(invalid("The time zone is unknown"));
    }
    if !(60..=86_400).contains(&body.grace_secs) {
        return Err(invalid("The grace period must be between 1 minute and 24 hours"));
    }
    if body
        .max_duration_secs
        .is_some_and(|secs| !(60..=7 * 86_400).contains(&secs))
    {
        return Err(invalid("The max duration must be between 1 minute and 7 days"));
    }
    let server_id = read_text(body.server_id);
    if let Some(id) = &server_id
        && state
            .database
            .find_server(&ServerId::from(id.as_str()))
            .await?
            .is_none()
    {
        return Err(invalid("The server does not exist"));
    }
    let command = read_text(body.command);
    if command
        .as_ref()
        .is_some_and(|command| command.len() > MAX_COMMAND_LENGTH)
    {
        return Err(invalid("The command can be at most 500 characters"));
    }
    Ok(CronFields {
        name,
        schedule_kind: body.schedule_kind,
        expression,
        interval_secs,
        time_zone: body.time_zone,
        grace_secs: body.grace_secs,
        max_duration_secs: body.max_duration_secs,
        server_id,
        command,
        paused: body.paused,
    })
}

async fn find_cron(state: &AppState, id: &CronId) -> Result<CronModel> {
    state
        .database
        .find_cron(id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Cron not found"))
}

async fn to_views(state: &AppState, crons: Vec<CronModel>) -> Result<Vec<CronView>> {
    let mut summaries = summarize_crons(&state.analytics).await?;
    let open = state.database.list_open_cron_failures().await?;
    let open_runs = state.database.list_open_cron_runs().await?;
    let servers: HashMap<String, String> = state
        .database
        .list_servers()
        .await?
        .into_iter()
        .map(|server| (server.id, server.name))
        .collect();
    let now = Utc::now();
    Ok(crons
        .into_iter()
        .map(|cron| {
            let failures: Vec<&CronFailure> =
                open.iter().filter(|failure| failure.cron_id == cron.id).collect();
            let server_name =
                cron.server_id.as_ref().and_then(|id| servers.get(id)).cloned();
            let summary = summaries.remove(&cron.id).unwrap_or_default();
            let running_since = open_runs
                .iter()
                .filter(|run| run.cron_id == cron.id)
                .map(|run| run.started_at)
                .max();
            CronView::new(cron, summary, &failures, server_name, running_since, now)
        })
        .collect())
}

async fn to_view(state: &AppState, cron: CronModel) -> Result<CronView> {
    Ok(to_views(state, vec![cron]).await?.remove(0))
}

#[utoipa::path(get, path = "/v1/crons", responses((status = 200, body = Vec<CronView>)))]
async fn list_crons(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<CronView>>> {
    let crons = state.database.list_crons().await?;
    Ok(Json(to_views(&state, crons).await?))
}

#[utoipa::path(
    post,
    path = "/v1/crons",
    request_body = CronBody,
    responses((status = 201, body = CronView))
)]
async fn create_cron(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<CronBody>,
) -> Result<(StatusCode, Json<CronView>)> {
    let fields = read_fields(&state, body).await?;
    let cron = state
        .database
        .create_cron(new_id(Prefix::Cron), generate_secret(PING_KEY_LENGTH), fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_view(&state, cron).await?)))
}

#[utoipa::path(
    get,
    path = "/v1/crons/{cron_id}",
    params(("cron_id" = String, Path)),
    responses((status = 200, body = CronView))
)]
async fn get_cron(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(cron_id): Path<CronId>,
) -> Result<Json<CronView>> {
    let cron = find_cron(&state, &cron_id).await?;
    Ok(Json(to_view(&state, cron).await?))
}

#[utoipa::path(
    put,
    path = "/v1/crons/{cron_id}",
    params(("cron_id" = String, Path)),
    request_body = CronBody,
    responses((status = 200, body = CronView))
)]
async fn update_cron(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(cron_id): Path<CronId>,
    Json(body): Json<CronBody>,
) -> Result<Json<CronView>> {
    let cron = find_cron(&state, &cron_id).await?;
    let fields = read_fields(&state, body).await?;
    let resumed = cron.paused && !fields.paused;
    let rescheduled = cron.schedule_kind != fields.schedule_kind
        || cron.expression != fields.expression
        || cron.interval_secs != fields.interval_secs
        || cron.time_zone != fields.time_zone;
    let missed_slot_at = (resumed || rescheduled).then(Utc::now);
    let cron = state.database.update_cron(&cron_id, fields, missed_slot_at).await?;
    Ok(Json(to_view(&state, cron).await?))
}

#[utoipa::path(
    delete,
    path = "/v1/crons/{cron_id}",
    params(("cron_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_cron(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(cron_id): Path<CronId>,
) -> Result<StatusCode> {
    if !state.database.delete_cron(&cron_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Cron not found"));
    }
    state.database.remove_target_from_rules(cron_id.as_str()).await?;
    delete_cron_runs(&state.analytics, cron_id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/crons/{cron_id}/stats",
    params(("cron_id" = String, Path)),
    responses((status = 200, body = StatsView))
)]
async fn get_cron_stats(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(cron_id): Path<CronId>,
) -> Result<Json<StatsView>> {
    find_cron(&state, &cron_id).await?;
    let now = Utc::now().timestamp_millis();
    let id = cron_id.to_string();
    let (stats, bars) =
        read_cron_stats(&state.analytics, id.clone(), now, BAR_LIMIT).await?;
    let points =
        list_cron_points(&state.analytics, id.clone(), now - DAY_MS, POINT_BUCKET_MS)
            .await?;
    let recent = list_recent_runs(&state.analytics, id, RECENT_LIMIT).await?;
    let failures = state.database.list_cron_failures(&cron_id, FAILURE_LIMIT).await?;
    Ok(Json(StatsView {
        stats,
        bars,
        points,
        recent,
        failures: failures.into_iter().map(CronFailureView::from).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/crons/preview",
    params(PreviewQuery),
    responses((status = 200, body = PreviewView))
)]
async fn preview_cron_schedule(
    _user: CurrentUser,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<PreviewView>> {
    let cron = parse_expression(&query.expression)
        .ok_or_else(|| invalid("Enter a cron expression, like */5 * * * *"))?;
    let time_zone: Tz = query
        .time_zone
        .as_deref()
        .unwrap_or("UTC")
        .parse()
        .map_err(|_| invalid("The time zone is unknown"))?;
    let schedule = Schedule::Cron { cron: Box::new(cron), time_zone };
    let next_runs = std::iter::successors(schedule.next_after(Utc::now()), |at| {
        schedule.next_after(*at)
    })
    .take(PREVIEW_RUNS)
    .map(|at| at.timestamp_millis())
    .collect();
    Ok(Json(PreviewView { next_runs }))
}

async fn record_ping(
    state: &AppState,
    key: &str,
    action: Option<&str>,
    query: PingQuery,
    body: &[u8],
) -> Result<&'static str> {
    let not_found = || Error::new(ErrorName::NotFound, "Cron not found");
    let (kind, exit_code) = read_ping_action(action).ok_or_else(not_found)?;
    let cron = state.database.find_cron_by_key(key).await?.ok_or_else(not_found)?;
    let ping = Ping {
        kind,
        exit_code,
        run_id: read_text(query.rid).map(|rid| rid.chars().take(64).collect()),
        output: read_output(&body[..body.len().min(MAX_OUTPUT_BYTES * 4)]),
    };
    state.crons.record_ping(&cron, ping, Utc::now()).await?;
    Ok("OK")
}

async fn ping_cron(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<PingQuery>,
    body: Bytes,
) -> Result<&'static str> {
    record_ping(&state, &key, None, query, &body).await
}

async fn ping_cron_with_action(
    State(state): State<AppState>,
    Path((key, action)): Path<(String, String)>,
    Query(query): Query<PingQuery>,
    body: Bytes,
) -> Result<&'static str> {
    record_ping(&state, &key, Some(&action), query, &body).await
}
