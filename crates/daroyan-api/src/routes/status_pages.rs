use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use daroyan_core::analytics::cron_runs::summarize_cron_days;
use daroyan_core::analytics::uptime_results::{
    CHECK_WINDOWS, CRON_WINDOWS, CheckSummary, DAY_MS, DayBar, count_in_windows,
    summarize_checks, to_day_start,
};
use daroyan_core::auth::{find_access, find_active_session};
use daroyan_core::crons::{FAILED_RUN, MISSED_RUN};
use daroyan_core::db::entities::cron_failures::Model as CronFailure;
use daroyan_core::db::entities::crons::Model as CronModel;
use daroyan_core::db::entities::incident_updates::Model as IncidentUpdate;
use daroyan_core::db::entities::incidents::Model as Incident;
use daroyan_core::db::entities::status_pages::Model as StatusPage;
use daroyan_core::db::entities::uptime_checks::Model as UptimeCheck;
use daroyan_core::db::ids::{IncidentId, StatusPageId};
use daroyan_core::db::queries::status_pages::StatusPageFields;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use daroyan_core::incidents::{
    AffectedComponent, COMPONENT_STATUSES, FollowUp, IN_PROGRESS, KIND_MAINTENANCE,
    PARTIAL, SCHEDULED, is_closed, merge_status, rank, read_json,
};
use daroyan_core::rich_text::{escape_html, to_html};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::cookies::{SESSION_COOKIE, read_cookie};
use crate::extract::{AdminUser, CurrentUser};
use crate::views::status_page::{
    ComponentRef, PAGE_MAINTENANCE, PAGE_MAJOR, PAGE_PARTIAL, PublicActiveIncident,
    PublicComponent, PublicFollowUp, PublicIncidentComponent, PublicIncidentDetail,
    PublicMaintenance, PublicPage, PublicPastIncident, PublicSection, PublicUpdate,
    STATUS_DEGRADED, STATUS_OPERATIONAL, STATUS_OUTAGE, STATUS_UNKNOWN, SectionRef,
    StatusPageView, read_sections,
};

const DAYS: i64 = 90;
const PAST_DAYS: i64 = 14;
const MAX_NAME_LENGTH: usize = 100;
const MAX_SLUG_LENGTH: usize = 48;
const MAX_LOGO_LENGTH: usize = 280 * 1024;
const LOGO_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/svg+xml"];
const MAX_SECTIONS: usize = 20;
const MAX_COMPONENTS: usize = 100;
const KIND_CHECK: &str = "check";
const KIND_CRON: &str = "cron";
const PUBLIC_CACHE: &str = "public, max-age=30";
const PRIVATE_CACHE: &str = "private, no-store";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_status_pages, create_status_page))
        .routes(routes!(get_status_page, update_status_page, delete_status_page))
        .routes(routes!(get_public_status_page))
        .routes(routes!(get_public_incident))
        .routes(routes!(get_rss_feed))
        .routes(routes!(get_atom_feed))
}

#[derive(Deserialize, ToSchema)]
struct StatusPageBody {
    name: String,
    slug: String,
    logo_url: Option<String>,
    #[serde(default = "default_public")]
    is_public: bool,
    #[serde(default)]
    sections: Vec<SectionRef>,
}

fn default_public() -> bool {
    true
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorName::ValidationError, message)
}

fn not_found() -> Error {
    Error::new(ErrorName::NotFound, "Status page not found")
}

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG_LENGTH
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
        })
}

fn read_logo(logo_url: Option<String>) -> Result<Option<String>> {
    let Some(logo_url) = logo_url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
    else {
        return Ok(None);
    };
    let is_image = LOGO_TYPES
        .iter()
        .any(|kind| logo_url.starts_with(&format!("data:{kind};base64,")));
    if !is_image {
        return Err(invalid("The logo must be a PNG, JPEG, WebP or SVG image"));
    }
    if logo_url.len() > MAX_LOGO_LENGTH {
        return Err(invalid("The logo can be at most 200 KB"));
    }
    Ok(Some(logo_url))
}

