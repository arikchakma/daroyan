use std::fs::{self, Permissions};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderName, StatusCode, Uri, header};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Extension, Json, Router};
use daroyan_core::agents::Agents;
use daroyan_core::alerts::connect::ConnectStates;
use daroyan_core::analytics::Analytics;
use daroyan_core::config::Config;
use daroyan_core::crons::Crons;
use daroyan_core::db::connect_database;
use daroyan_core::error::{Error, ErrorName};
use daroyan_core::limits::{Limits, LogBudgets};
use daroyan_core::net::allow_private_targets;
use daroyan_core::servers::hosts::HostCache;
use daroyan_core::uptime::Uptime;
use daroyan_migration::duckdb::open_duckdb;
use daroyan_migration::{Migrator, MigratorTrait};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;
use crate::layers::cache_control::forbid_caching;
use crate::layers::client_ip::detect_client_ip;
use crate::layers::errors::render_errors;
use crate::layers::rate_limit::limit_globally;
use crate::mcp::{Mcp, handle_mcp};
use crate::routes;

pub async fn open_state(config: Config) -> anyhow::Result<AppState> {
    allow_private_targets(config.allow_private_targets);
    if let Some(parent) = config.duckdb_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let database = connect_database(&config).await?;
    Migrator::up(&database.conn, None).await?;
    let analytics = Analytics::try_from(open_duckdb(&config.duckdb_path)?)?;
    restrict_data_files(&config);
    let uptime = Uptime::new(database.clone(), analytics.clone());
    let crons = Crons::new(database.clone(), analytics.clone());
    Ok(AppState {
        log_budgets: LogBudgets::new(config.otlp_log_budget),
        hosts: HostCache::default(),
        config: Arc::new(config),
        database,
        analytics,
        uptime,
        crons,
        agents: Agents::default(),
        limits: Limits::default(),
        connects: ConnectStates::default(),
    })
}

fn restrict_data_files(config: &Config) {
    let sqlite = config
        .database_url
        .strip_prefix("sqlite://")
        .and_then(|rest| rest.split('?').next())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let files = sqlite
        .iter()
        .flat_map(|path| [path.clone(), suffixed(path, "-wal"), suffixed(path, "-shm")])
        .chain([config.duckdb_path.clone(), suffixed(&config.duckdb_path, ".wal")]);
    for file in files.filter(|file| file.exists()) {
        if let Err(error) = fs::set_permissions(&file, Permissions::from_mode(0o600)) {
            tracing::warn!(path = %file.display(), %error, "make a data file private");
        }
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

const CACHE_HASHED: &str = "public, max-age=31536000, immutable";
const CACHE_STATIC: &str = "public, max-age=3600";
const CACHE_PAGE: &str = "no-cache";

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: https:; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

const SECURITY_HEADERS: [(HeaderName, &str); 4] = [
    (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
];

const INSTALL_SCRIPT: &str = include_str!("../../daroyan-agent/install.sh");

struct WebFile {
    path: &'static str,
    content_type: &'static str,
    data: &'static [u8],
}

const WEB_FILES: &[WebFile] = include!(concat!(env!("OUT_DIR"), "/web.rs"));

fn get_web_file(path: &str) -> Option<&'static WebFile> {
    let index = WEB_FILES.binary_search_by(|file| file.path.cmp(path)).ok()?;
    Some(&WEB_FILES[index])
}

fn accepts_brotli(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|coding| {
            let mut parts = coding.split(';');
            let is_brotli = parts
                .next()
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("br"));
            let quality = parts
                .filter_map(|param| param.trim().strip_prefix("q="))
                .find_map(|quality| quality.trim().parse::<f32>().ok());
            is_brotli && quality != Some(0.0)
        })
}

fn decompress_brotli(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decompressed = Vec::new();
    brotli_decompressor::Decompressor::new(data, 4096).read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

async fn serve_web(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("v1/") || path.starts_with("p/") || path.starts_with("releases/")
    {
        return not_found().into_response();
    }
    if path == "install.sh" {
        return (
            SECURITY_HEADERS,
            [(header::CONTENT_TYPE, "text/x-shellscript")],
            INSTALL_SCRIPT.replace("__DAROYAN_VERSION__", env!("CARGO_PKG_VERSION")),
        )
            .into_response();
    }
    let (file, cache) = match get_web_file(path) {
        Some(file) if path.starts_with("assets/") => (Some(file), CACHE_HASHED),
        Some(file) if !path.ends_with(".html") => (Some(file), CACHE_STATIC),
        Some(file) => (Some(file), CACHE_PAGE),
        None => (get_web_file("index.html"), CACHE_PAGE),
    };
    let Some(file) = file else {
        return not_found().into_response();
    };
    let response_headers = [
        (header::CONTENT_TYPE, file.content_type),
        (header::CACHE_CONTROL, cache),
        (header::VARY, "accept-encoding"),
    ];
    if accepts_brotli(&headers) {
        return (
            SECURITY_HEADERS,
            response_headers,
            [(header::CONTENT_ENCODING, "br")],
            file.data,
        )
            .into_response();
    }
    match decompress_brotli(file.data) {
        Ok(body) => (SECURITY_HEADERS, response_headers, body).into_response(),
        Err(error) => Error::from(anyhow::Error::from(error)).into_response(),
    }
}

fn not_found() -> Error {
    Error::new(ErrorName::NotFound, "Not found")
}

pub fn build_spec() -> utoipa::openapi::OpenApi {
    let (_, spec) = OpenApiRouter::<AppState>::new()
        .merge(routes::routes())
        .split_for_parts();
    spec
}

pub fn build_app(state: AppState) -> Router {
    let (router, spec) = OpenApiRouter::new().merge(routes::routes()).split_for_parts();
    let api = router.clone().layer(from_fn(render_errors)).with_state(state.clone());
    let mcp = Mcp::new(
        api,
        &serde_json::to_value(&spec).expect("openapi spec serializes"),
        state.config.app_url.as_deref(),
    );
    let router = router
        .route("/openapi.json", get(move || async move { Json(spec) }))
        .route(
            "/mcp",
            post(handle_mcp)
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .layer(Extension(Arc::new(mcp))),
        )
        .merge(routes::crons::ping_router())
        .merge(routes::otlp::router())
        .layer(from_fn(forbid_caching));
    router
        .fallback_service(any(serve_web))
        .method_not_allowed_fallback(|| async { not_found() })
        .layer(from_fn_with_state(state.clone(), limit_globally))
        .layer(from_fn(render_errors))
        .layer(CompressionLayer::new())
        .layer(from_fn_with_state(state.clone(), detect_client_ip))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn accepts(value: &str) -> bool {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_str(value).unwrap());
        accepts_brotli(&headers)
    }

    #[test]
    fn reads_brotli_from_accept_encoding() {
        assert!(accepts("gzip, deflate, br, zstd"));
        assert!(accepts("BR;q=0.5"));
        assert!(!accepts("gzip, deflate"));
        assert!(!accepts("br;q=0, gzip"));
        assert!(!accepts("br; q=0.000"));
        assert!(!accepts("brotli"));
        assert!(!accepts_brotli(&HeaderMap::new()));
    }
}
