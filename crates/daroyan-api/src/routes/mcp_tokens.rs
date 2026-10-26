use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use daroyan_core::db::entities::mcp_tokens::Model as McpToken;
use daroyan_core::db::ids::McpTokenId;
use daroyan_core::db::queries::mcp_tokens::NewMcpToken;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::token::{MCP_TOKEN_PREFIX, generate_token, hash_token};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

const MAX_NAME_LENGTH: usize = 100;
const HINT_LENGTH: usize = 4;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_mcp_tokens, create_mcp_token))
        .routes(routes!(delete_mcp_token))
}

#[derive(Serialize, ToSchema)]
#[schema(as = McpToken)]
struct McpTokenView {
    id: String,
    name: String,
    hint: String,
    #[serde(with = "daroyan_core::time")]
    created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    last_used_at: Option<DateTime<Utc>>,
}

impl From<McpToken> for McpTokenView {
    fn from(token: McpToken) -> Self {
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
#[schema(as = CreatedMcpToken)]
struct CreatedMcpTokenView {
    token: String,
    #[serde(flatten)]
    mcp_token: McpTokenView,
}

#[derive(Deserialize, ToSchema)]
struct McpTokenBody {
    name: String,
}

#[utoipa::path(get, path = "/v1/mcp-tokens", responses((status = 200, body = Vec<McpTokenView>)))]
async fn list_mcp_tokens(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<McpTokenView>>> {
    current.require_session()?;
    let user = current.user;
    let tokens = state.database.list_mcp_tokens(&user.id).await?;
    Ok(Json(tokens.into_iter().map(McpTokenView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/v1/mcp-tokens",
    request_body = McpTokenBody,
    responses((status = 201, body = CreatedMcpTokenView))
)]
async fn create_mcp_token(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(body): Json<McpTokenBody>,
) -> Result<(StatusCode, Json<CreatedMcpTokenView>)> {
    current.require_session()?;
    let user = current.user;
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME_LENGTH {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Name the token, up to 100 characters",
        ));
    }
    let token = generate_token(MCP_TOKEN_PREFIX);
    let hint = token[token.len() - HINT_LENGTH..].to_string();
    let stored = state
        .database
        .create_mcp_token(NewMcpToken {
            id: new_id(Prefix::McpToken),
            user_id: user.id,
            name,
            token_hash: hash_token(&token),
            hint,
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedMcpTokenView { token, mcp_token: stored.into() }),
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/mcp-tokens/{token_id}",
    params(("token_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_mcp_token(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(token_id): Path<McpTokenId>,
) -> Result<StatusCode> {
    current.require_session()?;
    let user = current.user;
    if !state.database.delete_mcp_token(&token_id, &user.id).await? {
        return Err(Error::new(ErrorName::NotFound, "MCP token not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
