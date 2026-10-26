use axum::Json;
use axum::extract::State;
use daroyan_core::error::{Error, ErrorName, Result};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::backup::{BackupStatusView, BackupView};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_backup_status, create_backup))
}

#[utoipa::path(get, path = "/v1/backups", responses((status = 200, body = BackupStatusView)))]
async fn get_backup_status(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Json<BackupStatusView> {
    Json(BackupStatusView {
        is_enabled: state.config.backup_dir.is_some(),
        last_backup: state.analytics.read_last_backup().map(BackupView::from),
    })
}

#[utoipa::path(post, path = "/v1/backups", responses((status = 200, body = BackupView)))]
async fn create_backup(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
) -> Result<Json<BackupView>> {
    let dir = state.config.backup_dir.clone().ok_or_else(|| {
        Error::new(
            ErrorName::ValidationError,
            "Backups are off. Set DUCKDB_BACKUP_DIR to turn them on.",
        )
    })?;
    Ok(Json(state.analytics.back_up(dir).await?.into()))
}
