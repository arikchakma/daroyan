use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use daroyan_core::auth::{Access, Role, find_access, find_active_session};
use daroyan_core::db::entities::users::Model as User;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::token::{MCP_TOKEN_PREFIX, hash_token};

use crate::AppState;
use crate::cookies::{SESSION_COOKIE, read_cookie};
use crate::extract::{cross_site_error, is_cross_site, read_bearer};
use crate::layers::client_ip::ClientIp;

const TOUCH_EVERY: Duration = Duration::minutes(1);

pub struct CurrentUser {
    pub user: User,
    pub access: Access,
    pub is_token: bool,
}

impl CurrentUser {
    pub fn require_session(&self) -> Result<()> {
        if self.is_token {
            return Err(Error::new(
                ErrorName::Forbidden,
                "Sign in to Daroyan to do this. MCP tokens cannot.",
            ));
        }
        Ok(())
    }
}

pub struct AdminUser(pub CurrentUser);

#[derive(Clone, Copy)]
pub struct McpCall;

async fn find_mcp_user(state: &AppState, token: &str) -> Result<Option<User>> {
    let Some((stored, user)) =
        state.database.find_mcp_token_by_hash(&hash_token(token)).await?
    else {
        return Ok(None);
    };
    let now = Utc::now();
    if stored.last_used_at.is_none_or(|at| now - at > TOUCH_EVERY) {
        state.database.touch_mcp_token(&stored.id, now).await?;
    }
    Ok(Some(user))
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Error> {
        let unauthorized = || Error::new(ErrorName::Unauthorized, "Unauthorized");
        let cookie = read_cookie(&parts.headers, SESSION_COOKIE);
        let bearer = read_bearer(&parts.headers)
            .ok()
            .filter(|token| token.starts_with(MCP_TOKEN_PREFIX))
            .filter(|_| parts.extensions.get::<McpCall>().is_some());
        if cookie.is_none() && bearer.is_none() {
            return Err(unauthorized());
        }
        if cookie.is_some() && !parts.method.is_safe() && is_cross_site(&parts.headers) {
            return Err(cross_site_error());
        }
        let is_token = cookie.is_none();
        let ClientIp(ip) = ClientIp::from_request_parts(parts, state).await?;
        state.limits.check_auth_blocked(ip)?;
        let user = match (cookie, bearer) {
            (Some(token), _) => find_active_session(&state.database, &token)
                .await?
                .map(|(_, user)| user),
            (None, Some(token)) => find_mcp_user(state, &token).await?,
            (None, None) => None,
        };
        let Some(user) = user else {
            state.limits.record_auth_failure(ip);
            return Err(unauthorized());
        };
        let Some(access) =
            find_access(&state.database, &state.config, &user.email).await?
        else {
            return Err(Error::new(
                ErrorName::Unauthorized,
                "You no longer have access. Ask an admin to invite you again.",
            ));
        };
        Ok(Self { user, access, is_token })
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Error> {
        let current = CurrentUser::from_request_parts(parts, state).await?;
        if current.access.role != Role::Admin {
            return Err(Error::new(ErrorName::Forbidden, "Only admins can do this"));
        }
        Ok(Self(current))
    }
}
