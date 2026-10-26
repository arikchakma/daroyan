use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use daroyan_core::db::entities::ingest_tokens::Model as IngestToken;
use daroyan_core::db::ids::IngestTokenId;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::token::{INGEST_TOKEN_PREFIX, generate_token, hash_token};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};

const MAX_NAME_LENGTH: usize = 100;
const HINT_LENGTH: usize = 4;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_ingest_tokens, create_ingest_token))
        .routes(routes!(delete_ingest_token))
}

#[derive(Serialize, ToSchema)]
#[schema(as = IngestToken)]
struct IngestTokenView {
    id: String,
    name: String,
    hint: String,
    #[serde(with = "daroyan_core::time")]
    created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    last_used_at: Option<DateTime<Utc>>,
}

impl From<IngestToken> for IngestTokenView {
    fn from(token: IngestToken) -> Self {
        Self {
            id: token.id,
            name: token.name,
            hint: token.hint,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = CreatedIngestToken)]
struct CreatedIngestTokenView {
    token: String,
    #[serde(flatten)]
    ingest_token: IngestTokenView,
}

#[derive(Deserialize, ToSchema)]
struct IngestTokenBody {
    name: String,
}

#[utoipa::path(get, path = "/v1/ingest-tokens", responses((status = 200, body = Vec<IngestTokenView>)))]
async fn list_ingest_tokens(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<IngestTokenView>>> {
    let tokens = state.database.list_ingest_tokens().await?;
    Ok(Json(tokens.into_iter().map(IngestTokenView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/v1/ingest-tokens",
    request_body = IngestTokenBody,
    responses((status = 201, body = CreatedIngestTokenView))
)]
async fn create_ingest_token(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<IngestTokenBody>,
) -> Result<(StatusCode, Json<CreatedIngestTokenView>)> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME_LENGTH {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Name the token, up to 100 characters",
        ));
    }
    let token = generate_token(INGEST_TOKEN_PREFIX);
    let hint = token[token.len() - HINT_LENGTH..].to_string();
    let stored = state
        .database
        .create_ingest_token(new_id(Prefix::IngestToken), name, hash_token(&token), hint)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedIngestTokenView { token, ingest_token: stored.into() }),
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/ingest-tokens/{token_id}",
    params(("token_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_ingest_token(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(token_id): Path<IngestTokenId>,
) -> Result<StatusCode> {
    if !state.database.delete_ingest_token(&token_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Ingest token not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
