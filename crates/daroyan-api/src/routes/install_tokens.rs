use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use daroyan_core::db::ids::InstallTokenId;
use daroyan_core::db::queries::install_tokens::NewInstallToken;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::token::{INSTALL_TOKEN_PREFIX, generate_token, hash_token};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::AdminUser;
use crate::views::install_token::InstallTokenView;

const INSTALL_TOKEN_HOURS: i64 = 24;
const MAX_USES: i32 = 100;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_install_token))
        .routes(routes!(delete_install_token))
}

#[derive(Deserialize, ToSchema)]
struct CreateInstallTokenBody {
    max_uses: Option<i32>,
}

#[utoipa::path(
    post,
    path = "/v1/install-tokens",
    request_body = CreateInstallTokenBody,
    responses((status = 201, body = InstallTokenView))
)]
async fn create_install_token(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    body: Option<Json<CreateInstallTokenBody>>,
) -> Result<(StatusCode, Json<InstallTokenView>)> {
    let max_uses = body.and_then(|Json(body)| body.max_uses).unwrap_or(1);
    if !(1..=MAX_USES).contains(&max_uses) {
        return Err(Error::new(
            ErrorName::ValidationError,
            format!("max_uses must be between 1 and {MAX_USES}"),
        ));
    }
    let token = generate_token(INSTALL_TOKEN_PREFIX);
    let stored = state
        .database
        .create_install_token(NewInstallToken {
            id: new_id(Prefix::InstallToken),
            token_hash: hash_token(&token),
            expires_at: Utc::now() + Duration::hours(INSTALL_TOKEN_HOURS),
            max_uses,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(InstallTokenView {
            id: stored.id,
            token,
            expires_at: stored.expires_at,
            max_uses: stored.max_uses,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/install-tokens/{token_id}",
    params(("token_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_install_token(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(token_id): Path<InstallTokenId>,
) -> Result<StatusCode> {
    if !state.database.delete_install_token(&token_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Install token not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
