use chrono::{DateTime, Utc};
use daroyan_protocol::host::Host;
use daroyan_protocol::report::TopProcess;
use reqwest::Url;

use crate::alerts::message::Moment;
use crate::alerts::rules::{Condition, read_log_severity};
use crate::analytics::Analytics;
use crate::analytics::log_lines::{
    LogMatch, LogRun, OTLP_SERVER_ID, list_last_lines, list_top_messages,
    summarize_log_run,
};
use crate::analytics::server_events::{ProcessExit, find_last_exit};
use crate::analytics::server_states::{DiskTrend, fit_disk_trend};
use crate::analytics::uptime_results::{
    ResultItem, find_last_up, list_cert_expiries, list_recent_results,
};
use crate::db::entities::alerts::Model as Alert;
use crate::db::entities::servers::Model as Server;
use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::error::Result;
use crate::servers::report::LastReport;
use crate::uptime::probe::STATUS_UP;

const TREND_WINDOW_MS: i64 = 6 * 3_600_000;
const MIN_TREND_MS: i64 = 3_600_000;
const PSEUDO_FILESYSTEMS: [&str; 6] =
    ["tmpfs", "devtmpfs", "overlay", "squashfs", "proc", "sysfs"];
const TOP_MESSAGES: u32 = 3;
const LAST_LINES: u32 = 3;
const LAST_LINES_WINDOW_MS: i64 = 3_600_000;
const EXIT_SLACK_MS: i64 = 5 * 60_000;
const RECENT_RESULTS: u32 = 20;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum AlertFacts {
    #[default]
    None,
    Logs(LogFacts),
    Usage(UsageFacts),
    Process(ProcessFacts),
    Check(CheckFacts),
    Disk(DiskFacts),
    Offline(OfflineFacts),
    Cert(CertFacts),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogFacts {
    pub top: Vec<(String, u64)>,
    pub run: Option<LogRun>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UsageFacts {
    pub load1: f64,
    pub cores: u32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub top: Vec<TopProcess>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessFacts {
    pub exit: Option<ProcessExit>,
    pub last_lines: Vec<LogLine>,
    pub controllable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub at: DateTime<Utc>,
    pub severity: u8,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckFacts {
    pub url: String,
    pub interval_secs: u32,
    pub failures: usize,
    pub latest: Option<CheckResult>,
    pub last_up: Option<CheckResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub at: DateTime<Utc>,
    pub code: Option<u16>,
    pub response_ms: Option<u32>,
    pub error: Option<String>,
    pub dns_ms: Option<u32>,
    pub connect_ms: Option<u32>,
    pub tls_ms: Option<u32>,
    pub ttfb_ms: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiskFacts {
    pub mount: String,
    pub used: u64,
    pub total: u64,
    pub trend: Option<DiskTrend>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OfflineFacts {
    pub last_seen_at: Option<DateTime<Utc>>,
    pub agent_version: Option<String>,
    pub checks: Option<CheckCount>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckCount {
    pub passing: usize,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertFacts {
    pub domain: String,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct FactSources<'a> {
    pub analytics: &'a Analytics,
    pub server: Option<&'a Server>,
    pub check: Option<&'a UptimeCheck>,
    pub server_checks: Option<CheckCount>,
}

impl FactSources<'_> {
    pub async fn gather(
        &self,
        condition: &Condition,
        alert: &Alert,
        moment: Moment,
        now: DateTime<Utc>,
    ) -> Result<AlertFacts> {
        let now_ms = now.timestamp_millis();

        let facts = match condition {
            Condition::Logs { level, within_secs, service, text, .. } => {
                let Some(server_id) = alert.server_id.clone() else {
                    return Ok(AlertFacts::None);
                };

                let service = match server_id.as_str() {
                    OTLP_SERVER_ID => Some(alert.subject.clone()),
                    _ => service.clone(),
                };
                let matcher = LogMatch {
                    min_severity: read_log_severity(level).unwrap_or(17),
                    service,
                    text: text.clone(),
                };

                let window_ms = i64::from(*within_secs) * 1000;

                if moment.is_resolved() {
                    let run = summarize_log_run(
                        self.analytics,
                        server_id,
                        matcher,
                        alert.started_at.timestamp_millis() - window_ms,
                        window_ms,
                    )
                    .await?;

                    AlertFacts::Logs(LogFacts { top: Vec::new(), run: Some(run) })
                } else {
                    let top = list_top_messages(
                        self.analytics,
                        server_id,
                        matcher,
                        now_ms - window_ms,
                        TOP_MESSAGES,
                    )
                    .await?;

                    AlertFacts::Logs(LogFacts { top, run: None })
                }
            }
            Condition::Cpu { .. } | Condition::Memory { .. } | Condition::Load { .. } => {
                let (Some(report), Some(host)) = (self.report(), self.host()) else {
                    return Ok(AlertFacts::None);
                };

                AlertFacts::Usage(UsageFacts {
                    load1: report.state.load1,
                    cores: host.cpu_cores,
                    mem_used: report.state.mem_used,
                    mem_total: host.mem_total,
                    top: report.top,
                })
            }
            Condition::ProcessStopped { .. } => {
                let Some(server) = self.server else { return Ok(AlertFacts::None) };

                let exit = find_last_exit(
                    self.analytics,
                    server.id.clone(),
                    alert.subject.clone(),
                    alert.started_at.timestamp_millis() - EXIT_SLACK_MS,
                )
                .await?;

                let last_lines = list_last_lines(
                    self.analytics,
                    server.id.clone(),
                    alert.subject.clone(),
                    now_ms - LAST_LINES_WINDOW_MS,
                    LAST_LINES,
                )
                .await?
                .into_iter()
                .filter_map(|(at, severity, message)| {
                    Some(LogLine {
                        at: DateTime::from_timestamp_millis(at)?,
                        severity,
                        message,
                    })
                })
                .collect();

                let controllable = self.report().is_some_and(|report| {
                    report.processes.iter().any(|process| {
                        process.name == alert.subject && process.controllable
                    })
                });

                AlertFacts::Process(ProcessFacts { exit, last_lines, controllable })
            }
            Condition::Disk { .. } => {
                let (Some(server), Some(report)) = (self.server, self.report()) else {
                    return Ok(AlertFacts::None);
                };
                let Some(disk) =
                    report.state.disks.iter().find(|disk| disk.mount == alert.subject)
                else {
                    return Ok(AlertFacts::None);
                };

                let trend = if PSEUDO_FILESYSTEMS.contains(&disk.filesystem.as_str()) {
                    None
                } else {
                    fit_disk_trend(
                        self.analytics,
                        server.id.clone(),
                        disk.mount.clone(),
                        now_ms - TREND_WINDOW_MS,
                    )
                    .await?
                    .filter(|trend| trend.window_ms >= MIN_TREND_MS)
                };

                AlertFacts::Disk(DiskFacts {
                    mount: disk.mount.clone(),
                    used: disk.disk_used,
                    total: disk.disk_total,
                    trend,
                })
            }
            Condition::Offline => AlertFacts::Offline(OfflineFacts {
                last_seen_at: self.server.and_then(|server| server.last_seen_at),
                agent_version: self.host().map(|host| host.version),
                checks: self.server_checks,
            }),
            Condition::UptimeDown => {
                let Some(check) = self.check else { return Ok(AlertFacts::None) };

                let recent =
                    list_recent_results(self.analytics, check.id.clone(), RECENT_RESULTS)
                        .await?;
                let failures =
                    recent.iter().take_while(|result| result.status != STATUS_UP).count();
                let last_up = find_last_up(self.analytics, check.id.clone()).await?;

                AlertFacts::Check(CheckFacts {
                    url: check.target.clone(),
                    interval_secs: u32::try_from(check.interval_secs).unwrap_or_default(),
                    failures,
                    latest: recent.into_iter().next().and_then(CheckResult::read),
                    last_up: last_up.and_then(CheckResult::read),
                })
            }
            Condition::CertDays { .. } => {
                let Some(check) = self.check else { return Ok(AlertFacts::None) };

                let expires_at = list_cert_expiries(self.analytics)
                    .await?
                    .remove(&check.id)
                    .and_then(DateTime::from_timestamp_millis);

                AlertFacts::Cert(CertFacts {
                    domain: Url::parse(&check.target)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_string))
                        .unwrap_or_else(|| check.target.clone()),
                    expires_at,
                })
            }
            _ => AlertFacts::None,
        };

        Ok(facts)
    }

    fn report(&self) -> Option<LastReport> {
        serde_json::from_value(self.server?.last_report.clone()?).ok()
    }

    fn host(&self) -> Option<Host> {
        serde_json::from_value(self.server?.host.clone()).ok()
    }
}

impl CheckResult {
    fn read(item: ResultItem) -> Option<Self> {
        Some(Self {
            at: DateTime::from_timestamp_millis(item.at)?,
            code: item.code,
            response_ms: item.response_ms,
            error: item.error,
            dns_ms: item.dns_ms,
            connect_ms: item.connect_ms,
            tls_ms: item.tls_ms,
            ttfb_ms: item.ttfb_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    const MIGRATION_DISKS: &str =
        include_str!("../../../daroyan-migration/duckdb/0009_disk_states.sql");
    const MIGRATIONS: [&str; 5] = [
        include_str!("../../../daroyan-migration/duckdb/0001_server_states.sql"),
        include_str!("../../../daroyan-migration/duckdb/0002_log_lines.sql"),
        include_str!("../../../daroyan-migration/duckdb/0003_log_raw.sql"),
        include_str!("../../../daroyan-migration/duckdb/0004_uptime_results.sql"),
        include_str!("../../../daroyan-migration/duckdb/0005_uptime_timings.sql"),
    ];

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, hour, minute, 0).unwrap()
    }

    fn ms(hour: u32, minute: u32) -> i64 {
        at(hour, minute).timestamp_millis()
    }

    fn make_server() -> Server {
        Server {
            id: "srv_1".to_string(),
            name: "Inboxjot Server".to_string(),
            labels: json!(["production"]),
            host: json!({
                "name": "Inboxjot Server", "labels": ["production"], "hostname": "inboxjot",
                "platform": "Ubuntu", "platform_version": "24.04", "kernel": "6.8.0", "arch": "x86_64",
                "cpu": "EPYC", "cpu_cores": 3, "mem_total": 4_000, "swap_total": 0,
                "private_ip": "10.0.0.2", "boot_time": 1, "version": "0.0.3"
            }),
            token_hash: String::new(),
            last_report: Some(json!({
                "state": {
                    "taken_at": 1, "uptime": 1, "cpu": 96.0, "load1": 4.2, "load5": 1.0, "load15": 1.0,
                    "mem_used": 1_000, "swap_used": 0, "process_count": 1, "tcp_conn_count": 0,
                    "udp_conn_count": 0, "nics": [],
                    "disks": [{
                        "mount": "/", "filesystem": "ext4", "disk_total": 100, "disk_used": 90,
                        "inode_total": 0, "inode_used": 0, "read_speed": 0, "write_speed": 0
                    }]
                },
                "processes": [],
                "ports": []
            })),
            last_seen_at: Some(at(8, 58)),
            created_at: at(0, 0),
            updated_at: at(0, 0),
        }
    }

    fn make_check() -> UptimeCheck {
        UptimeCheck {
            id: "chk_1".to_string(),
            name: "API".to_string(),
            kind: "http".to_string(),
            target: "https://app.inboxjot.com/health".to_string(),
            method: "GET".to_string(),
            expected_status: None,
            body_contains: None,
            timeout_secs: 10,
            interval_secs: 60,
            paused: false,
            failure_threshold: 3,
            success_threshold: 2,
            retry_interval_secs: 30,
            created_at: at(0, 0),
            updated_at: at(0, 0),
        }
    }

    fn make_alert(subject: &str) -> Alert {
        Alert {
            id: "alt_1".to_string(),
            rule_id: "rul_1".to_string(),
            server_id: Some("srv_1".to_string()),
            check_id: Some("chk_1".to_string()),
            cron_id: None,
            subject: subject.to_string(),
            value: String::new(),
            severity: "critical".to_string(),
            status: "firing".to_string(),
            started_at: at(8, 50),
            resolved_at: None,
            last_notified_at: None,
            reminders_sent: 0,
        }
    }

    fn seed(conn: &duckdb::Connection) -> duckdb::Result<()> {
        for migration in MIGRATIONS {
            conn.execute_batch(migration)?;
        }

        let log = "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message) VALUES (?, ?, ?, 'stderr', ?, ?)";
        for (minute, message) in
            [(56, "export failed"), (57, "export failed"), (58, "db timeout")]
        {
            conn.execute(
                log,
                duckdb::params!["srv_1", ms(8, minute), "api", 17, message],
            )?;
        }

        conn.execute(
            log,
            duckdb::params!["srv_1", ms(8, 57), "api", 9, "export queued"],
        )?;
        conn.execute(
            log,
            duckdb::params!["srv_1", ms(8, 59), "worker", 13, "memory at 3.6 GB"],
        )?;
        conn.execute(
            "INSERT INTO server_events VALUES ('srv_1', ?, 'process_exited', '{\"name\":\"worker\",\"code\":137}')",
            [ms(8, 59)],
        )?;
        conn.execute(
            "INSERT INTO server_events VALUES ('srv_1', ?, 'process_exited', '{\"name\":\"api\",\"oom\":true}')",
            [ms(7, 0)],
        )?;

        conn.execute_batch(MIGRATION_DISKS)?;
        let disk = "INSERT INTO disk_states VALUES ('srv_1', ?, ?, ?, 100, ?)";
        for (hour, used) in [(4, 30), (5, 50), (6, 70), (7, 90)] {
            conn.execute(disk, duckdb::params![ms(hour, 0), "/", "ext4", used])?;
            conn.execute(disk, duckdb::params![ms(hour, 0), "/run", "tmpfs", used])?;
        }

        conn.execute(disk, duckdb::params![ms(1, 0), "/", "ext4", 0])?;

        let result = "INSERT INTO uptime_results (check_id, checked_at, status, code, response_ms, error, cert_expires_at, dns_ms, connect_ms, tls_ms, ttfb_ms) VALUES ('chk_1', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let expiry = Utc
            .with_ymd_and_hms(2026, 11, 9, 23, 59, 0)
            .unwrap()
            .timestamp_millis();
        conn.execute(
            result,
            duckdb::params![
                ms(8, 55),
                "up",
                200,
                118,
                None::<String>,
                expiry,
                10,
                20,
                30,
                40
            ],
        )?;
        conn.execute(
            result,
            duckdb::params![
                ms(8, 56),
                "down",
                None::<u16>,
                None::<u32>,
                "Timed out",
                None::<i64>,
                12,
                31,
                48,
                None::<u32>
            ],
        )?;
        conn.execute(
            result,
            duckdb::params![
                ms(8, 57),
                "down",
                None::<u16>,
                None::<u32>,
                "Timed out",
                None::<i64>,
                12,
                31,
                48,
                None::<u32>
            ],
        )?;

        Ok(())
    }

    #[tokio::test]
    async fn gathers_facts_from_analytics() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-facts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics.run(seed).unwrap();

        let (server, check) = (make_server(), make_check());
        let sources = FactSources {
            analytics: &analytics,
            server: Some(&server),
            check: Some(&check),
            server_checks: Some(CheckCount { passing: 1, total: 1 }),
        };

        let now = at(9, 0);
        let logs = Condition::Logs {
            level: "error".to_string(),
            count: 2,
            within_secs: 300,
            service: Some("api".to_string()),
            text: None,
        };
        let gather = |condition: Condition, subject: &'static str, moment: Moment| {
            let sources = &sources;
            async move {
                sources
                    .gather(&condition, &make_alert(subject), moment, now)
                    .await
                    .unwrap()
            }
        };

        assert_eq!(
            gather(logs.clone(), "api", Moment::Firing).await,
            AlertFacts::Logs(LogFacts {
                top: vec![
                    ("export failed".to_string(), 2),
                    ("db timeout".to_string(), 1)
                ],
                run: None,
            })
        );
        assert_eq!(
            gather(logs, "api", Moment::Resolved { lasted_secs: 600 }).await,
            AlertFacts::Logs(LogFacts {
                top: Vec::new(),
                run: Some(LogRun { total: 3, peak: 3, last_at: Some(ms(8, 58)) }),
            })
        );

        let AlertFacts::Process(process) =
            gather(Condition::ProcessStopped { name: None }, "worker", Moment::Firing)
                .await
        else {
            panic!("expected process facts");
        };
        assert_eq!(process.exit.unwrap().code, Some(137));
        assert_eq!(process.last_lines.len(), 1);
        assert_eq!(process.last_lines[0].message, "memory at 3.6 GB");

        let AlertFacts::Process(stale) =
            gather(Condition::ProcessStopped { name: None }, "api", Moment::Firing).await
        else {
            panic!("expected process facts");
        };
        assert_eq!(stale.exit, None);

        assert_eq!(
            gather(Condition::Disk { above: 85.0 }, "/", Moment::Firing).await,
            AlertFacts::Disk(DiskFacts {
                mount: "/".to_string(),
                used: 90,
                total: 100,
                trend: Some(DiskTrend { bytes_per_hour: 20.0, window_ms: 3 * 3_600_000 }),
            })
        );

        let AlertFacts::Check(down) =
            gather(Condition::UptimeDown, "Status", Moment::Firing).await
        else {
            panic!("expected check facts");
        };
        assert_eq!(down.failures, 2);
        assert_eq!(down.latest.unwrap().tls_ms, Some(48));
        assert_eq!(down.last_up.unwrap().response_ms, Some(118));

        assert_eq!(
            gather(Condition::CertDays { below: 14.0 }, "Certificate", Moment::Firing)
                .await,
            AlertFacts::Cert(CertFacts {
                domain: "app.inboxjot.com".to_string(),
                expires_at: Some(Utc.with_ymd_and_hms(2026, 11, 9, 23, 59, 0).unwrap()),
            })
        );

        assert_eq!(
            gather(Condition::Offline, "Server", Moment::Firing).await,
            AlertFacts::Offline(OfflineFacts {
                last_seen_at: Some(at(8, 58)),
                agent_version: Some("0.0.3".to_string()),
                checks: Some(CheckCount { passing: 1, total: 1 }),
            })
        );

        assert!(matches!(
            gather(Condition::Cpu { above: 90.0 }, "CPU", Moment::Firing).await,
            AlertFacts::Usage(UsageFacts { cores: 3, mem_total: 4_000, .. })
        ));

        std::fs::remove_dir_all(dir).ok();
    }
}
