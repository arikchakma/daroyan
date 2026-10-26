use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use daroyan_core::db::entities::incidents::Model as Incident;
use daroyan_core::db::entities::status_pages::Model as StatusPage;
use daroyan_core::db::entities::users::Model as User;
use daroyan_core::db::ids::{AlertId, IncidentId, IncidentUpdateId};
use daroyan_core::db::queries::incidents::{IncidentFields, PostmortemFields};
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::incidents::{
    AUTOMATIC, AffectedComponent, COMPONENT_STATUSES, EntryInput, FollowUp,
    KIND_INCIDENT, KIND_MAINTENANCE, MAINTENANCE, NOTE, RELATED_KINDS, RelatedItem,
    Responder, SCHEDULED, SEVERITIES, UPDATE, add_entry as record_entry,
    describe_component_status, is_closed, notify_incident,
    notify_update as notify_update_with, statuses_of,
};
use daroyan_core::rich_text::{RichTextError, paragraph, read_document};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::incident::{
    ComponentView, IncidentUpdateView, IncidentView, PersonView, RelatedView,
    ResponderView, StatusPageRefView,
};
use crate::views::status_page::read_sections;
use crate::views::user::UserView;

const MAX_TITLE_LENGTH: usize = 200;
const MAX_BODY_LENGTH: usize = 5000;
const MAX_POSTMORTEM_LENGTH: usize = 20_000;
const MAX_FOLLOW_UP_LENGTH: usize = 500;
const MAX_ROLE_LENGTH: usize = 40;
const MAX_ITEMS: usize = 100;
const CLOCK_SKEW_SECS: i64 = 60;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_incidents, create_incident))
        .routes(routes!(get_incident, update_incident, delete_incident))
        .routes(routes!(list_incident_updates, post_incident_update))
        .routes(routes!(edit_incident_update))
        .routes(routes!(save_postmortem, publish_postmortem, unpublish_postmortem))
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorName::ValidationError, message)
}

fn not_found() -> Error {
    Error::new(ErrorName::NotFound, "Incident not found")
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, ToSchema)]
struct IncidentBody {
    title: String,
    severity: Option<String>,
    lead_id: Option<String>,
    started_at: Option<DateTime<Utc>>,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    #[serde(default = "yes")]
    auto_start: bool,
    #[serde(default = "yes")]
    auto_finish: bool,
    #[serde(default = "yes")]
    remind_before: bool,
    #[serde(default)]
    components: Vec<AffectedComponent>,
    #[serde(default)]
    related: Vec<RelatedItem>,
    #[serde(default)]
    responders: Vec<Responder>,
    #[serde(default)]
    channel_ids: Vec<String>,
    #[serde(default)]
    hidden_page_ids: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
struct FirstUpdateBody {
    status: Option<String>,
    #[schema(schema_with = rich_text_schema)]
    body: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
struct NewIncidentBody {
    #[schema(example = "incident")]
    kind: String,
    #[serde(flatten)]
    incident: IncidentBody,
    update: FirstUpdateBody,
    alert_id: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct IncidentUpdateBody {
    #[schema(example = "update")]
    kind: String,
    status: Option<String>,
    #[schema(schema_with = rich_text_schema)]
    body: serde_json::Value,
    #[serde(default = "yes")]
    is_public: bool,
    #[serde(default = "yes")]
    notify: bool,
}

#[derive(Deserialize, ToSchema)]
struct PublishPostmortemBody {
    #[serde(default = "yes")]
    notify: bool,
}

#[derive(Deserialize, ToSchema)]
struct EditUpdateBody {
    #[schema(schema_with = rich_text_schema)]
    body: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
struct FollowUpBody {
    id: Option<String>,
    body: String,
    owner_id: Option<String>,
    #[serde(default)]
    is_done: bool,
}

#[derive(Deserialize, ToSchema)]
struct PostmortemBody {
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    postmortem: Option<serde_json::Value>,
    #[serde(default)]
    follow_ups: Vec<FollowUpBody>,
}

struct Directory {
    checks: HashMap<String, String>,
    crons: HashMap<String, String>,
    servers: HashMap<String, String>,
    rules: HashMap<String, String>,
    users: HashMap<String, User>,
    integrations: HashSet<String>,
    pages: Vec<StatusPage>,
}

impl Directory {
    async fn load(state: &AppState) -> Result<Self> {
        let database = &state.database;
        Ok(Self {
            checks: database
                .list_uptime_checks()
                .await?
                .into_iter()
                .map(|check| (check.id, check.name))
                .collect(),
            crons: database
                .list_crons()
                .await?
                .into_iter()
                .map(|cron| (cron.id, cron.name))
                .collect(),
            servers: database
                .list_servers()
                .await?
                .into_iter()
                .map(|server| (server.id, server.name))
                .collect(),
            rules: database
                .list_alert_rules()
                .await?
                .into_iter()
                .map(|rule| (rule.id, rule.name))
                .collect(),
            users: database
                .list_users()
                .await?
                .into_iter()
                .map(|user| (user.id.clone(), user))
                .collect(),
            integrations: database
                .list_integrations()
                .await?
                .into_iter()
                .map(|integration| integration.id)
                .collect(),
            pages: database.list_status_pages().await?,
        })
    }

