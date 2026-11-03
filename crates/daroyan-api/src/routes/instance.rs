use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;
use crate::views::instance::InstanceView;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_instance))
}

#[utoipa::path(
    get,
    path = "/v1/instance",
    description = "Settings this Daroyan instance reads from its environment: the public URL, how long metrics and logs are kept, and how many owners ALLOWED_EMAILS lists",
    responses((status = 200, body = InstanceView))
)]
async fn get_instance(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Json<InstanceView> {
    let config = &state.config;

    Json(InstanceView {
        app_url: config.app_url.clone(),
        retention_days: config.retention_days,
        log_retention_days: config.log_retention_days,
        allowed_email_count: config.allowed_emails.len(),
    })
}
