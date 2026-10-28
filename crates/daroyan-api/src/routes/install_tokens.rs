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
use crate::extract::{AdminUser, CurrentUser};
use crate::views::install_token::{CreatedInstallTokenView, InstallTokenView};

const DEFAULT_NAME: &str = "Install command";
const DEFAULT_EXPIRES_IN_HOURS: i64 = 24;
const DEFAULT_MAX_USES: i32 = 1;
const MAX_EXPIRES_IN_HOURS: i64 = 90 * 24;
const MAX_USES: i32 = 100;
const MAX_NAME_LENGTH: usize = 100;
const HINT_LENGTH: usize = 4;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_install_tokens, create_install_token))
        .routes(routes!(delete_install_token))
}

#[derive(Deserialize, ToSchema)]
struct CreateInstallTokenBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "default_expires_in_hours")]
    expires_in_hours: Option<i64>,
    #[serde(default = "default_max_uses")]
    max_uses: Option<i32>,
}

impl Default for CreateInstallTokenBody {
    fn default() -> Self {
        Self {
            name: None,
            expires_in_hours: default_expires_in_hours(),
            max_uses: default_max_uses(),
        }
    }
}

fn default_expires_in_hours() -> Option<i64> {
    Some(DEFAULT_EXPIRES_IN_HOURS)
}

fn default_max_uses() -> Option<i32> {
    Some(DEFAULT_MAX_USES)
}

#[utoipa::path(get, path = "/v1/install-tokens", responses((status = 200, body = Vec<InstallTokenView>)))]
async fn list_install_tokens(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<InstallTokenView>>> {
    let tokens = state.database.list_install_tokens().await?;
    Ok(Json(tokens.into_iter().map(InstallTokenView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/v1/install-tokens",
    request_body = CreateInstallTokenBody,
    responses((status = 201, body = CreatedInstallTokenView))
)]
async fn create_install_token(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    body: Option<Json<CreateInstallTokenBody>>,
) -> Result<(StatusCode, Json<CreatedInstallTokenView>)> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let name = body.name.as_deref().map(str::trim).unwrap_or_default();
    let name = if name.is_empty() { DEFAULT_NAME } else { name };
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(Error::new(
            ErrorName::ValidationError,
            format!("name must be up to {MAX_NAME_LENGTH} characters"),
        ));
    }
    if body
        .max_uses
        .is_some_and(|max_uses| !(1..=MAX_USES).contains(&max_uses))
    {
        return Err(Error::new(
            ErrorName::ValidationError,
            format!("max_uses must be between 1 and {MAX_USES}, or null for unlimited"),
        ));
    }
    if body
        .expires_in_hours
        .is_some_and(|hours| !(1..=MAX_EXPIRES_IN_HOURS).contains(&hours))
    {
        return Err(Error::new(
            ErrorName::ValidationError,
            format!(
                "expires_in_hours must be between 1 and {MAX_EXPIRES_IN_HOURS}, or null to never expire"
            ),
        ));
    }

    let token = generate_token(INSTALL_TOKEN_PREFIX);
    let stored = state
        .database
        .create_install_token(NewInstallToken {
            id: new_id(Prefix::InstallToken),
            name: name.to_string(),
            token_hash: hash_token(&token),
            hint: token[token.len() - HINT_LENGTH..].to_string(),
            expires_at: body
                .expires_in_hours
                .map(|hours| Utc::now() + Duration::hours(hours)),
            max_uses: body.max_uses,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedInstallTokenView { token, install_token: stored.into() }),
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
