use axum::Json;
use axum::extract::{Query, State};
use daroyan_core::analytics::log_lines::{OTLP_SERVER_ID, OTLP_SERVER_NAME};
use daroyan_core::db::ids::AlertRuleId;
use daroyan_core::db::queries::alerts::{STATUS_FIRING, STATUS_RESOLVED};
use daroyan_core::error::Result;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;
use crate::views::alert::AlertView;

const ALERT_LIMIT: u64 = 200;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_alerts))
}

#[derive(Deserialize)]
struct AlertsQuery {
    status: Option<String>,
    rule_id: Option<AlertRuleId>,
}

#[utoipa::path(
    get,
    path = "/v1/alerts",
    params(
        ("status" = Option<String>, Query, description = "firing, resolved or all (default)"),
        ("rule_id" = Option<String>, Query)
    ),
    responses((status = 200, body = Vec<AlertView>))
)]
async fn list_alerts(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<AlertsQuery>,
) -> Result<Json<Vec<AlertView>>> {
    let status = match query.status.as_deref() {
        Some(STATUS_FIRING) => Some(STATUS_FIRING),
        Some(STATUS_RESOLVED) => Some(STATUS_RESOLVED),
        _ => None,
    };
    let alerts = state
        .database
        .list_alerts(status, query.rule_id.as_ref(), ALERT_LIMIT)
        .await?;
    let rules = state.database.list_alert_rules().await?;
    let servers = state.database.list_servers().await?;
    let checks = state.database.list_uptime_checks().await?;
    let crons = state.database.list_crons().await?;
    let views = alerts
        .into_iter()
        .map(|alert| {
            let rule_name = rules
                .iter()
                .find(|rule| rule.id == alert.rule_id)
                .map(|rule| rule.name.clone())
                .unwrap_or_else(|| alert.rule_id.clone());
            let target_name = servers
                .iter()
                .find(|server| Some(&server.id) == alert.server_id.as_ref())
                .map(|server| server.name.clone())
                .or_else(|| {
                    (alert.server_id.as_deref() == Some(OTLP_SERVER_ID))
                        .then(|| OTLP_SERVER_NAME.to_string())
                })
                .or_else(|| {
                    checks
                        .iter()
                        .find(|check| Some(&check.id) == alert.check_id.as_ref())
                        .map(|check| check.name.clone())
                })
                .or_else(|| {
                    crons
                        .iter()
                        .find(|cron| Some(&cron.id) == alert.cron_id.as_ref())
                        .map(|cron| cron.name.clone())
                })
                .or_else(|| {
                    alert
                        .server_id
                        .clone()
                        .or_else(|| alert.check_id.clone())
                        .or_else(|| alert.cron_id.clone())
                })
                .unwrap_or_default();
            AlertView::new(alert, rule_name, target_name)
        })
        .collect();
    Ok(Json(views))
}