async fn read_fields(
    state: &AppState,
    body: StatusPageBody,
    id: Option<&str>,
) -> Result<StatusPageFields> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(invalid("Add a name of up to 100 characters"));
    }
    let slug = body.slug.trim().to_lowercase();
    if !is_valid_slug(&slug) {
        return Err(invalid(
            "The slug can use up to 48 lowercase letters, numbers and dashes",
        ));
    }
    if state
        .database
        .find_status_page_by_slug(&slug)
        .await?
        .is_some_and(|page| Some(page.id.as_str()) != id)
    {
        return Err(Error::new(ErrorName::Conflict, "That slug is already taken"));
    }
    let logo_url = read_logo(body.logo_url)?;
    let sections = read_sections_body(state, body.sections).await?;
    Ok(StatusPageFields {
        name,
        slug,
        logo_url,
        is_public: body.is_public,
        sections: serde_json::to_value(sections).map_err(anyhow::Error::from)?,
    })
}

async fn read_sections_body(
    state: &AppState,
    sections: Vec<SectionRef>,
) -> Result<Vec<SectionRef>> {
    if sections.len() > MAX_SECTIONS {
        return Err(invalid("A status page can have up to 20 sections"));
    }
    if sections.iter().map(|section| section.components.len()).sum::<usize>()
        > MAX_COMPONENTS
    {
        return Err(invalid("A status page can have up to 100 components"));
    }
    let known: HashSet<(&str, String)> = state
        .database
        .list_uptime_checks()
        .await?
        .into_iter()
        .map(|check| (KIND_CHECK, check.id))
        .chain(
            state
                .database
                .list_crons()
                .await?
                .into_iter()
                .map(|cron| (KIND_CRON, cron.id)),
        )
        .collect();
    let mut seen = HashSet::new();
    sections
        .into_iter()
        .map(|section| {
            let name = section.name.trim().to_string();
            if name.is_empty() || name.len() > MAX_NAME_LENGTH {
                return Err(invalid("Name each section, up to 100 characters"));
            }
            let components = section
                .components
                .into_iter()
                .map(|component| {
                    let name = component.name.trim().to_string();
                    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
                        return Err(invalid("Name each component, up to 100 characters"));
                    }
                    if !known.contains(&(component.kind.as_str(), component.id.clone())) {
                        return Err(invalid(
                            "A component points to a monitor that does not exist",
                        ));
                    }
                    if !seen.insert((component.kind.clone(), component.id.clone())) {
                        return Err(invalid("Each monitor can appear once per page"));
                    }
                    Ok(ComponentRef { name, ..component })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(SectionRef { name, components })
        })
        .collect()
}

struct Monitors {
    checks: HashMap<String, UptimeCheck>,
    check_summaries: HashMap<String, CheckSummary>,
    open_checks: HashSet<String>,
    crons: HashMap<String, CronModel>,
    cron_days: HashMap<String, Vec<DayBar>>,
    open_crons: Vec<CronFailure>,
    incidents: Vec<OpenIncident>,
    excluded: HashMap<(String, String), HashMap<i64, DayBar>>,
}

async fn load_maintenance_counts(
    state: &AppState,
    now_ms: i64,
) -> Result<HashMap<(String, String), HashMap<i64, DayBar>>> {
    let since = to_day_start(now_ms, DAYS);
    let mut windows: HashMap<(String, String), Vec<(i64, i64)>> = HashMap::new();
    for incident in state.database.list_incidents().await? {
        if incident.kind != KIND_MAINTENANCE || incident.status == SCHEDULED {
            continue;
        }
        let start = incident.started_at.timestamp_millis();
        let end = incident.resolved_at.map_or(now_ms, |at| at.timestamp_millis());
        if end < since {
            continue;
        }
        for component in read_json::<Vec<AffectedComponent>>(&incident.components) {
            windows
                .entry((component.kind, component.id))
                .or_default()
                .push((start, end));
        }
    }
    let mut counts = HashMap::new();
    for ((kind, id), ranges) in windows {
        let source = match kind.as_str() {
            KIND_CHECK => &CHECK_WINDOWS,
            KIND_CRON => &CRON_WINDOWS,
            _ => continue,
        };
        let by_day =
            count_in_windows(&state.analytics, source, id.clone(), ranges).await?;
        counts.insert((kind, id), by_day);
    }
    Ok(counts)
}

struct OpenIncident {
    is_maintenance: bool,
    hidden_page_ids: HashSet<String>,
    components: Vec<AffectedComponent>,
}

impl OpenIncident {
    fn new(incident: Incident) -> Option<Self> {
        let is_maintenance = incident.kind == KIND_MAINTENANCE;
        if is_maintenance && incident.status != IN_PROGRESS {
            return None;
        }
        Some(Self {
            is_maintenance,
            hidden_page_ids: serde_json::from_value(incident.hidden_page_ids)
                .unwrap_or_default(),
            components: serde_json::from_value(incident.components).unwrap_or_default(),
        })
    }
}

impl Monitors {
    async fn load(state: &AppState) -> Result<Self> {
        let now_ms = Utc::now().timestamp_millis();
        Ok(Self {
            checks: state
                .database
                .list_uptime_checks()
                .await?
                .into_iter()
                .map(|check| (check.id.clone(), check))
                .collect(),
            check_summaries: summarize_checks(&state.analytics, now_ms, DAYS).await?,
            open_checks: state
                .database
                .list_open_uptime_outages()
                .await?
                .into_iter()
                .map(|outage| outage.check_id)
                .collect(),
            crons: state
                .database
                .list_crons()
                .await?
                .into_iter()
                .map(|cron| (cron.id.clone(), cron))
                .collect(),
            cron_days: summarize_cron_days(&state.analytics, now_ms, DAYS).await?,
            open_crons: state.database.list_open_cron_failures().await?,
            incidents: state
                .database
                .list_open_incidents()
                .await?
                .into_iter()
                .filter_map(OpenIncident::new)
                .collect(),
            excluded: load_maintenance_counts(state, now_ms).await?,
        })
    }

    fn read_status(
        &self,
        page_id: &str,
        component: &ComponentRef,
    ) -> Option<&'static str> {
        let monitor = self.read_monitor_status(component)?;
        let mut incident: Option<&'static str> = None;
        let mut is_maintenance = false;
        for open in self
            .incidents
            .iter()
            .filter(|open| !open.hidden_page_ids.contains(page_id))
        {
            for affected in open.components.iter().filter(|affected| {
                affected.kind == component.kind && affected.id == component.id
            }) {
                if open.is_maintenance {
                    is_maintenance = true;
                } else if let Some(status) = COMPONENT_STATUSES
                    .into_iter()
                    .find(|status| *status == affected.status)
                    && incident.is_none_or(|current| rank(status) > rank(current))
                {
                    incident = Some(status);
                }
            }
        }
        Some(merge_status(monitor, incident, is_maintenance))
    }

    fn read_monitor_status(&self, component: &ComponentRef) -> Option<&'static str> {
        match component.kind.as_str() {
            KIND_CHECK => {
                let check = self.checks.get(&component.id)?;
                let summary = self.check_summaries.get(&check.id);
                Some(
                    if check.paused
                        || summary.is_none_or(|summary| summary.checked_at.is_none())
                    {
                        STATUS_UNKNOWN
                    } else if self.open_checks.contains(&check.id) {
                        STATUS_OUTAGE
                    } else if summary.is_some_and(|summary| {
                        summary.status.as_deref() == Some("degraded")
                    }) {
                        STATUS_DEGRADED
                    } else {
                        STATUS_OPERATIONAL
                    },
                )
            }
            KIND_CRON => {
                let cron = self.crons.get(&component.id)?;
                let is_open = |kind: &str| {
                    self.open_crons
                        .iter()
                        .any(|outage| outage.cron_id == cron.id && outage.kind == kind)
                };
                Some(if cron.paused || cron.last_ping_at.is_none() {
                    STATUS_UNKNOWN
                } else if is_open(FAILED_RUN) {
                    STATUS_OUTAGE
                } else if is_open(MISSED_RUN) {
                    STATUS_DEGRADED
                } else {
                    STATUS_OPERATIONAL
                })
            }
            _ => None,
        }
    }

    fn read_days(&self, component: &ComponentRef) -> Vec<DayBar> {
        let days = match component.kind.as_str() {
            KIND_CHECK => {
                self.check_summaries.get(&component.id).map(|summary| &summary.days)
            }
            KIND_CRON => self.cron_days.get(&component.id),
            _ => None,
        };
        let excluded = self.excluded.get(&(component.kind.clone(), component.id.clone()));
        let days = days.filter(|days| !days.is_empty()).map(|days| {
            days.iter()
                .map(|day| match excluded.and_then(|excluded| excluded.get(&day.day)) {
                    Some(skip) => DayBar {
                        total: day.total.saturating_sub(skip.total),
                        down: day.down.saturating_sub(skip.down),
                        ..day.clone()
                    },
                    None => day.clone(),
                })
                .collect::<Vec<_>>()
        });
        days.unwrap_or_else(|| {
            let since = (Utc::now().timestamp_millis() / DAY_MS - DAYS + 1) * DAY_MS;
            (0..DAYS)
                .map(|index| DayBar { day: since + index * DAY_MS, ..Default::default() })
                .collect()
        })
    }

    fn build_component(
        &self,
        page_id: &str,
        component: &ComponentRef,
    ) -> Option<PublicComponent> {
        let status = self.read_status(page_id, component)?;
        let days = self.read_days(component);
        let total: u32 = days.iter().map(|day| day.total).sum();
        let down: u32 = days.iter().map(|day| day.down).sum();
        Some(PublicComponent {
            name: component.name.clone(),
            kind: component.kind.clone(),
            status: status.to_string(),
            uptime: (total > 0).then(|| f64::from(total - down) / f64::from(total)),
            days,
        })
    }

    fn build_sections(&self, page: &StatusPage) -> Vec<PublicSection> {
        read_sections(page)
            .into_iter()
            .map(|section| PublicSection {
                components: section
                    .components
                    .iter()
                    .filter_map(|component| self.build_component(&page.id, component))
                    .collect(),
                name: section.name,
            })
            .filter(|section| !section.components.is_empty())
            .collect()
    }

    fn read_page_status(&self, page: &StatusPage) -> &'static str {
        let sections: Vec<Vec<&str>> = read_sections(page)
            .iter()
            .map(|section| {
                section
                    .components
                    .iter()
                    .filter_map(|component| self.read_status(&page.id, component))
                    .collect()
            })
            .collect();
        summarize_sections(&sections)
    }
}