    fn name_of(&self, kind: &str, id: &str) -> Option<&str> {
        let names = match kind {
            "check" => &self.checks,
            "cron" => &self.crons,
            "server" => &self.servers,
            "rule" => &self.rules,
            _ => return None,
        };
        names.get(id).map(String::as_str)
    }

    fn person(&self, id: Option<&str>) -> Option<PersonView> {
        let id = id?;
        Some(PersonView {
            id: id.to_string(),
            name: self
                .users
                .get(id)
                .map(|user| user.name.clone())
                .unwrap_or_else(|| "Former user".to_string()),
        })
    }

    fn user(&self, id: Option<&str>) -> Option<UserView> {
        self.users.get(id?).cloned().map(UserView::from)
    }

    fn view(&self, incident: Incident, public_update_count: u64) -> IncidentView {
        let components: Vec<AffectedComponent> = read_json(&incident.components);
        let hidden: HashSet<String> = read_json(&incident.hidden_page_ids);
        let affected: HashSet<(&str, &str)> = components
            .iter()
            .map(|component| (component.kind.as_str(), component.id.as_str()))
            .collect();
        let status_pages = self
            .pages
            .iter()
            .filter(|page| {
                read_sections(page)
                    .iter()
                    .flat_map(|section| &section.components)
                    .any(|component| {
                        affected
                            .contains(&(component.kind.as_str(), component.id.as_str()))
                    })
            })
            .map(|page| StatusPageRefView {
                id: page.id.clone(),
                name: page.name.clone(),
                slug: page.slug.clone(),
                is_shown: !hidden.contains(&page.id),
            })
            .collect();
        let components = components
            .iter()
            .filter_map(|component| {
                Some(ComponentView {
                    name: self.name_of(&component.kind, &component.id)?.to_string(),
                    kind: component.kind.clone(),
                    id: component.id.clone(),
                    status: component.status.clone(),
                })
            })
            .collect();
        let related = read_json::<Vec<RelatedItem>>(&incident.related)
            .into_iter()
            .filter_map(|item| {
                Some(RelatedView {
                    name: self.name_of(&item.kind, &item.id)?.to_string(),
                    kind: item.kind,
                    id: item.id,
                })
            })
            .collect();
        let responders = read_json::<Vec<Responder>>(&incident.responders)
            .into_iter()
            .filter_map(|responder| {
                Some(ResponderView {
                    name: self.users.get(&responder.user_id)?.name.clone(),
                    id: responder.user_id,
                    role: responder.role,
                })
            })
            .collect();
        let channel_ids = read_json::<Vec<String>>(&incident.channel_ids)
            .into_iter()
            .filter(|id| self.integrations.contains(id))
            .collect();
        IncidentView {
            lead: self.person(incident.lead_id.as_deref()),
            components,
            related,
            responders,
            channel_ids,
            status_pages,
            follow_ups: read_json(&incident.follow_ups),
            public_update_count,
            id: incident.id,
            kind: incident.kind,
            title: incident.title,
            status: incident.status,
            severity: incident.severity,
            started_at: incident.started_at,
            resolved_at: incident.resolved_at,
            starts_at: incident.starts_at,
            ends_at: incident.ends_at,
            auto_start: incident.auto_start,
            auto_finish: incident.auto_finish,
            remind_before: incident.remind_before,
            postmortem: incident.postmortem,
            postmortem_published_at: incident.postmortem_published_at,
            created_at: incident.created_at,
            updated_at: incident.updated_at,
        }
    }

