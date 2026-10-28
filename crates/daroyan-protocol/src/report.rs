use serde::{Deserialize, Serialize};

use crate::host::Host;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Report {
    pub host: Option<Host>,
    pub state: State,
    pub processes: Vec<ProcessState>,
    pub ports: Vec<PortState>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub top: Vec<TopProcess>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct TopProcess {
    pub name: String,
    pub count: u32,
    pub cpu: f32,
    pub mem_used: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct State {
    pub taken_at: u64,
    pub uptime: u64,
    pub cpu: f32,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub mem_used: u64,
    pub swap_used: u64,
    pub process_count: u32,
    pub tcp_conn_count: u32,
    pub udp_conn_count: u32,
    pub disks: Vec<DiskState>,
    pub nics: Vec<NicState>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct DiskState {
    pub mount: String,
    pub filesystem: String,
    pub disk_total: u64,
    pub disk_used: u64,
    pub inode_total: u64,
    pub inode_used: u64,
    pub read_speed: u64,
    pub write_speed: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct NicState {
    pub name: String,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub net_in_transfer: u64,
    pub net_out_transfer: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Stopped,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct ProcessState {
    pub name: String,
    pub status: ProcessStatus,
    pub count: u32,
    pub started_at: Option<u64>,
    pub cpu: f32,
    pub mem_used: u64,
    pub read_speed: u64,
    pub write_speed: u64,
    #[serde(default)]
    pub controllable: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct PortState {
    #[serde(default)]
    pub name: Option<String>,
    pub host: String,
    pub port: u16,
    pub open: bool,
    pub latency_ms: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Event {
    pub at: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    AgentStarted { version: String },
    ProcessExited { name: String, code: Option<i32> },
    ProcessRestarted { name: String },
    ProcessRestartFailed { name: String },
    ProcessRestartGaveUp { name: String, attempts: u32 },
}
