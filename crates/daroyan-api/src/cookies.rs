use axum::http::HeaderMap;
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use cookie::time::{Duration, OffsetDateTime};
use cookie::{Cookie, SameSite};
use daroyan_core::config::Config;

pub const SESSION_COOKIE: &str = "daroyan_session";
pub const SIGNED_IN_COOKIE: &str = "daroyan_signed_in";
pub const STATE_COOKIE: &str = "daroyan_oauth_state";
pub const VERIFIER_COOKIE: &str = "daroyan_oauth_verifier";
pub const NEXT_COOKIE: &str = "daroyan_oauth_next";

fn is_secure(config: &Config) -> bool {
    config
        .app_url
        .as_deref()
        .is_some_and(|url| url.starts_with("https://"))
}

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    let value = jar.get(name)?.value();
    (!value.is_empty()).then(|| value.to_owned())
}

pub fn create_session_cookies(
    config: &Config,
    token: &str,
    expires_at: DateTime<Utc>,
) -> [String; 2] {
    let expires = OffsetDateTime::from_unix_timestamp(expires_at.timestamp())
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let session = Cookie::build((SESSION_COOKIE, token.to_owned())).http_only(true);
    let signed_in = Cookie::build((SIGNED_IN_COOKIE, "1"));
    [session, signed_in].map(|cookie| {
        cookie
            .path("/")
            .secure(is_secure(config))
            .same_site(SameSite::Lax)
            .expires(expires)
            .build()
            .to_string()
    })
}

pub fn create_short_cookie(config: &Config, name: &'static str, value: &str) -> String {
    Cookie::build((name, value.to_owned()))
        .path("/")
        .http_only(true)
        .secure(is_secure(config))
        .same_site(SameSite::Lax)
        .max_age(Duration::minutes(10))
        .build()
        .encoded()
        .to_string()
}

pub fn clear_cookie(name: &'static str) -> String {
    Cookie::build((name, ""))
        .path("/")
        .max_age(Duration::ZERO)
        .build()
        .to_string()
}