fn summarize_sections(sections: &[Vec<&str>]) -> &'static str {
    let is_major = sections.iter().any(|statuses| {
        let known: Vec<&&str> =
            statuses.iter().filter(|status| **status != STATUS_UNKNOWN).collect();
        !known.is_empty() && known.iter().all(|status| **status == STATUS_OUTAGE)
    });
    let is_partial = sections
        .iter()
        .flatten()
        .any(|status| [STATUS_OUTAGE, PARTIAL, STATUS_DEGRADED].contains(status));
    if is_major {
        PAGE_MAJOR
    } else if is_partial {
        PAGE_PARTIAL
    } else {
        STATUS_OPERATIONAL
    }
}

struct PageIncidents {
    names: HashMap<(String, String), String>,
    incidents: Vec<Incident>,
    latest: HashMap<String, PublicUpdate>,
}

impl PageIncidents {
    async fn load(state: &AppState, page: &StatusPage) -> Result<Self> {
        let names = read_sections(page)
            .into_iter()
            .flat_map(|section| section.components)
            .map(|component| ((component.kind, component.id), component.name))
            .collect();
        let mut loaded = Self {
            names,
            incidents: Vec::new(),
            latest: HashMap::new(),
        };
        loaded.incidents = state
            .database
            .list_incidents()
            .await?
            .into_iter()
            .filter(|incident| loaded.components(page, incident).is_some())
            .collect();
        let ids = loaded.incidents.iter().map(|incident| incident.id.clone()).collect();
        for update in state.database.list_public_updates(ids).await? {
            loaded
                .latest
                .entry(update.incident_id.clone())
                .or_insert_with(|| to_public_update(update));
        }
        Ok(loaded)
    }

