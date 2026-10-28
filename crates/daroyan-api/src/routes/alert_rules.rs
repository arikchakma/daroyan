use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use daroyan_core::alerts::backtest::backtest_rule;
use daroyan_core::alerts::engine::{deliver_notice, parse_rule};
use daroyan_core::alerts::notify::{Details, Notice, Tone};
use daroyan_core::alerts::rules::{
    Condition, RuleOptions, Scope, describe_condition, validate_options,
};
use daroyan_core::db::entities::alert_rules::Model as AlertRule;
use daroyan_core::db::ids::AlertRuleId;
use daroyan_core::db::queries::alert_rules::{
    AlertRuleChanges, NewAlertRule, RuleDeclare,
};
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::incidents::SEVERITIES;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::alert_delivery::AlertDeliveryView;
use crate::views::alert_rule::AlertRuleView;

const DELIVERY_LIMIT: u64 = 50;
const BACKTEST_DAYS: i64 = 7;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_alert_rules, create_alert_rule))
        .routes(routes!(get_alert_rule, update_alert_rule, delete_alert_rule))
        .routes(routes!(list_alert_deliveries))
        .routes(routes!(backtest_alert_rule))
        .routes(routes!(test_alert_rule))
}

#[derive(Deserialize, ToSchema)]
struct CreateAlertRuleBody {
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    condition: Condition,
    #[serde(default)]
    for_secs: u32,
    scope: Scope,
    #[serde(default)]
    integration_ids: Vec<String>,
    #[serde(default)]
    options: RuleOptions,
    #[serde(default)]
    declare_incident: bool,
    declare_severity: Option<String>,
    #[serde(default)]
    declare_hidden_page_ids: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct UpdateAlertRuleBody {
    name: Option<String>,
    enabled: Option<bool>,
    condition: Option<Condition>,
    for_secs: Option<u32>,
    scope: Option<Scope>,
    integration_ids: Option<Vec<String>>,
    options: Option<RuleOptions>,
    declare_incident: Option<bool>,
    declare_severity: Option<String>,
    declare_hidden_page_ids: Option<Vec<String>>,
}

#[derive(Deserialize, ToSchema)]
struct BacktestAlertRuleBody {
    condition: Condition,
    #[serde(default)]
    for_secs: u32,
    scope: Scope,
}

#[derive(Serialize, ToSchema)]
#[schema(as = AlertRuleBacktest)]
struct BacktestView {
    is_supported: bool,
    days: i64,
    hits: Vec<BacktestHitView>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = AlertRuleBacktestHit)]
struct BacktestHitView {
    server_id: String,
    server_name: String,
    subject: String,
    value: String,
    at: i64,
}

#[derive(Deserialize, ToSchema)]
struct TestAlertRuleBody {
    name: String,
    condition: Condition,
    #[serde(default)]
    for_secs: u32,
    integration_ids: Vec<String>,
}

fn default_true() -> bool {
    true
}

async fn find_alert_rule(state: &AppState, id: &AlertRuleId) -> Result<AlertRule> {
    state
        .database
        .find_alert_rule(id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Alert rule not found"))
}

fn read_options(options: RuleOptions) -> RuleOptions {
    let trim = |value: Option<String>| {
        value
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    };
    RuleOptions {
        runbook_url: trim(options.runbook_url),
        severity_label: trim(options.severity_label),
        ..options
    }
}

fn read_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::new(ErrorName::ValidationError, "Add a name"));
    }
    Ok(name.to_string())
}

async fn read_declare(
    state: &AppState,
    is_on: bool,
    severity: Option<String>,
    hidden_page_ids: Vec<String>,
) -> Result<RuleDeclare> {
    if is_on
        && !severity
            .as_deref()
            .is_some_and(|severity| SEVERITIES.contains(&severity))
    {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Pick the severity of the incident: minor, major or critical",
        ));
    }
    let pages = state.database.list_status_pages().await?;
    if hidden_page_ids
        .iter()
        .any(|id| !pages.iter().any(|page| &page.id == id))
    {
        return Err(Error::new(
            ErrorName::ValidationError,
            "A status page does not exist",
        ));
    }
    Ok(RuleDeclare {
        is_on,
        severity: severity.filter(|_| is_on),
        hidden_page_ids: serde_json::to_value(hidden_page_ids)?,
    })
}

