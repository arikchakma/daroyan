use chrono::{DateTime, Utc};
use daroyan_core::analytics::uptime_results::DayBar;
use daroyan_core::db::entities::status_pages::Model as StatusPage;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const STATUS_OPERATIONAL: &str = "operational";
pub const STATUS_DEGRADED: &str = "degraded";
pub const STATUS_OUTAGE: &str = "outage";
pub const STATUS_UNKNOWN: &str = "unknown";
pub const PAGE_PARTIAL: &str = "partial";
pub const PAGE_MAJOR: &str = "major";
pub const PAGE_MAINTENANCE: &str = "maintenance";

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
#[schema(as = StatusPageComponent)]
pub struct ComponentRef {
    #[schema(example = "check")]
    pub kind: String,
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
#[schema(as = StatusPageSection)]
pub struct SectionRef {
    pub name: String,
    pub components: Vec<ComponentRef>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = StatusPage)]
pub struct StatusPageView {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub logo_url: Option<String>,
    pub is_public: bool,
    pub sections: Vec<SectionRef>,
    #[schema(example = "operational")]
    pub status: String,
    pub component_count: usize,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time")]
    pub updated_at: DateTime<Utc>,
}

impl StatusPageView {
    pub fn new(page: StatusPage, status: &str) -> Self {
        let sections = read_sections(&page);
        Self {
            component_count: sections
                .iter()
                .map(|section| section.components.len())
                .sum(),
            status: status.to_string(),
            sections,
            id: page.id,
            name: page.name,
            slug: page.slug,
            logo_url: page.logo_url,
            is_public: page.is_public,
            created_at: page.created_at,
            updated_at: page.updated_at,
        }
    }
}

pub fn read_sections(page: &StatusPage) -> Vec<SectionRef> {
    serde_json::from_value(page.sections.clone()).unwrap_or_default()
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusComponent)]
pub struct PublicComponent {
    pub name: String,
    #[schema(example = "check")]
    pub kind: String,
    #[schema(example = "operational")]
    pub status: String,
    pub uptime: Option<f64>,
    pub days: Vec<DayBar>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusSection)]
pub struct PublicSection {
    pub name: String,
    pub components: Vec<PublicComponent>,
}

#[derive(Clone, Serialize, ToSchema)]
#[schema(as = PublicStatusIncidentComponent)]
pub struct PublicIncidentComponent {
    pub name: String,
    #[schema(example = "outage")]
    pub status: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusUpdate)]
pub struct PublicUpdate {
    #[schema(example = "identified")]
    pub status: Option<String>,
    #[schema(value_type = std::collections::HashMap<String, serde_json::Value>)]
    pub body: serde_json::Value,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusActiveIncident)]
pub struct PublicActiveIncident {
    pub id: String,
    pub title: String,
    #[schema(example = "investigating")]
    pub status: String,
    #[schema(example = "major")]
    pub severity: Option<String>,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    pub components: Vec<PublicIncidentComponent>,
    pub latest_update: Option<PublicUpdate>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusMaintenance)]
pub struct PublicMaintenance {
    pub id: String,
    pub title: String,
    #[schema(example = "scheduled")]
    pub status: String,
    #[serde(with = "daroyan_core::time::option")]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub ends_at: Option<DateTime<Utc>>,
    pub components: Vec<PublicIncidentComponent>,
    pub latest_update: Option<PublicUpdate>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusPastIncident)]
pub struct PublicPastIncident {
    pub id: String,
    pub title: String,
    #[schema(example = "resolved")]
    pub status: String,
    #[schema(example = "major")]
    pub severity: Option<String>,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusIncident)]
pub struct PublicIncidentDetail {
    pub id: String,
    #[schema(example = "incident")]
    pub kind: String,
    pub title: String,
    #[schema(example = "resolved")]
    pub status: String,
    #[schema(example = "major")]
    pub severity: Option<String>,
    #[serde(with = "daroyan_core::time")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "daroyan_core::time::option")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time::option")]
    pub ends_at: Option<DateTime<Utc>>,
    pub components: Vec<PublicIncidentComponent>,
    pub updates: Vec<PublicUpdate>,
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    pub postmortem: Option<serde_json::Value>,
    pub follow_ups: Vec<PublicFollowUp>,
    #[serde(with = "daroyan_core::time::option")]
    pub postmortem_published_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusFollowUp)]
pub struct PublicFollowUp {
    pub body: String,
    pub is_done: bool,
}

#[derive(Serialize, ToSchema)]
#[schema(as = PublicStatusPage)]
pub struct PublicPage {
    pub name: String,
    pub logo_url: Option<String>,
    #[schema(example = "operational")]
    pub status: String,
    pub sections: Vec<PublicSection>,
    pub active_incidents: Vec<PublicActiveIncident>,
    pub maintenance: Vec<PublicMaintenance>,
    pub past_incidents: Vec<PublicPastIncident>,
    #[serde(with = "daroyan_core::time")]
    pub updated_at: DateTime<Utc>,
}