    fn components(
        &self,
        page: &StatusPage,
        incident: &Incident,
    ) -> Option<Vec<PublicIncidentComponent>> {
        let hidden: Vec<String> = read_json(&incident.hidden_page_ids);
        if hidden.contains(&page.id) {
            return None;
        }
        let components: Vec<PublicIncidentComponent> =
            read_json::<Vec<AffectedComponent>>(&incident.components)
                .into_iter()
                .filter_map(|component| {
                    Some(PublicIncidentComponent {
                        name: self.names.get(&(component.kind, component.id))?.clone(),
                        status: component.status,
                    })
                })
                .collect();
        (!components.is_empty()).then_some(components)
    }

    fn summarize(
        &mut self,
        page: &StatusPage,
        now: DateTime<Utc>,
    ) -> (Vec<PublicActiveIncident>, Vec<PublicMaintenance>, Vec<PublicPastIncident>)
    {
        let since = now - chrono::Duration::days(PAST_DAYS);
        let mut active = Vec::new();
        let mut maintenance = Vec::new();
        let mut past = Vec::new();
        for incident in &self.incidents {
            let components = self.components(page, incident).unwrap_or_default();
            if incident.kind == KIND_MAINTENANCE {
                if !is_closed(&incident.status) {
                    maintenance.push(PublicMaintenance {
                        id: incident.id.clone(),
                        title: incident.title.clone(),
                        status: incident.status.clone(),
                        starts_at: incident.starts_at,
                        ends_at: incident.ends_at,
                        components,
                        latest_update: self.latest.remove(&incident.id),
                    });
                }
                continue;
            }
            if !is_closed(&incident.status) {
                active.push(PublicActiveIncident {
                    id: incident.id.clone(),
                    title: incident.title.clone(),
                    status: incident.status.clone(),
                    severity: incident.severity.clone(),
                    started_at: incident.started_at,
                    components,
                    latest_update: self.latest.remove(&incident.id),
                });
            }
            if incident.started_at >= since {
                past.push(PublicPastIncident {
                    id: incident.id.clone(),
                    title: incident.title.clone(),
                    status: incident.status.clone(),
                    severity: incident.severity.clone(),
                    started_at: incident.started_at,
                    resolved_at: incident.resolved_at,
                });
            }
        }
        maintenance.sort_by_key(|item| item.starts_at);
        (active, maintenance, past)
    }
}

