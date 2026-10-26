use axum::Json;
use axum::extract::State;
use chrono::Utc;
use daroyan_core::analytics::server_states::{TrendPoint, list_fleet_cpu};
use daroyan_core::analytics::spans::list_error_counts;
use daroyan_core::error::Result;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

const HOUR_MS: i64 = 3_600_000;
const WINDOW_HOURS: i64 = 24;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_overview_trends))
}

#[derive(Serialize, ToSchema)]
#[schema(as = OverviewTrends)]
struct OverviewTrendsView {
    cpu: Vec<TrendPoint>,
    errors: Vec<TrendPoint>,
}

#[utoipa::path(
    get,
    path = "/v1/overview/trends",
    description = "Hourly fleet CPU and exception counts for the last 24 hours. Times are in ms",
    responses((status = 200, body = OverviewTrendsView))
)]
async fn get_overview_trends(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<OverviewTrendsView>> {
    let now = Utc::now().timestamp_millis();
    let since = (now / HOUR_MS - (WINDOW_HOURS - 1)) * HOUR_MS;
    Ok(Json(OverviewTrendsView {
        cpu: list_fleet_cpu(&state.analytics, since, HOUR_MS).await?,
        errors: list_error_counts(&state.analytics, since * 1_000, HOUR_MS * 1_000)
            .await?,
    }))
}
