pub mod connections;
pub mod disk;
pub mod host;
pub mod network;
pub mod top;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::config::Config;
use daroyan_protocol::report::State;
use sysinfo::{
    Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
};

pub struct Collector {
    pub system: System,
    pub disks: Disks,
    pub networks: Networks,
    refreshed_at: Instant,
    pub elapsed_secs: f64,
}

impl Collector {
    pub fn new() -> Self {
        let mut collector = Self {
            system: System::new(),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            refreshed_at: Instant::now(),
            elapsed_secs: 0.0,
        };
        collector.system.refresh_cpu_all();
        collector.refresh();
        collector
    }

    pub fn refresh(&mut self) {
        self.elapsed_secs = self.refreshed_at.elapsed().as_secs_f64();
        self.refreshed_at = Instant::now();
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        self.disks.refresh(true);
        self.networks.refresh(true);
    }

    pub fn collect_state(&self, config: &Config) -> State {
        let load = System::load_average();
        let (tcp_conn_count, udp_conn_count) = connections::count_connections();
        State {
            taken_at: now_millis(),
            uptime: System::uptime(),
            cpu: self.system.global_cpu_usage(),
            load1: load.one,
            load5: load.five,
            load15: load.fifteen,
            mem_used: self.system.used_memory(),
            swap_used: self.system.used_swap(),
            process_count: self
                .system
                .processes()
                .values()
                .filter(|process| process.thread_kind().is_none())
                .count() as u32,
            tcp_conn_count,
            udp_conn_count,
            disks: disk::collect_disks(
                &self.disks,
                &config.host.disks,
                self.elapsed_secs,
            ),
            nics: network::collect_nics(
                &self.networks,
                &config.host.networks,
                self.elapsed_secs,
            ),
        }
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn per_sec(bytes: u64, elapsed_secs: f64) -> u64 {
    if elapsed_secs <= 0.0 {
        return 0;
    }
    (bytes as f64 / elapsed_secs) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_sec_divides_by_elapsed_and_guards_zero() {
        assert_eq!(per_sec(1000, 10.0), 100);
        assert_eq!(per_sec(1000, 0.0), 0);
    }
}
