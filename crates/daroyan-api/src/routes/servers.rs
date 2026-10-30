use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use daroyan_core::analytics::server_events::{EventItem, list_recent_events};
use daroyan_core::analytics::server_states::{MetricPoint, list_metric_points};
use daroyan_core::db::entities::servers::Model as Server;
use daroyan_core::db::ids::ServerId;
use daroyan_core::error::{Error, ErrorName, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::server::{ServerDetailView, ServerItemView};

const CHART_POINTS: i64 = 120;
const MIN_BUCKET_MS: i64 = 10_000;
const EVENT_LIMIT: u32 = 50;
const MAX_NAME_LENGTH: usize = 100;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_servers))
        .routes(routes!(get_server, rename_server, delete_server))
        .routes(routes!(get_server_metrics))
        .routes(routes!(list_server_events))
}

pub async fn find_server(state: &AppState, server_id: &ServerId) -> Result<Server> {
    state
        .database
        .find_server(server_id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Server not found"))
}

#[utoipa::path(get, path = "/v1/servers", responses((status = 200, body = Vec<ServerItemView>)))]
async fn list_servers(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<ServerItemView>>> {
    let servers = state.database.list_servers().await?;
    Ok(Json(
        servers
            .into_iter()
            .map(|server| {
                let is_connected = state.agents.is_connected(&server.id);
                ServerItemView::new(server, is_connected)
            })
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/servers/{server_id}",
    params(("server_id" = String, Path)),
    responses((status = 200, body = ServerDetailView))
)]
async fn get_server(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
) -> Result<Json<ServerDetailView>> {
    let server = find_server(&state, &server_id).await?;
    let is_connected = state.agents.is_connected(&server.id);
    Ok(Json(ServerDetailView::new(server, is_connected)))
}

#[derive(Deserialize, ToSchema)]
struct RenameServerBody {
    name: String,
}

#[utoipa::path(
    patch,
    path = "/v1/servers/{server_id}",
    params(("server_id" = String, Path)),
    request_body = RenameServerBody,
    responses((status = 200, body = ServerDetailView))
)]
async fn rename_server(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(server_id): Path<ServerId>,
    Json(body): Json<RenameServerBody>,
) -> Result<Json<ServerDetailView>> {
    let name = body.name.trim();
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Add a name of up to 100 characters",
        ));
    }
    find_server(&state, &server_id).await?;
    let server = state.database.rename_server(&server_id, name).await?;
    let is_connected = state.agents.is_connected(&server.id);
    Ok(Json(ServerDetailView::new(server, is_connected)))
}

#[utoipa::path(
    delete,
    path = "/v1/servers/{server_id}",
    params(("server_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_server(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(server_id): Path<ServerId>,
) -> Result<StatusCode> {
    if !state.database.delete_server(&server_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Server not found"));
    }
    state.database.remove_target_from_rules(server_id.as_str()).await?;
    state.agents.disconnect(server_id.as_str());
    state.hosts.forget();
    state.analytics.delete_server_rows(server_id.to_string()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MetricsQuery {
    range: Option<MetricRange>,
}

#[derive(Deserialize, Clone, Copy, Default)]
enum MetricRange {
    #[serde(rename = "1h")]
    Hour,
    #[default]
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
}

impl MetricRange {
    fn to_millis(self) -> i64 {
        let hours = match self {
            Self::Hour => 1,
            Self::SixHours => 6,
            Self::Day => 24,
            Self::Week => 24 * 7,
            Self::Month => 24 * 30,
        };
        hours * 3_600_000
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = ServerMetrics)]
struct ServerMetricsView {
    bucket_ms: i64,
    points: Vec<MetricPoint>,
}

#[utoipa::path(
    get,
    path = "/v1/servers/{server_id}/metrics",
    params(
        ("server_id" = String, Path),
        ("range" = Option<String>, Query, description = "1h, 6h, 24h, 7d or 30d")
    ),
    responses((status = 200, body = ServerMetricsView))
)]
async fn get_server_metrics(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<ServerMetricsView>> {
    find_server(&state, &server_id).await?;
    let range_ms = query.range.unwrap_or_default().to_millis();
    let bucket_ms = (range_ms / CHART_POINTS).max(MIN_BUCKET_MS);
    let from_ms = Utc::now().timestamp_millis() - range_ms;
    let points =
        list_metric_points(&state.analytics, &server_id, from_ms, bucket_ms).await?;
    Ok(Json(ServerMetricsView { bucket_ms, points }))
}

#[utoipa::path(
    get,
    path = "/v1/servers/{server_id}/events",
    params(("server_id" = String, Path)),
    responses((status = 200, body = Vec<EventItem>))
)]
async fn list_server_events(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(server_id): Path<ServerId>,
) -> Result<Json<Vec<EventItem>>> {
    find_server(&state, &server_id).await?;
    Ok(Json(list_recent_events(&state.analytics, &server_id, EVENT_LIMIT).await?))
}
