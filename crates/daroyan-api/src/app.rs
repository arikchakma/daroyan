use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{HeaderName, StatusCode, Uri, header};
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
use daroyan_core::limits::Limits;
use daroyan_core::net::allow_private_targets;
use daroyan_core::uptime::Uptime;
use daroyan_migration::duckdb::run_duckdb_migrations;
use daroyan_migration::{Migrator, MigratorTrait};
use rust_embed::RustEmbed;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;
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
    let analytics = Analytics::open(&config.duckdb_path)?;
    analytics.run(run_duckdb_migrations)?;
    restrict_data_files(&config);
    let uptime = Uptime::new(database.clone(), analytics.clone());
    let crons = Crons::new(database.clone(), analytics.clone());
    Ok(AppState {
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

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; img-src 'self' data: https:; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

const SECURITY_HEADERS: [(HeaderName, &str); 4] = [
    (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
];

const INSTALL_SCRIPT: &str = include_str!("../../daroyan-agent/install.sh");

#[derive(RustEmbed)]
#[folder = "../../apps/web/build/client"]
#[allow_missing = true]
struct Web;

async fn serve_web(uri: Uri) -> Response {
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
    let (file, cache) = match Web::get(path) {
        Some(file) if path.starts_with("assets/") => (Some(file), CACHE_HASHED),
        Some(file) if !path.ends_with(".html") => (Some(file), CACHE_STATIC),
        Some(file) => (Some(file), CACHE_PAGE),
        None => (Web::get("index.html"), CACHE_PAGE),
    };
    match file {
        Some(file) => (
            SECURITY_HEADERS,
            [
                (header::CONTENT_TYPE, file.metadata.mimetype().to_owned()),
                (header::CACHE_CONTROL, cache.to_owned()),
            ],
            file.data,
        )
            .into_response(),
        None => not_found().into_response(),
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
        .merge(routes::otlp::router());
    router
        .fallback_service(any(serve_web).layer(CompressionLayer::new()))
        .method_not_allowed_fallback(|| async { not_found() })
        .layer(from_fn_with_state(state.clone(), limit_globally))
        .layer(from_fn(render_errors))
        .layer(from_fn_with_state(state.clone(), detect_client_ip))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