fn to_public_update(update: IncidentUpdate) -> PublicUpdate {
    PublicUpdate {
        status: update.status,
        body: update.body,
        created_at: update.created_at,
    }
}

fn rank_page_status(status: &str) -> u8 {
    match status {
        PAGE_MAINTENANCE => 1,
        PAGE_PARTIAL => 2,
        PAGE_MAJOR => 3,
        _ => 0,
    }
}

fn combine_page_status(
    components: &'static str,
    active: &[PublicActiveIncident],
    maintenance: &[PublicMaintenance],
) -> &'static str {
    let from_incidents = active.iter().map(|incident| {
        if incident.severity.as_deref() == Some("critical") {
            PAGE_MAJOR
        } else {
            PAGE_PARTIAL
        }
    });
    let from_maintenance = maintenance
        .iter()
        .filter(|item| item.status == IN_PROGRESS)
        .map(|_| PAGE_MAINTENANCE);
    std::iter::once(components)
        .chain(from_incidents)
        .chain(from_maintenance)
        .max_by_key(|status| rank_page_status(status))
        .unwrap_or(STATUS_OPERATIONAL)
}

async fn find_visible_page(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<StatusPage> {
    let page = state
        .database
        .find_status_page_by_slug(slug)
        .await?
        .ok_or_else(not_found)?;
    if !page.is_public && !is_signed_in(state, headers).await? {
        return Err(not_found());
    }
    Ok(page)
}

fn cache_of(page: &StatusPage) -> &'static str {
    if page.is_public { PUBLIC_CACHE } else { PRIVATE_CACHE }
}

