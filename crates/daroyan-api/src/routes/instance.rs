use axum::Json;
use axum::extract::State;
use daroyan_core::analytics::archive::summarize_archive;
use daroyan_core::error::Result;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;
use crate::views::instance::{ArchiveView, InstanceView};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_instance))
}

#[utoipa::path(
    get,
    path = "/v1/instance",
    description = "Settings this Daroyan instance reads from its environment: the public URL, how long metrics and logs are kept, and the log archive",
    responses((status = 200, body = InstanceView))
)]
async fn get_instance(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<InstanceView>> {
    let config = &state.config;
    let archive = match (&config.archive, &state.archive) {
        (Some(settings), Some(archive)) => {
            let summary = state.analytics.read(summarize_archive).await?;
            let prefix = if settings.prefix.is_empty() {
                String::new()
            } else {
                format!("/{}", settings.prefix)
            };

            Some(ArchiveView {
                url: format!("s3://{}{prefix}", settings.bucket),
                retention_days: settings.retention_days,
                files: summary.files,
                bytes: summary.bytes,
                oldest_at: summary.oldest_at,
                exported_at: summary.exported_at,
                last_error: archive.read_last_error(),
            })
        }
        _ => None,
    };

    Ok(Json(InstanceView {
        app_url: config.app_url.clone(),
        retention_days: config.retention_days,
        log_retention_days: config.log_retention_days,
        archive,
    }))
}
