use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use daroyan_core::alerts::engine::{DeliveryLog, notify_integrations, parse_rule};
use daroyan_core::alerts::notify::{Details, Notice, Tone};
use daroyan_core::alerts::rules::{format_clock, format_duration, read_time_zone};
use daroyan_core::db::entities::users::Model as User;
use daroyan_core::db::ids::{AlertMuteId, AlertRuleId, CronId};
use daroyan_core::db::queries::alert_mutes::NewAlertMute;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use serde::Deserialize;
use serde_json::json;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;
use crate::views::alert_mute::AlertMuteView;
use crate::views::user::UserView;

const MAX_MUTE_SECS: i64 = 30 * 86_400;
const MAX_REASON_LENGTH: usize = 200;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_alert_mutes, create_alert_mute))
        .routes(routes!(delete_alert_mute))
}

#[derive(Deserialize, ToSchema)]
struct CreateAlertMuteBody {
    rule_id: AlertRuleId,
    server_id: Option<String>,
    check_id: Option<String>,
    cron_id: Option<String>,
    ends_at: Option<DateTime<Utc>>,
    reason: Option<String>,
    time_zone: Option<String>,
}

#[utoipa::path(get, path = "/v1/alert-mutes", responses((status = 200, body = Vec<AlertMuteView>)))]
async fn list_alert_mutes(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<AlertMuteView>>> {
    let mutes = state.database.list_alert_mutes().await?;
    let users: HashMap<String, User> = state
        .database
        .list_users()
        .await?
        .into_iter()
        .map(|user| (user.id.clone(), user))
        .collect();
    Ok(Json(
        mutes
            .into_iter()
            .map(|mute| {
                let creator = users.get(&mute.creator_id).cloned().map(UserView::from);
                AlertMuteView::new(mute, creator)
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/alert-mutes",
    request_body = CreateAlertMuteBody,
    responses((status = 201, body = AlertMuteView))
)]
async fn create_alert_mute(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(body): Json<CreateAlertMuteBody>,
) -> Result<(StatusCode, Json<AlertMuteView>)> {
    let invalid = |message: &str| Error::new(ErrorName::ValidationError, message);
    let rule = state
        .database
        .find_alert_rule(&body.rule_id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Alert rule not found"))?;
    let now = Utc::now();
    if let Some(ends_at) = body.ends_at
        && !(1..=MAX_MUTE_SECS).contains(&(ends_at - now).num_seconds())
    {
        return Err(invalid("Mute for up to 30 days from now"));
    }
    let targets = [&body.server_id, &body.check_id, &body.cron_id];
    if targets.iter().filter(|id| id.is_some()).count() > 1 {
        return Err(invalid("Mute one server, check or cron, not several"));
    }
    let target_id = targets.into_iter().find_map(Clone::clone);
    let firing = state.database.list_firing_alerts().await?;
    let covered: Vec<_> = firing
        .iter()
        .filter(|alert| alert.rule_id == rule.id)
        .filter(|alert| {
            target_id.is_none()
                || alert.server_id == target_id
                || alert.check_id == target_id
                || alert.cron_id == target_id
        })
        .collect();
    if body.ends_at.is_none() && covered.is_empty() {
        return Err(invalid("Nothing is firing, so pick how long to mute for"));
    }
    let target_name = match (&body.server_id, &body.check_id, &body.cron_id) {
        (Some(id), _, _) => state
            .database
            .list_servers()
            .await?
            .into_iter()
            .find(|server| &server.id == id)
            .map(|server| server.name)
            .ok_or_else(|| invalid("The server does not exist"))?,
        (_, Some(id), _) => state
            .database
            .list_uptime_checks()
            .await?
            .into_iter()
            .find(|check| &check.id == id)
            .map(|check| check.name)
            .ok_or_else(|| invalid("The check does not exist"))?,
        (_, _, Some(id)) => state
            .database
            .find_cron(&CronId::from(id.as_str()))
            .await?
            .map(|cron| cron.name)
            .ok_or_else(|| invalid("The cron does not exist"))?,
        _ => "every target".to_string(),
    };
    let reason = body
        .reason
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    if reason.as_ref().is_some_and(|text| text.len() > MAX_REASON_LENGTH) {
        return Err(invalid("The reason can be up to 200 characters"));
    }
    let first_name = current
        .user
        .name
        .split_whitespace()
        .next()
        .unwrap_or("Someone")
        .to_string();
    let mute = state
        .database
        .create_alert_mute(NewAlertMute {
            id: new_id(Prefix::AlertMute),
            rule_id: rule.id.clone(),
            server_id: body.server_id,
            check_id: body.check_id,
            cron_id: body.cron_id,
            ends_at: body.ends_at,
            reason: reason.clone(),
            creator_id: current.user.id.clone(),
        })
        .await?;
    if let Some(parsed) = parse_rule(&rule) {
        let time_zone = read_time_zone(body.time_zone.as_deref());
        let (length, resume) = match body.ends_at {
            Some(ends_at) => (
                format!(
                    " for {}",
                    format_duration(
                        u32::try_from(((ends_at - now).num_seconds() + 59) / 60 * 60)
                            .unwrap_or_default()
                    )
                ),
                format!(
                    "Messages resume at {} if it is still firing",
                    format_clock(ends_at, time_zone)
                ),
            ),
            None => (String::new(), "Messages resume once it resolves".to_string()),
        };
        let reason = reason.map(|text| format!(" · {text}")).unwrap_or_default();
        let notice = Notice {
            event: "alert.muted",
            tone: Tone::Note,
            title: format!("Muted: {}", rule.name),
            body: format!("{first_name} muted {target_name}{length}{reason}\n{resume}"),
            url: state
                .config
                .app_url
                .as_ref()
                .map(|url| format!("{}/alerts/{}", url.trim_end_matches('/'), rule.id)),
            data: json!({ "rule": { "id": rule.id, "name": rule.name }, "mute": AlertMuteView::new(mute.clone(), Some(UserView::from(current.user.clone()))) }),
            details: Details::default(),
        };
        let integrations = state.database.list_integrations().await?;
        let log = DeliveryLog {
            rule_id: rule.id.clone(),
            alert_id: covered.first().map(|alert| alert.id.clone()),
            in_thread: false,
        };
        notify_integrations(
            &state.database,
            &parsed.integration_ids,
            &integrations,
            &notice,
            &log,
        );
    }
    Ok((
        StatusCode::CREATED,
        Json(AlertMuteView::new(mute, Some(UserView::from(current.user)))),
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/alert-mutes/{mute_id}",
    params(("mute_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_alert_mute(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(mute_id): Path<AlertMuteId>,
) -> Result<StatusCode> {
    if !state.database.delete_alert_mute(&mute_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Mute not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