async fn check_integrations(state: &AppState, integration_ids: &[String]) -> Result<()> {
    let integrations = state.database.list_integrations().await?;
    if let Some(missing) = integration_ids
        .iter()
        .find(|id| !integrations.iter().any(|integration| &&integration.id == id))
    {
        return Err(Error::new(
            ErrorName::ValidationError,
            format!("Integration {missing} does not exist"),
        ));
    }
    Ok(())
}

#[utoipa::path(get, path = "/v1/alert-rules", responses((status = 200, body = Vec<AlertRuleView>)))]
async fn list_alert_rules(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<AlertRuleView>>> {
    let rules = state.database.list_alert_rules().await?;
    Ok(Json(rules.into_iter().map(AlertRuleView::new).collect::<Result<_>>()?))
}

#[utoipa::path(
    post,
    path = "/v1/alert-rules",
    request_body = CreateAlertRuleBody,
    responses((status = 201, body = AlertRuleView))
)]
async fn create_alert_rule(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<CreateAlertRuleBody>,
) -> Result<(StatusCode, Json<AlertRuleView>)> {
    let options = read_options(body.options);
    validate_options(&body.condition, &options, &body.scope, body.for_secs)?;
    check_integrations(&state, &body.integration_ids).await?;
    check_integrations(&state, &options.warning_integration_ids).await?;
    let declare = read_declare(
        &state,
        body.declare_incident,
        body.declare_severity,
        body.declare_hidden_page_ids,
    )
    .await?;
    let rule = state
        .database
        .create_alert_rule(NewAlertRule {
            id: new_id(Prefix::AlertRule),
            name: read_name(&body.name)?,
            enabled: body.enabled,
            condition: serde_json::to_value(&body.condition)?,
            for_secs: i64::from(body.for_secs),
            scope: serde_json::to_value(&body.scope)?,
            integration_ids: serde_json::to_value(&body.integration_ids)?,
            options: serde_json::to_value(&options)?,
            declare,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(AlertRuleView::new(rule)?)))
}

#[utoipa::path(
    patch,
    path = "/v1/alert-rules/{rule_id}",
    params(("rule_id" = String, Path)),
    request_body = UpdateAlertRuleBody,
    responses((status = 200, body = AlertRuleView))
)]
async fn update_alert_rule(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(rule_id): Path<AlertRuleId>,
    Json(body): Json<UpdateAlertRuleBody>,
) -> Result<Json<AlertRuleView>> {
    let rule = find_alert_rule(&state, &rule_id).await?;
    let current = parse_rule(&rule).ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("alert rule {} is unreadable", rule.id))
    })?;
    let for_secs = u32::try_from(rule.for_secs).unwrap_or_default();
    let options = body.options.map(read_options);
    validate_options(
        body.condition.as_ref().unwrap_or(&current.condition),
        options.as_ref().unwrap_or(&current.options),
        body.scope.as_ref().unwrap_or(&current.scope),
        body.for_secs.unwrap_or(for_secs),
    )?;
    if let Some(integration_ids) = &body.integration_ids {
        check_integrations(&state, integration_ids).await?;
    }
    if let Some(options) = &options {
        check_integrations(&state, &options.warning_integration_ids).await?;
    }
    let is_declare_changed = body.declare_incident.is_some()
        || body.declare_severity.is_some()
        || body.declare_hidden_page_ids.is_some();
    let declare = if is_declare_changed {
        Some(
            read_declare(
                &state,
                body.declare_incident.unwrap_or(rule.declare_incident),
                body.declare_severity.or_else(|| rule.declare_severity.clone()),
                body.declare_hidden_page_ids.unwrap_or_else(|| {
                    serde_json::from_value(rule.declare_hidden_page_ids.clone())
                        .unwrap_or_default()
                }),
            )
            .await?,
        )
    } else {
        None
    };
    let rule = state
        .database
        .update_alert_rule(
            &rule_id,
            AlertRuleChanges {
                name: body.name.as_deref().map(read_name).transpose()?,
                enabled: body.enabled,
                condition: body
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()?,
                for_secs: body.for_secs.map(i64::from),
                scope: body.scope.as_ref().map(serde_json::to_value).transpose()?,
                integration_ids: body
                    .integration_ids
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()?,
                options: options.as_ref().map(serde_json::to_value).transpose()?,
                declare,
            },
        )
        .await?;
    Ok(Json(AlertRuleView::new(rule)?))
}