async fn find_page(state: &AppState, id: &StatusPageId) -> Result<StatusPage> {
    state.database.find_status_page(id).await?.ok_or_else(not_found)
}

async fn to_view(state: &AppState, page: StatusPage) -> Result<StatusPageView> {
    let monitors = Monitors::load(state).await?;
    let status = monitors.read_page_status(&page);
    Ok(StatusPageView::new(page, status))
}

#[utoipa::path(
    get,
    path = "/v1/status-pages",
    responses((status = 200, body = Vec<StatusPageView>))
)]
async fn list_status_pages(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<StatusPageView>>> {
    let pages = state.database.list_status_pages().await?;
    let monitors = Monitors::load(&state).await?;
    Ok(Json(
        pages
            .into_iter()
            .map(|page| {
                let status = monitors.read_page_status(&page);
                StatusPageView::new(page, status)
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/status-pages",
    request_body = StatusPageBody,
    responses((status = 201, body = StatusPageView))
)]
async fn create_status_page(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<StatusPageBody>,
) -> Result<(StatusCode, Json<StatusPageView>)> {
    let fields = read_fields(&state, body, None).await?;
    let page = state
        .database
        .create_status_page(new_id(Prefix::StatusPage), fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_view(&state, page).await?)))
}

#[utoipa::path(
    get,
    path = "/v1/status-pages/{page_id}",
    params(("page_id" = String, Path)),
    responses((status = 200, body = StatusPageView))
)]
async fn get_status_page(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(page_id): Path<StatusPageId>,
) -> Result<Json<StatusPageView>> {
    let page = find_page(&state, &page_id).await?;
    Ok(Json(to_view(&state, page).await?))
}

#[utoipa::path(
    put,
    path = "/v1/status-pages/{page_id}",
    params(("page_id" = String, Path)),
    request_body = StatusPageBody,
    responses((status = 200, body = StatusPageView))
)]
async fn update_status_page(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(page_id): Path<StatusPageId>,
    Json(body): Json<StatusPageBody>,
) -> Result<Json<StatusPageView>> {
    find_page(&state, &page_id).await?;
    let fields = read_fields(&state, body, Some(page_id.as_str())).await?;
    let page = state.database.update_status_page(&page_id, fields).await?;
    Ok(Json(to_view(&state, page).await?))
}

