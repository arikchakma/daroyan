use crate::alerts::rules::{Condition, format_duration};
use crate::servers::report::LastReport;
use crate::servers::status::get_percent;

#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub subject: String,
    pub value: f64,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warning,
    Critical,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn read(value: &str) -> Self {
        if value == "warning" { Self::Warning } else { Self::Critical }
    }
}

pub fn is_past(condition: &Condition, value: f64, limit: f64) -> bool {
    if condition.is_below() { value < limit } else { value > limit }
}

pub fn rate(condition: &Condition, warning: Option<f64>, value: f64) -> Level {
    let Some(critical) = condition.critical() else {
        return Level::Critical;
    };
    if is_past(condition, value, critical)
        || (matches!(condition, Condition::Logs { .. }) && value >= critical)
    {
        Level::Critical
    } else if warning.is_some_and(|warning| is_past(condition, value, warning)) {
        Level::Warning
    } else {
        Level::Ok
    }
}

pub struct ServerSnapshot<'a> {
    pub is_connected: bool,
    pub mem_total: u64,
    pub report: Option<&'a LastReport>,
    pub log_count: u64,
}

pub fn find_readings(
    condition: &Condition,
    server: &ServerSnapshot,
) -> Option<Vec<Reading>> {
    if let Condition::Logs { level, within_secs, service, .. } = condition {
        let found = server.log_count;
        return Some(vec![make_reading(
            service.as_deref().unwrap_or("Logs"),
            found as f64,
            format!("{found} {level} lines in {}", format_duration(*within_secs)),
        )]);
    }
    let report = server.report?;
    if let Condition::Offline = condition {
        return Some(if server.is_connected {
            Vec::new()
        } else {
            vec![make_reading("Server", 1.0, "Offline".to_string())]
        });
    }
    if !server.is_connected {
        return None;
    }
    let state = &report.state;
    let readings = match condition {
        Condition::Cpu { .. } => {
            let cpu = f64::from(state.cpu);
            vec![make_reading("CPU", cpu, format_percent(cpu))]
        }
        Condition::Memory { .. } => {
            let memory = get_percent(state.mem_used, server.mem_total);
            vec![make_reading("Memory", memory, format_percent(memory))]
        }
        Condition::Disk { .. } => state
            .disks
            .iter()
            .map(|disk| {
                let used = get_percent(disk.disk_used, disk.disk_total);
                make_reading(&disk.mount, used, format_percent(used))
            })
            .collect(),
        Condition::Load { .. } => {
            vec![make_reading("Load", state.load1, format!("{:.2}", state.load1))]
        }
        Condition::ProcessStopped { name } => report
            .processes
            .iter()
            .filter(|process| name.as_ref().is_none_or(|name| &process.name == name))
            .filter(|process| process.count == 0)
            .map(|process| make_reading(&process.name, 1.0, "Stopped".to_string()))
            .collect(),
        Condition::PortClosed { name } => report
            .ports
            .iter()
            .map(|port| {
                let label = port
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}:{}", port.host, port.port));
                (port, label)
            })
            .filter(|(_, label)| name.as_ref().is_none_or(|name| label == name))
            .filter(|(port, _)| !port.open)
            .map(|(_, label)| make_reading(&label, 1.0, "Closed".to_string()))
            .collect(),
        _ => Vec::new(),
    };
    Some(readings)
}

pub fn make_reading(subject: &str, value: f64, label: String) -> Reading {
    Reading { subject: subject.to_string(), value, label }
}

