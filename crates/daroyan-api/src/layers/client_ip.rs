use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use daroyan_core::config::Config;
use daroyan_core::error::Error;

use crate::AppState;

#[derive(Clone, Copy, Debug)]
pub struct ClientIp(pub IpAddr);

pub async fn detect_client_ip(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let ip = get_client_ip(peer.ip(), request.headers(), &state.config);
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}

pub fn get_client_ip(peer: IpAddr, headers: &HeaderMap, config: &Config) -> IpAddr {
    let peer = peer.to_canonical();
    if !config.trusted_proxies.iter().any(|proxy| proxy.contains(&peer)) {
        return peer;
    }
    headers
        .get(config.client_ip_header.as_str())
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .map_or(peer, |ip| ip.to_canonical())
}

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Error> {
        parts.extensions.get::<Self>().copied().ok_or_else(|| {
            Error::Internal(anyhow::anyhow!("the client ip layer did not run"))
        })
    }
}

#[cfg(test)]
mod tests {
    use daroyan_core::config::parse_trusted_proxies;

    use super::*;

    fn make_config() -> Config {
        Config {
            host: "127.0.0.1".parse().unwrap(),
            port: 0,
            database_url: String::new(),
            duckdb_path: "unused".into(),
            retention_days: 30,
            log_retention_days: 7,
            otlp_log_budget: 1024 * 1024,
            backup_dir: None,
            trusted_proxies: parse_trusted_proxies("127.0.0.1,::1").unwrap(),
            client_ip_header: "cf-connecting-ip".to_string(),
            app_url: None,
            google_client_id: None,
            google_client_secret: None,
            allowed_emails: Vec::new(),
            slack_api_url: "https://slack.com".to_string(),
            slack_client_id: None,
            slack_client_secret: None,
            discord_client_id: None,
            discord_client_secret: None,
            telegram_bot_token: None,
            allow_private_targets: false,
        }
    }

    #[test]
    fn trusts_the_header_only_from_a_trusted_proxy() {
        let config = make_config();
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "198.51.100.4".parse().unwrap());

        let caddy: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert_eq!(
            get_client_ip(caddy, &headers, &config),
            "198.51.100.4".parse::<IpAddr>().unwrap()
        );

        let stranger: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(get_client_ip(stranger, &headers, &config), stranger);

        assert_eq!(
            get_client_ip(caddy, &HeaderMap::new(), &config),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
    }
}