#[utoipa::path(
    delete,
    path = "/v1/status-pages/{page_id}",
    params(("page_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_status_page(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(page_id): Path<StatusPageId>,
) -> Result<StatusCode> {
    if !state.database.delete_status_page(&page_id).await? {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn is_signed_in(state: &AppState, headers: &HeaderMap) -> Result<bool> {
    let Some(token) = read_cookie(headers, SESSION_COOKIE) else {
        return Ok(false);
    };
    let Some((_, user)) = find_active_session(&state.database, &token).await? else {
        return Ok(false);
    };
    Ok(find_access(&state.database, &state.config, &user.email)
        .await?
        .is_some())
}

#[utoipa::path(
    get,
    path = "/v1/status/{slug}",
    params(("slug" = String, Path)),
    responses((status = 200, body = PublicPage))
)]
async fn get_public_status_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse> {
    let page = find_visible_page(&state, &headers, &slug).await?;
    let monitors = Monitors::load(&state).await?;
    let sections = monitors.build_sections(&page);
    let statuses: Vec<Vec<&str>> = sections
        .iter()
        .map(|section| {
            section
                .components
                .iter()
                .map(|component| component.status.as_str())
                .collect()
        })
        .collect();
    let now = Utc::now();
    let mut incidents = PageIncidents::load(&state, &page).await?;
    let (active_incidents, maintenance, past_incidents) = incidents.summarize(&page, now);
    let status = combine_page_status(
        summarize_sections(&statuses),
        &active_incidents,
        &maintenance,
    );
    let view = PublicPage {
        status: status.to_string(),
        active_incidents,
        maintenance,
        past_incidents,
        sections,
        name: page.name.clone(),
        logo_url: page.logo_url.clone(),
        updated_at: now,
    };
    Ok(([(header::CACHE_CONTROL, cache_of(&page))], Json(view)))
}

#[utoipa::path(
    get,
    path = "/v1/status/{slug}/incidents/{incident_id}",
    params(("slug" = String, Path), ("incident_id" = String, Path)),
    responses((status = 200, body = PublicIncidentDetail))
)]
async fn get_public_incident(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((slug, incident_id)): Path<(String, IncidentId)>,
) -> Result<impl IntoResponse> {
    let page = find_visible_page(&state, &headers, &slug).await?;
    let missing = || Error::new(ErrorName::NotFound, "Incident not found");
    let incident = state
        .database
        .find_incident(&incident_id)
        .await?
        .ok_or_else(missing)?;
    let incidents = PageIncidents::load(&state, &page).await?;
    let components = incidents.components(&page, &incident).ok_or_else(missing)?;
    let updates = state
        .database
        .list_public_updates(vec![incident.id.clone()])
        .await?
        .into_iter()
        .map(to_public_update)
        .collect();
    let is_published = incident.postmortem_published_at.is_some();
    let view = PublicIncidentDetail {
        id: incident.id,
        kind: incident.kind,
        title: incident.title,
        status: incident.status,
        severity: incident.severity,
        started_at: incident.started_at,
        resolved_at: incident.resolved_at,
        starts_at: incident.starts_at,
        ends_at: incident.ends_at,
        components,
        updates,
        follow_ups: if is_published {
            serde_json::from_value::<Vec<FollowUp>>(incident.follow_ups.clone())
                .unwrap_or_default()
                .into_iter()
                .map(|follow_up| PublicFollowUp {
                    body: follow_up.body,
                    is_done: follow_up.is_done,
                })
                .collect()
        } else {
            Vec::new()
        },
        postmortem: incident.postmortem.filter(|_| is_published),
        postmortem_published_at: incident.postmortem_published_at,
    };
    Ok(([(header::CACHE_CONTROL, cache_of(&page))], Json(view)))
}

struct FeedItem {
    title: String,
    link: String,
    id: String,
    at: DateTime<Utc>,
    html: String,
}

