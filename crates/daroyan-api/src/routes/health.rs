use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_health))
}

#[derive(Serialize, ToSchema)]
struct Health {
    ok: bool,
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = Health)))]
async fn get_health() -> Json<Health> {
    Json(Health { ok: true })
}
