use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Host {
    pub name: String,
    pub labels: Vec<String>,
    pub hostname: String,
    pub platform: String,
    pub platform_version: String,
    pub kernel: String,
    pub arch: String,
    pub cpu: String,
    pub cpu_cores: u32,
    pub mem_total: u64,
    pub swap_total: u64,
    pub private_ip: Option<String>,
    pub boot_time: u64,
    pub version: String,
}