async fn list_feed_items(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(StatusPage, Vec<FeedItem>)> {
    let page = find_visible_page(state, headers, slug).await?;
    if !page.is_public {
        return Err(not_found());
    }
    let since = Utc::now() - chrono::Duration::days(DAYS);
    let mut incidents = PageIncidents::load(state, &page).await?;
    let base = state
        .config
        .app_url
        .as_deref()
        .unwrap_or_default()
        .trim_end_matches('/');
    let mut items: Vec<FeedItem> = std::mem::take(&mut incidents.incidents)
        .into_iter()
        .filter(|incident| incident.created_at >= since)
        .map(|incident| {
            let latest = incidents.latest.remove(&incident.id);
            FeedItem {
                link: format!("{base}/s/{}/incidents/{}", page.slug, incident.id),
                at: latest
                    .as_ref()
                    .map_or(incident.started_at, |update| update.created_at),
                html: latest.map(|update| to_html(&update.body)).unwrap_or_default(),
                title: incident.title,
                id: incident.id,
            }
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.at));
    Ok((page, items))
}

#[utoipa::path(
    get,
    path = "/v1/status/{slug}/feed.rss",
    params(("slug" = String, Path)),
    responses((status = 200, content_type = "application/rss+xml", body = String))
)]
async fn get_rss_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse> {
    let (page, items) = list_feed_items(&state, &headers, &slug).await?;
    let base = state
        .config
        .app_url
        .as_deref()
        .unwrap_or_default()
        .trim_end_matches('/');
    let entries: String = items
        .iter()
        .map(|item| {
            format!(
                "<item><title>{}</title><link>{}</link><guid isPermaLink=\"false\">{}</guid><pubDate>{}</pubDate><description>{}</description></item>",
                escape_html(&item.title),
                escape_html(&item.link),
                item.id,
                item.at.to_rfc2822(),
                escape_html(&item.html)
            )
        })
        .collect();
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\"><channel><title>{}</title><link>{base}/s/{}</link><description>Incidents and maintenance for {}</description>{entries}</channel></rss>",
        escape_html(&page.name),
        page.slug,
        escape_html(&page.name)
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"),
            (header::CACHE_CONTROL, PUBLIC_CACHE),
        ],
        xml,
    ))
}

#[utoipa::path(
    get,
    path = "/v1/status/{slug}/feed.atom",
    params(("slug" = String, Path)),
    responses((status = 200, content_type = "application/atom+xml", body = String))
)]
async fn get_atom_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse> {
    let (page, items) = list_feed_items(&state, &headers, &slug).await?;
    let base = state
        .config
        .app_url
        .as_deref()
        .unwrap_or_default()
        .trim_end_matches('/');
    let updated = items.first().map_or_else(Utc::now, |item| item.at);
    let entries: String = items
        .iter()
        .map(|item| {
            format!(
                "<entry><title>{}</title><id>{}</id><link href=\"{}\"/><updated>{}</updated><content type=\"html\">{}</content></entry>",
                escape_html(&item.title),
                escape_html(&item.link),
                escape_html(&item.link),
                item.at.to_rfc3339(),
                escape_html(&item.html)
            )
        })
        .collect();
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><feed xmlns=\"http://www.w3.org/2005/Atom\"><title>{}</title><id>{base}/s/{}</id><link href=\"{base}/s/{}\"/><updated>{}</updated>{entries}</feed>",
        escape_html(&page.name),
        page.slug,
        page.slug,
        updated.to_rfc3339()
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/atom+xml; charset=utf-8"),
            (header::CACHE_CONTROL, PUBLIC_CACHE),
        ],
        xml,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_simple_slugs() {
        for slug in ["acme", "acme-status", "a1", "x"] {
            assert!(is_valid_slug(slug), "{slug}");
        }
        for slug in
            ["", "-acme", "acme-", "Acme", "acme status", "acme_status", &"a".repeat(49)]
        {
            assert!(!is_valid_slug(slug), "{slug}");
        }
    }

    #[test]
    fn reports_a_major_outage_only_when_a_whole_section_is_down() {
        let summarize = |sections: &[&[&str]]| {
            summarize_sections(
                &sections.iter().map(|section| section.to_vec()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(summarize(&[]), STATUS_OPERATIONAL);
        assert_eq!(summarize(&[&[STATUS_UNKNOWN]]), STATUS_OPERATIONAL);
        assert_eq!(summarize(&[&[STATUS_OPERATIONAL, STATUS_DEGRADED]]), PAGE_PARTIAL);
        assert_eq!(summarize(&[&[STATUS_OPERATIONAL, STATUS_OUTAGE]]), PAGE_PARTIAL);
        assert_eq!(
            summarize(&[&[STATUS_OUTAGE, STATUS_UNKNOWN], &[STATUS_OPERATIONAL]]),
            PAGE_MAJOR
        );
    }
}
