use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use daroyan_core::db::entities::servers::Model as Server;
use daroyan_core::error::{Error, ErrorName};
use daroyan_core::token::hash_token;

use crate::AppState;
use crate::extract::read_bearer;
use crate::layers::client_ip::ClientIp;

pub struct AuthenticatedServer {
    pub server: Server,
}

impl FromRequestParts<AppState> for AuthenticatedServer {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Error> {
        let ClientIp(ip) = ClientIp::from_request_parts(parts, state).await?;
        state.limits.check_auth_blocked(ip)?;
        let server = match read_bearer(&parts.headers) {
            Ok(token) => {
                state.database.find_server_by_token_hash(&hash_token(&token)).await?
            }
            Err(_) => None,
        };
        let Some(server) = server else {
            state.limits.record_auth_failure(ip);
            return Err(Error::new(ErrorName::Unauthorized, "Unauthorized"));
        };
        Ok(Self { server })
    }
}