    fn describe_components(&self, components: &[AffectedComponent]) -> String {
        let names: Vec<String> = components
            .iter()
            .filter_map(|component| {
                Some(format!(
                    "{} ({})",
                    self.name_of(&component.kind, &component.id)?,
                    describe_component_status(&component.status)
                ))
            })
            .collect();
        if names.is_empty() {
            "No components are affected now.".to_string()
        } else {
            format!("Affected components are now {}.", names.join(", "))
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned + Default>(value: &serde_json::Value) -> T {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(value).map_err(anyhow::Error::from)?)
}

fn read_text(text: &str, max: usize, message: &str) -> Result<String> {
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > max {
        return Err(invalid(message));
    }
    Ok(text)
}

fn rich_text_schema() -> utoipa::openapi::schema::Schema {
    use utoipa::openapi::schema::{
        AdditionalProperties, ObjectBuilder, OneOfBuilder, Type,
    };

    OneOfBuilder::new()
        .item(ObjectBuilder::new().schema_type(Type::String))
        .item(
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .additional_properties(Some(AdditionalProperties::FreeForm(true))),
        )
        .description(Some(
            "Plain text, with a blank line between paragraphs, or a TipTap document",
        ))
        .build()
        .into()
}

fn read_rich_text(
    document: &serde_json::Value,
    max: usize,
    message: &str,
) -> Result<serde_json::Value> {
    read_document(document, max).map_err(|error| match error {
        RichTextError::Invalid => invalid("The text has formatting Daroyan cannot show"),
        RichTextError::Empty | RichTextError::TooLong => invalid(message),
    })
}

fn read_postmortem(
    document: Option<serde_json::Value>,
) -> Result<Option<serde_json::Value>> {
    let Some(document) = document.filter(|document| !document.is_null()) else {
        return Ok(None);
    };
    match read_document(&document, MAX_POSTMORTEM_LENGTH) {
        Ok(document) => Ok(Some(document)),
        Err(RichTextError::Empty) => Ok(None),
        Err(RichTextError::Invalid) => {
            Err(invalid("The text has formatting Daroyan cannot show"))
        }
        Err(RichTextError::TooLong) => {
            Err(invalid("The postmortem can be up to 20,000 characters"))
        }
    }
}

fn read_fields(
    directory: &Directory,
    body: IncidentBody,
    kind: &str,
    existing: Option<&Incident>,
) -> Result<IncidentFields> {
    let now = Utc::now();
    let is_maintenance = kind == KIND_MAINTENANCE;
    let title =
        read_text(&body.title, MAX_TITLE_LENGTH, "Add a title of up to 200 characters")?;
    let severity = if is_maintenance {
        None
    } else {
        let severity = body.severity.unwrap_or_default();
        if !SEVERITIES.contains(&severity.as_str()) {
            return Err(invalid("Pick a severity: minor, major or critical"));
        }
        Some(severity)
    };
    if body
        .lead_id
        .as_ref()
        .is_some_and(|id| !directory.users.contains_key(id))
    {
        return Err(invalid("The lead must be someone who has signed in to Daroyan"));
    }
    let (starts_at, ends_at) = if is_maintenance {
        let (Some(starts_at), Some(ends_at)) = (body.starts_at, body.ends_at) else {
            return Err(invalid("Set when the maintenance starts and ends"));
        };
        if ends_at <= starts_at {
            return Err(invalid("The maintenance has to end after it starts"));
        }
        (Some(starts_at), Some(ends_at))
    } else {
        (None, None)
    };
    let started_at = match (is_maintenance, existing) {
        (true, Some(existing)) if existing.status != SCHEDULED => existing.started_at,
        (true, _) => starts_at.unwrap_or(now),
        (false, _) => {
            let started_at = body
                .started_at
                .or(existing.map(|existing| existing.started_at))
                .unwrap_or(now);
            if (started_at - now).num_seconds() > CLOCK_SKEW_SECS {
                return Err(invalid("The start time cannot be in the future"));
            }
            started_at
        }
    };
    if body.components.len() > MAX_ITEMS || body.related.len() > MAX_ITEMS {
        return Err(invalid(
            "An incident can list up to 100 components and related items",
        ));
    }
    let mut seen = HashSet::new();
    let components = body
        .components
        .into_iter()
        .map(|component| {
            if !["check", "cron"].contains(&component.kind.as_str())
                || directory.name_of(&component.kind, &component.id).is_none()
            {
                return Err(invalid(
                    "A component points to a monitor that does not exist",
                ));
            }
            if !seen.insert((component.kind.clone(), component.id.clone())) {
                return Err(invalid("Each monitor can be affected once"));
            }
            let status = if is_maintenance {
                MAINTENANCE.to_string()
            } else if COMPONENT_STATUSES.contains(&component.status.as_str()) {
                component.status
            } else {
                return Err(invalid(
                    "Pick a component status: degraded, partial or outage",
                ));
            };
            Ok(AffectedComponent { status, ..component })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let related = body
        .related
        .into_iter()
        .filter(|item| seen.insert((item.kind.clone(), item.id.clone())))
        .map(|item| {
            if !RELATED_KINDS.contains(&item.kind.as_str())
                || directory.name_of(&item.kind, &item.id).is_none()
            {
                return Err(invalid("A related item does not exist"));
            }
            Ok(item)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let responders = body
        .responders
        .into_iter()
        .filter(|responder| seen.insert(responder.user_id.clone()))
        .map(|responder| {
            if !directory.users.contains_key(&responder.user_id) {
                return Err(invalid("A responder must be someone who has signed in"));
            }
            let role = read_text(
                &responder.role,
                MAX_ROLE_LENGTH,
                "Give each responder a role of up to 40 characters",
            )?;
            Ok(Responder { role, ..responder })
        })
        .collect::<Result<Vec<_>>>()?;
    if body.channel_ids.iter().any(|id| !directory.integrations.contains(id)) {
        return Err(invalid("A channel to notify is not connected"));
    }
    if body
        .hidden_page_ids
        .iter()
        .any(|id| !directory.pages.iter().any(|page| &page.id == id))
    {
        return Err(invalid("A status page does not exist"));
    }
    let unique = |ids: Vec<String>| -> Vec<String> {
        let mut seen = HashSet::new();
        ids.into_iter().filter(|id| seen.insert(id.clone())).collect()
    };
    Ok(IncidentFields {
        title,
        severity,
        lead_id: body.lead_id,
        started_at,
        starts_at,
        ends_at,
        auto_start: body.auto_start,
        auto_finish: body.auto_finish,
        remind_before: body.remind_before,
        components: to_json(&components)?,
        related: to_json(&related)?,
        responders: to_json(&responders)?,
        channel_ids: to_json(&unique(body.channel_ids))?,
        hidden_page_ids: to_json(&unique(body.hidden_page_ids))?,
    })
}

async fn find_incident(state: &AppState, id: &IncidentId) -> Result<Incident> {
    state.database.find_incident(id).await?.ok_or_else(not_found)
}

async fn count_public_updates(
    state: &AppState,
    ids: Vec<String>,
) -> Result<HashMap<String, u64>> {
    Ok(state
        .database
        .count_public_updates(ids)
        .await?
        .into_iter()
        .map(|(id, count)| (id, u64::try_from(count).unwrap_or_default()))
        .collect())
}

async fn to_view(state: &AppState, incident: Incident) -> Result<IncidentView> {
    let directory = Directory::load(state).await?;
    let counts = count_public_updates(state, vec![incident.id.clone()]).await?;
    let count = counts.get(&incident.id).copied().unwrap_or_default();
    Ok(directory.view(incident, count))
}

async fn add_entry(
    state: &AppState,
    incident_id: &str,
    kind: &'static str,
    status: Option<String>,
    body: serde_json::Value,
    is_public: bool,
    creator_id: Option<String>,
) -> Result<daroyan_core::db::entities::incident_updates::Model> {
    let entry = EntryInput {
        kind,
        status,
        body,
        is_public,
        creator_id,
        created_at: Utc::now(),
    };
    record(state, incident_id, entry).await
}

async fn record(
    state: &AppState,
    incident_id: &str,
    entry: EntryInput,
) -> Result<daroyan_core::db::entities::incident_updates::Model> {
    record_entry(&state.database, incident_id, entry).await
}

async fn notify_update(
    state: &AppState,
    incident: &Incident,
    status: &str,
    body: &serde_json::Value,
) {
    let app_url = state.config.app_url.as_deref();
    if let Err(error) =
        notify_update_with(&state.database, app_url, incident, status, body).await
    {
        tracing::warn!(incident_id = %incident.id, error = ?error, "notify an incident update");
    }
}

fn closing_time(incident: &Incident, status: &str) -> Option<DateTime<Utc>> {
    is_closed(status).then(|| incident.resolved_at.unwrap_or_else(Utc::now))
}

#[utoipa::path(
    get,
    path = "/v1/incidents",
    responses((status = 200, body = Vec<IncidentView>))
)]
async fn list_incidents(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<IncidentView>>> {
    let incidents = state.database.list_incidents().await?;
    let directory = Directory::load(&state).await?;
    let counts = count_public_updates(
        &state,
        incidents.iter().map(|incident| incident.id.clone()).collect(),
    )
    .await?;
    Ok(Json(
        incidents
            .into_iter()
            .map(|incident| {
                let count = counts.get(&incident.id).copied().unwrap_or_default();
                directory.view(incident, count)
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/incidents",
    request_body = NewIncidentBody,
    responses((status = 201, body = IncidentView))
)]
async fn create_incident(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(body): Json<NewIncidentBody>,
) -> Result<(StatusCode, Json<IncidentView>)> {
    let kind = match body.kind.as_str() {
        KIND_INCIDENT => KIND_INCIDENT,
        KIND_MAINTENANCE => KIND_MAINTENANCE,
        _ => return Err(invalid("Pick a kind: incident or maintenance")),
    };
    let status = if kind == KIND_MAINTENANCE {
        SCHEDULED.to_string()
    } else {
        let status = body.update.status.unwrap_or_default();
        if !statuses_of(kind).contains(&status.as_str()) {
            return Err(invalid(
                "Pick a status: investigating, identified, monitoring or resolved",
            ));
        }
        status
    };
    let message = read_rich_text(
        &body.update.body,
        MAX_BODY_LENGTH,
        "Write a first update of up to 5,000 characters",
    )?;
    let directory = Directory::load(&state).await?;
    let fields = read_fields(&directory, body.incident, kind, None)?;
    let declared_from = match body.alert_id {
        Some(alert_id) => {
            let alert = state
                .database
                .find_alert(&AlertId::from(alert_id.as_str()))
                .await?
                .ok_or_else(|| invalid("The alert does not exist"))?;
            let (kind, target_id) =
                match (&alert.server_id, &alert.check_id, &alert.cron_id) {
                    (_, Some(check_id), _) => ("check", check_id.clone()),
                    (_, _, Some(cron_id)) => ("cron", cron_id.clone()),
                    (server_id, _, _) => {
                        ("server", server_id.clone().unwrap_or_default())
                    }
                };
            let rule = directory.name_of("rule", &alert.rule_id).unwrap_or("An alert");
            let target =
                directory.name_of(kind, &target_id).unwrap_or("a deleted target");
            Some(format!(
                "{rule} fired on {target}. {} declared this incident from the alert.",
                current.user.name
            ))
        }
        None => None,
    };
    let id = new_id(Prefix::Incident);
    let mut incident = state
        .database
        .create_incident(id.clone(), kind, &status, fields)
        .await?;
    if is_closed(&status) {
        incident = state
            .database
            .set_incident_status(&IncidentId(id.clone()), &status, None, Some(Utc::now()))
            .await?;
    }
    let now = Utc::now();
    if let Some(declared_from) = declared_from {
        record(
            &state,
            &id,
            EntryInput {
                kind: AUTOMATIC,
                status: None,
                body: paragraph(&declared_from),
                is_public: false,
                creator_id: Some(current.user.id.clone()),
                created_at: now - chrono::Duration::milliseconds(1),
            },
        )
        .await?;
    }
    notify_update(&state, &incident, &status, &message).await;
    record(
        &state,
        &id,
        EntryInput {
            kind: UPDATE,
            status: Some(status),
            body: message,
            is_public: true,
            creator_id: Some(current.user.id),
            created_at: now,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(to_view(&state, incident).await?)))
}

#[utoipa::path(
    get,
    path = "/v1/incidents/{incident_id}",
    params(("incident_id" = String, Path)),
    responses((status = 200, body = IncidentView))
)]
async fn get_incident(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(incident_id): Path<IncidentId>,
) -> Result<Json<IncidentView>> {
    let incident = find_incident(&state, &incident_id).await?;
    Ok(Json(to_view(&state, incident).await?))
}

#[utoipa::path(
    put,
    path = "/v1/incidents/{incident_id}",
    params(("incident_id" = String, Path)),
    request_body = IncidentBody,
    responses((status = 200, body = IncidentView))
)]
async fn update_incident(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(incident_id): Path<IncidentId>,
    Json(body): Json<IncidentBody>,
) -> Result<Json<IncidentView>> {
    let existing = find_incident(&state, &incident_id).await?;
    let directory = Directory::load(&state).await?;
    let fields = read_fields(&directory, body, &existing.kind, Some(&existing))?;
    let mut entries = Vec::new();
    if fields.severity != existing.severity
        && let Some(severity) = &fields.severity
    {
        entries.push(format!("Severity changed to {}.", capitalize(severity)));
    }
    if fields.lead_id != existing.lead_id {
        entries.push(match directory.person(fields.lead_id.as_deref()) {
            Some(lead) => format!("{} is now the lead.", lead.name),
            None => "The incident has no lead now.".to_string(),
        });
    }
    if fields.components != existing.components {
        let components: Vec<AffectedComponent> = read_json(&fields.components);
        entries.push(directory.describe_components(&components));
    }
    let is_moved = fields.starts_at != existing.starts_at;
    let mut incident = state.database.update_incident(&incident_id, fields).await?;
    if is_moved && incident.reminded_at.is_some() {
        incident = state.database.mark_incident_reminded(&incident_id, None).await?;
    }
    for entry in entries {
        add_entry(
            &state,
            &incident.id,
            AUTOMATIC,
            None,
            paragraph(&entry),
            false,
            Some(current.user.id.clone()),
        )
        .await?;
    }
    Ok(Json(to_view(&state, incident).await?))
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[utoipa::path(
    delete,
    path = "/v1/incidents/{incident_id}",
    params(("incident_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_incident(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(incident_id): Path<IncidentId>,
) -> Result<StatusCode> {
    if !state.database.delete_incident(&incident_id).await? {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/v1/incidents/{incident_id}/updates",
    params(("incident_id" = String, Path)),
    responses((status = 200, body = Vec<IncidentUpdateView>))
)]
async fn list_incident_updates(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(incident_id): Path<IncidentId>,
) -> Result<Json<Vec<IncidentUpdateView>>> {
    find_incident(&state, &incident_id).await?;
    let directory = Directory::load(&state).await?;
    let updates = state.database.list_incident_updates(&incident_id).await?;
    Ok(Json(
        updates
            .into_iter()
            .map(|update| {
                let creator = directory.user(update.creator_id.as_deref());
                IncidentUpdateView::new(update, creator)
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/incidents/{incident_id}/updates",
    params(("incident_id" = String, Path)),
    request_body = IncidentUpdateBody,
    responses((status = 201, body = IncidentUpdateView))
)]
async fn post_incident_update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(incident_id): Path<IncidentId>,
    Json(body): Json<IncidentUpdateBody>,
) -> Result<(StatusCode, Json<IncidentUpdateView>)> {
    let incident = find_incident(&state, &incident_id).await?;
    let message = read_rich_text(
        &body.body,
        MAX_BODY_LENGTH,
        "Write an update of up to 5,000 characters",
    )?;
    let (kind, status, is_public) = match body.kind.as_str() {
        NOTE => (NOTE, None, false),
        UPDATE => {
            let status = body.status.unwrap_or_default();
            if !statuses_of(&incident.kind).contains(&status.as_str()) {
                return Err(invalid("Pick a status for this update"));
            }
            (UPDATE, Some(status), body.is_public)
        }
        _ => return Err(invalid("Pick a kind: update or note")),
    };
    let mut incident = incident;
    if let Some(status) = &status {
        let started_at = (incident.kind == KIND_MAINTENANCE
            && status != SCHEDULED
            && incident.status == SCHEDULED)
            .then(Utc::now);
        let resolved_at = closing_time(&incident, status);
        incident = state
            .database
            .set_incident_status(&incident_id, status, started_at, resolved_at)
            .await?;
    }
    if let Some(status) = status.as_deref().filter(|_| body.notify && is_public) {
        notify_update(&state, &incident, status, &message).await;
    }
    let author = Some(current.user.id.clone());
    let update =
        add_entry(&state, &incident.id, kind, status, message, is_public, author).await?;
    let creator = UserView::from(current.user);
    Ok((StatusCode::CREATED, Json(IncidentUpdateView::new(update, Some(creator)))))
}

#[utoipa::path(
    put,
    path = "/v1/incidents/{incident_id}/updates/{update_id}",
    params(("incident_id" = String, Path), ("update_id" = String, Path)),
    request_body = EditUpdateBody,
    responses((status = 200, body = IncidentUpdateView))
)]
async fn edit_incident_update(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path((incident_id, update_id)): Path<(IncidentId, IncidentUpdateId)>,
    Json(body): Json<EditUpdateBody>,
) -> Result<Json<IncidentUpdateView>> {
    let update = state
        .database
        .find_incident_update(&update_id)
        .await?
        .filter(|update| update.incident_id == incident_id.as_str())
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Update not found"))?;
    if update.kind == AUTOMATIC {
        return Err(invalid("Automatic entries cannot be edited"));
    }
    let message = read_rich_text(
        &body.body,
        MAX_BODY_LENGTH,
        "Write an update of up to 5,000 characters",
    )?;
    let update = state.database.edit_incident_update(&update_id, message).await?;
    let directory = Directory::load(&state).await?;
    let creator = directory.user(update.creator_id.as_deref());
    Ok(Json(IncidentUpdateView::new(update, creator)))
}

#[utoipa::path(
    put,
    path = "/v1/incidents/{incident_id}/postmortem",
    params(("incident_id" = String, Path)),
    request_body = PostmortemBody,
    responses((status = 200, body = IncidentView))
)]
async fn save_postmortem(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(incident_id): Path<IncidentId>,
    Json(body): Json<PostmortemBody>,
) -> Result<Json<IncidentView>> {
    let incident = find_incident(&state, &incident_id).await?;
    if incident.kind != KIND_INCIDENT {
        return Err(invalid("Only incidents have a postmortem"));
    }
    if body.follow_ups.len() > MAX_ITEMS {
        return Err(invalid("A postmortem can have up to 100 follow-ups"));
    }
    let directory = Directory::load(&state).await?;
    let follow_ups = body
        .follow_ups
        .into_iter()
        .map(|follow_up| {
            if follow_up
                .owner_id
                .as_ref()
                .is_some_and(|id| !directory.users.contains_key(id))
            {
                return Err(invalid(
                    "A follow-up owner must be someone who has signed in",
                ));
            }
            Ok(FollowUp {
                id: follow_up
                    .id
                    .filter(|id| id.starts_with("fup_"))
                    .unwrap_or_else(|| new_id(Prefix::FollowUp)),
                body: read_text(
                    &follow_up.body,
                    MAX_FOLLOW_UP_LENGTH,
                    "Each follow-up can be up to 500 characters",
                )?,
                owner_id: follow_up.owner_id,
                is_done: follow_up.is_done,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let incident = state
        .database
        .save_postmortem(
            &incident_id,
            PostmortemFields {
                postmortem: read_postmortem(body.postmortem)?,
                follow_ups: to_json(&follow_ups)?,
            },
        )
        .await?;
    Ok(Json(to_view(&state, incident).await?))
}

#[utoipa::path(
    post,
    path = "/v1/incidents/{incident_id}/postmortem",
    params(("incident_id" = String, Path)),
    request_body(content = Option<PublishPostmortemBody>),
    responses((status = 200, body = IncidentView))
)]
async fn publish_postmortem(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(incident_id): Path<IncidentId>,
    body: Option<Json<PublishPostmortemBody>>,
) -> Result<Json<IncidentView>> {
    let incident = find_incident(&state, &incident_id).await?;
    if incident.kind != KIND_INCIDENT || !is_closed(&incident.status) {
        return Err(invalid("Resolve the incident before publishing its postmortem"));
    }
    if incident.postmortem.is_none() {
        return Err(invalid("Write the postmortem before publishing"));
    }
    let is_first = incident.postmortem_published_at.is_none();
    let incident = state
        .database
        .publish_postmortem(&incident_id, Some(Utc::now()))
        .await?;
    let entry =
        if is_first { "Published the postmortem." } else { "Updated the postmortem." };
    if body.is_none_or(|Json(body)| body.notify) {
        let message = if is_first {
            "The postmortem is published."
        } else {
            "The postmortem was updated."
        };
        let app_url = state.config.app_url.as_deref();
        if let Err(error) = notify_incident(
            &state.database,
            app_url,
            &incident,
            &incident.status,
            message,
        )
        .await
        {
            tracing::warn!(incident_id = %incident.id, error = ?error, "notify a postmortem");
        }
    }
    add_entry(
        &state,
        &incident.id,
        AUTOMATIC,
        None,
        paragraph(entry),
        false,
        Some(current.user.id),
    )
    .await?;
    Ok(Json(to_view(&state, incident).await?))
}

#[utoipa::path(
    delete,
    path = "/v1/incidents/{incident_id}/postmortem",
    params(("incident_id" = String, Path)),
    responses((status = 200, body = IncidentView))
)]
async fn unpublish_postmortem(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(incident_id): Path<IncidentId>,
) -> Result<Json<IncidentView>> {
    let incident = find_incident(&state, &incident_id).await?;
    if incident.postmortem_published_at.is_none() {
        return Err(invalid("The postmortem is not published"));
    }
    let incident = state.database.publish_postmortem(&incident_id, None).await?;
    add_entry(
        &state,
        &incident.id,
        AUTOMATIC,
        None,
        paragraph("Unpublished the postmortem."),
        false,
        Some(current.user.id),
    )
    .await?;
    Ok(Json(to_view(&state, incident).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalizes_severity_names() {
        assert_eq!(capitalize("major"), "Major");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn keeps_the_first_closing_time() {
        let mut incident = Incident {
            id: "inc_1".to_string(),
            kind: KIND_INCIDENT.to_string(),
            title: "Down".to_string(),
            status: "investigating".to_string(),
            severity: None,
            lead_id: None,
            started_at: Utc::now(),
            resolved_at: None,
            starts_at: None,
            ends_at: None,
            auto_start: true,
            auto_finish: true,
            remind_before: true,
            reminded_at: None,
            components: serde_json::json!([]),
            related: serde_json::json!([]),
            responders: serde_json::json!([]),
            channel_ids: serde_json::json!([]),
            hidden_page_ids: serde_json::json!([]),
            follow_ups: serde_json::json!([]),
            postmortem: None,
            postmortem_published_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(closing_time(&incident, "monitoring").is_none());
        assert!(closing_time(&incident, "resolved").is_some());
        let first = Utc::now() - chrono::Duration::hours(1);
        incident.resolved_at = Some(first);
        assert_eq!(closing_time(&incident, "completed"), Some(first));
    }
}
