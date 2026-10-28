use chrono::{DateTime, Utc};
use daroyan_core::db::entities::servers::Model as Server;
use daroyan_core::servers::report::LastReport;
use daroyan_core::servers::status::{ServerStatus, get_percent, get_server_status};
use daroyan_protocol::host::Host;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = ServerItem)]
pub struct ServerItemView {
    pub id: String,
    pub name: String,
    pub labels: Vec<String>,
    pub status: ServerStatus,
    pub private_ip: Option<String>,
    pub platform: Option<String>,
    pub platform_version: Option<String>,
    pub cpu_cores: Option<u32>,
    pub mem_total: Option<u64>,
    pub cpu: Option<f32>,
    pub mem_percent: Option<f64>,
    pub disk_percent: Option<f64>,
    pub uptime: Option<u64>,
    #[serde(with = "daroyan_core::time::option")]
    pub last_seen_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = ServerDetail)]
pub struct ServerDetailView {
    pub id: String,
    pub name: String,
    pub labels: Vec<String>,
    pub status: ServerStatus,
    pub host: Option<Host>,
    pub last_report: Option<LastReport>,
    #[serde(with = "daroyan_core::time::option")]
    pub last_seen_at: Option<DateTime<Utc>>,
    #[serde(with = "daroyan_core::time")]
    pub created_at: DateTime<Utc>,
}

struct ParsedServer {
    host: Option<Host>,
    report: Option<LastReport>,
    labels: Vec<String>,
}

fn parse_server(server: &Server) -> ParsedServer {
    ParsedServer {
        host: Host::deserialize(&server.host).ok(),
        report: server
            .last_report
            .as_ref()
            .and_then(|report| LastReport::deserialize(report).ok()),
        labels: Vec::deserialize(&server.labels).unwrap_or_else(|error| {
            tracing::warn!(server_id = %server.id, %error, "unreadable server labels");
            Vec::new()
        }),
    }
}

impl ServerItemView {
    pub fn new(server: Server, is_connected: bool) -> Self {
        let ParsedServer { host, report, labels } = parse_server(&server);
        let mem_total = host.as_ref().map_or(0, |host| host.mem_total);
        let state = report.as_ref().map(|report| &report.state);
        Self {
            status: get_server_status(is_connected, mem_total, report.as_ref()),
            private_ip: host.as_ref().and_then(|host| host.private_ip.clone()),
            platform: host.as_ref().map(|host| host.platform.clone()),
            platform_version: host.as_ref().map(|host| host.platform_version.clone()),
            cpu_cores: host.as_ref().map(|host| host.cpu_cores),
            mem_total: host.as_ref().map(|host| host.mem_total),
            cpu: state.map(|state| state.cpu),
            mem_percent: state.map(|state| get_percent(state.mem_used, mem_total)),
            disk_percent: state.map(|state| {
                get_percent(
                    state.disks.iter().map(|disk| disk.disk_used).sum(),
                    state.disks.iter().map(|disk| disk.disk_total).sum(),
                )
            }),
            uptime: state.map(|state| state.uptime),
            id: server.id,
            name: server.name,
            labels,
            last_seen_at: server.last_seen_at,
            created_at: server.created_at,
        }
    }
}

impl ServerDetailView {
    pub fn new(server: Server, is_connected: bool) -> Self {
        let ParsedServer { host, report, labels } = parse_server(&server);
        let mem_total = host.as_ref().map_or(0, |host| host.mem_total);
        Self {
            status: get_server_status(is_connected, mem_total, report.as_ref()),
            id: server.id,
            name: server.name,
            labels,
            host,
            last_report: report,
            last_seen_at: server.last_seen_at,
            created_at: server.created_at,
        }
    }
}
