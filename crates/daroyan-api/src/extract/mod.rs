mod server;
mod user;

pub use server::AuthenticatedServer;
pub use user::{AdminUser, CurrentUser, McpCall};

use axum::http::HeaderMap;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::{Authorization, HeaderMapExt};
use daroyan_core::error::{Error, ErrorName};

pub fn read_bearer(headers: &HeaderMap) -> Result<String, Error> {
    headers
        .typed_get::<Authorization<Bearer>>()
        .map(|bearer| bearer.token().to_owned())
        .ok_or_else(|| Error::new(ErrorName::Unauthorized, "Unauthorized"))
}

pub fn is_cross_site(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-site")
        .is_some_and(|site| site != "same-origin" && site != "none")
}

pub fn cross_site_error() -> Error {
    Error::new(ErrorName::Forbidden, "Cross-site requests are not allowed")
}
