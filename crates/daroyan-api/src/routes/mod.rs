mod agent;
mod alert_mutes;
mod alert_rules;
mod alerts;
mod auth;
mod backups;
mod commands;
pub mod crons;
mod health;
mod incidents;
mod ingest_tokens;
mod install_tokens;
mod integrations;
mod logs;
mod mcp_tokens;
mod members;
pub mod otlp;
mod overview;
mod servers;
mod status_pages;
mod traces;
mod uptime_checks;
mod users;

use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(health::router())
        .merge(auth::router())
        .merge(backups::router())
        .merge(agent::router())
        .merge(install_tokens::router())
        .merge(servers::router())
        .merge(commands::router())
        .merge(integrations::router())
        .merge(alert_rules::router())
        .merge(alerts::router())
        .merge(alert_mutes::router())
        .merge(logs::router())
        .merge(uptime_checks::router())
        .merge(crons::router())
        .merge(status_pages::router())
        .merge(incidents::router())
        .merge(users::router())
        .merge(ingest_tokens::router())
        .merge(mcp_tokens::router())
        .merge(members::router())
        .merge(traces::router())
        .merge(overview::router())
}
