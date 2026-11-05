use serde::Serialize;
use utoipa::ToSchema;

use crate::servers::report::LastReport;

const WARNING_PERCENT: f64 = 90.0;

#[derive(Serialize, ToSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Pending,
    Healthy,
    Warning,
    Down,
}

pub fn get_server_status(
    is_connected: bool,
    mem_total: u64,
    last_report: Option<&LastReport>,
) -> ServerStatus {
    let Some(report) = last_report else {
        return ServerStatus::Pending;
    };
    if !is_connected {
        return ServerStatus::Down;
    }
    let state = &report.state;
    let is_busy = f64::from(state.cpu) >= WARNING_PERCENT
        || get_percent(state.mem_used, mem_total) >= WARNING_PERCENT
        || state
            .disks
            .iter()
            .any(|disk| get_percent(disk.disk_used, disk.disk_total) >= WARNING_PERCENT)
        || report.processes.iter().any(|process| {
            process.count == 0
                || process.containers.is_some_and(|containers| containers.is_short())
        })
        || report.ports.iter().any(|port| !port.open);
    if is_busy { ServerStatus::Warning } else { ServerStatus::Healthy }
}

pub fn get_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    used as f64 / total as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::report::{ContainerCount, ProcessState, ProcessStatus, State};

    use super::*;

    fn make_report(cpu: f32) -> LastReport {
        LastReport {
            state: State {
                taken_at: 0,
                uptime: 0,
                cpu,
                load1: 0.0,
                load5: 0.0,
                load15: 0.0,
                mem_used: 4,
                swap_used: 0,
                process_count: 0,
                tcp_conn_count: 0,
                udp_conn_count: 0,
                disks: Vec::new(),
                nics: Vec::new(),
            },
            processes: Vec::new(),
            ports: Vec::new(),
            top: Vec::new(),
        }
    }

    #[test]
    fn derives_the_status() {
        assert_eq!(get_server_status(true, 8, None), ServerStatus::Pending);
        assert_eq!(
            get_server_status(false, 8, Some(&make_report(10.0))),
            ServerStatus::Down
        );
        assert_eq!(
            get_server_status(true, 8, Some(&make_report(10.0))),
            ServerStatus::Healthy
        );
        assert_eq!(
            get_server_status(true, 8, Some(&make_report(95.0))),
            ServerStatus::Warning
        );
        assert_eq!(
            get_server_status(true, 4, Some(&make_report(10.0))),
            ServerStatus::Warning
        );
    }

    #[test]
    fn warns_when_a_docker_service_has_too_few_containers() {
        let with_running = |running: u32| {
            let mut report = make_report(10.0);
            report.processes.push(ProcessState {
                name: "Worker".to_string(),
                status: ProcessStatus::Running,
                count: running,
                started_at: None,
                cpu: 0.0,
                mem_used: 0,
                read_speed: 0,
                write_speed: 0,
                controllable: false,
                container: None,
                containers: Some(ContainerCount { running, min_running: 3 }),
            });
            report
        };

        assert_eq!(
            get_server_status(true, 8, Some(&with_running(3))),
            ServerStatus::Healthy
        );
        assert_eq!(
            get_server_status(true, 8, Some(&with_running(1))),
            ServerStatus::Warning
        );
    }
}