#[utoipa::path(
    delete,
    path = "/v1/alert-rules/{rule_id}",
    params(("rule_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_alert_rule(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(rule_id): Path<AlertRuleId>,
) -> Result<StatusCode> {
    if !state.database.delete_alert_rule(&rule_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Alert rule not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/alert-rules/{rule_id}",
    params(("rule_id" = String, Path)),
    responses((status = 200, body = AlertRuleView))
)]
async fn get_alert_rule(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(rule_id): Path<AlertRuleId>,
) -> Result<Json<AlertRuleView>> {
    Ok(Json(AlertRuleView::new(find_alert_rule(&state, &rule_id).await?)?))
}

#[utoipa::path(
    get,
    path = "/v1/alert-rules/{rule_id}/deliveries",
    params(("rule_id" = String, Path)),
    responses((status = 200, body = Vec<AlertDeliveryView>))
)]
async fn list_alert_deliveries(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(rule_id): Path<AlertRuleId>,
) -> Result<Json<Vec<AlertDeliveryView>>> {
    find_alert_rule(&state, &rule_id).await?;
    let deliveries =
        state.database.list_alert_deliveries(&rule_id, DELIVERY_LIMIT).await?;
    Ok(Json(deliveries.into_iter().map(AlertDeliveryView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/v1/alert-rules/backtest",
    request_body = BacktestAlertRuleBody,
    responses((status = 200, body = BacktestView))
)]
async fn backtest_alert_rule(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(body): Json<BacktestAlertRuleBody>,
) -> Result<Json<BacktestView>> {
    validate_options(
        &body.condition,
        &RuleOptions::default(),
        &body.scope,
        body.for_secs,
    )?;
    let hits = backtest_rule(
        &state.database,
        &state.analytics,
        &body.condition,
        &body.scope,
        body.for_secs,
        BACKTEST_DAYS,
    )
    .await?;
    Ok(Json(BacktestView {
        is_supported: hits.is_some(),
        days: BACKTEST_DAYS,
        hits: hits
            .unwrap_or_default()
            .into_iter()
            .map(|hit| BacktestHitView {
                server_id: hit.server_id,
                server_name: hit.server_name,
                subject: hit.subject,
                value: hit.value,
                at: hit.at,
            })
            .collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/v1/alert-rules/test",
    request_body = TestAlertRuleBody,
    responses((status = 204))
)]
async fn test_alert_rule(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<TestAlertRuleBody>,
) -> Result<StatusCode> {
    if body.integration_ids.is_empty() {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Turn on at least one integration",
        ));
    }
    check_integrations(&state, &body.integration_ids).await?;
    let notice = Notice {
        event: "test",
        tone: Tone::Note,
        title: format!("Test: {}", read_name(&body.name)?),
        body: format!(
            "This is a test. Real alerts for this rule look like this.\n{}",
            describe_condition(&body.condition, body.for_secs)
        ),
        url: state.config.app_url.clone(),
        data: json!({}),
        details: Details::default(),
    };
    let integrations = state.database.list_integrations().await?;
    let mut failures = Vec::new();
    for integration in integrations
        .iter()
        .filter(|item| body.integration_ids.contains(&item.id))
    {
        if let Err(error) =
            deliver_notice(&state.database, integration, &notice, None).await
        {
            failures.push(format!("{}: {error}", integration.name));
        }
    }
    if !failures.is_empty() {
        return Err(Error::new(ErrorName::ValidationError, failures.join(". ")));
    }
    Ok(StatusCode::NO_CONTENT)
}