pub fn format_percent(value: f64) -> String {
    format!("{value:.0}%")
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::report::{
        DiskState, PortState, ProcessState, ProcessStatus, State,
    };

    use super::*;

    fn make_report() -> LastReport {
        LastReport {
            state: State {
                taken_at: 0,
                uptime: 0,
                cpu: 96.4,
                load1: 0.5,
                load5: 0.0,
                load15: 0.0,
                mem_used: 50,
                swap_used: 0,
                process_count: 0,
                tcp_conn_count: 0,
                udp_conn_count: 0,
                disks: vec![DiskState {
                    mount: "/".to_string(),
                    filesystem: "ext4".to_string(),
                    disk_total: 100,
                    disk_used: 93,
                    inode_total: 0,
                    inode_used: 0,
                    read_speed: 0,
                    write_speed: 0,
                }],
                nics: Vec::new(),
            },
            processes: vec![make_process("Nginx", 4), make_process("Inboxing Worker", 0)],
            ports: vec![PortState {
                name: Some("Redis".to_string()),
                host: "redis-1".to_string(),
                port: 6379,
                open: false,
                latency_ms: None,
            }],
            top: Vec::new(),
        }
    }

    fn make_process(name: &str, count: u32) -> ProcessState {
        ProcessState {
            name: name.to_string(),
            status: if count > 0 {
                ProcessStatus::Running
            } else {
                ProcessStatus::Stopped
            },
            count,
            started_at: None,
            cpu: 0.0,
            mem_used: 0,
            read_speed: 0,
            write_speed: 0,
            controllable: true,
            container: None,
        }
    }

    fn evaluate(condition: Condition, is_connected: bool) -> Option<Vec<Reading>> {
        let report = make_report();
        let server = ServerSnapshot {
            is_connected,
            mem_total: 100,
            report: Some(&report),
            log_count: 0,
        };
        find_readings(&condition, &server)
    }

    fn breaching(condition: Condition, is_connected: bool) -> Vec<String> {
        evaluate(condition.clone(), is_connected)
            .unwrap()
            .into_iter()
            .filter(|reading| rate(&condition, None, reading.value) != Level::Ok)
            .map(|reading| reading.subject)
            .collect()
    }

    #[test]
    fn finds_breaches_in_a_report() {
        assert_eq!(
            evaluate(Condition::Cpu { above: 90.0 }, true).unwrap(),
            [Reading {
                subject: "CPU".to_string(),
                value: 96.4000015258789,
                label: "96%".to_string()
            }]
        );
        assert!(breaching(Condition::Memory { above: 60.0 }, true).is_empty());
        assert_eq!(breaching(Condition::Disk { above: 90.0 }, true), ["/"]);
        assert_eq!(
            breaching(Condition::ProcessStopped { name: None }, true),
            ["Inboxing Worker"]
        );
        assert!(
            breaching(
                Condition::ProcessStopped { name: Some("Nginx".to_string()) },
                true
            )
            .is_empty()
        );
        assert_eq!(breaching(Condition::PortClosed { name: None }, true), ["Redis"]);
    }

    #[test]
    fn rates_warning_and_critical_levels() {
        let cpu = Condition::Cpu { above: 90.0 };
        assert_eq!(rate(&cpu, Some(70.0), 95.0), Level::Critical);
        assert_eq!(rate(&cpu, Some(70.0), 80.0), Level::Warning);
        assert_eq!(rate(&cpu, Some(70.0), 60.0), Level::Ok);
        let cert = Condition::CertDays { below: 7.0 };
        assert_eq!(rate(&cert, Some(14.0), 3.0), Level::Critical);
        assert_eq!(rate(&cert, Some(14.0), 10.0), Level::Warning);
        assert_eq!(rate(&cert, Some(14.0), 30.0), Level::Ok);
    }

    #[test]
    fn only_the_offline_rule_runs_while_a_server_is_offline() {
        assert_eq!(evaluate(Condition::Cpu { above: 90.0 }, false), None);
        assert_eq!(breaching(Condition::Offline, false), ["Server"]);
        assert!(evaluate(Condition::Offline, true).unwrap().is_empty());
        let server = ServerSnapshot {
            is_connected: false,
            mem_total: 0,
            report: None,
            log_count: 0,
        };
        assert_eq!(find_readings(&Condition::Offline, &server), None);
    }
}
