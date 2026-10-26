use std::net::IpAddr;

use daroyan_protocol::host::Host;
use sysinfo::{Networks, System};

use crate::collect::network::is_reported;
use crate::config::Server;

pub fn collect_host(server: &Server, system: &System, networks: &Networks) -> Host {
    let hostname = System::host_name().unwrap_or_default();
    Host {
        name: server.name.clone().unwrap_or_else(|| hostname.clone()),
        labels: server.labels.clone(),
        hostname,
        platform: System::name().unwrap_or_default(),
        platform_version: System::os_version().unwrap_or_default(),
        kernel: System::kernel_version().unwrap_or_default(),
        arch: System::cpu_arch(),
        cpu: system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_default(),
        cpu_cores: system.cpus().len() as u32,
        mem_total: system.total_memory(),
        swap_total: system.total_swap(),
        private_ip: find_private_ip(networks),
        boot_time: System::boot_time(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn find_private_ip(networks: &Networks) -> Option<String> {
    networks
        .iter()
        .filter(|(name, _)| is_reported(name, &[]))
        .flat_map(|(_, data)| data.ip_networks())
        .find_map(|network| match network.addr {
            IpAddr::V4(addr) if addr.is_private() => Some(addr.to_string()),
            _ => None,
        })
}
