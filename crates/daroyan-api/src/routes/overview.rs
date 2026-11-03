use axum::Json;
use axum::extract::State;
use chrono::Utc;
use daroyan_core::error::Result;
use daroyan_core::fleet_events::{FleetEvents, list_events};
use daroyan_core::upcoming::{Upcoming, list_upcoming};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_overview_events))
        .routes(routes!(list_overview_upcoming))
}

#[utoipa::path(
    get,
    path = "/v1/overview/events",
    description = "What happened across the fleet in the last 24 hours: alerts firing and resolving, uptime checks going down and up, crons failing and recovering, processes exiting and restarting, and agents going offline and online. Counts come in 30 minute buckets. Times are in ms",
    responses((status = 200, body = FleetEvents))
)]
async fn list_overview_events(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<FleetEvents>> {
    Ok(Json(
        list_events(&state.database, &state.analytics, &state.agents, Utc::now()).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/overview/upcoming",
    description = "What is coming up in the next 30 days: disks projected to fill from their growth this week, TLS certificates about to expire, scheduled maintenance, and agents behind the latest version. Items are sorted by when they happen, undated ones last. soon marks items close enough to act on. Times are in ms",
    responses((status = 200, body = Upcoming))
)]
async fn list_overview_upcoming(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Upcoming>> {
    Ok(Json(list_upcoming(&state.database, &state.analytics, Utc::now()).await?))
}
