use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use daroyan_protocol::host::Host;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::alerts::deploys::join_names;
use crate::alerts::facts::PSEUDO_FILESYSTEMS;
use crate::alerts::message::format_bytes;
use crate::alerts::rules::format_duration;
use crate::analytics::Analytics;
use crate::analytics::server_states::{DiskGrowth, list_disk_growth};
use crate::analytics::uptime_results::list_cert_expiries;
use crate::db::Database;
use crate::db::entities::incidents::Model as Incident;
use crate::db::entities::servers::Model as Server;
use crate::db::entities::status_pages::Model as StatusPage;
use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::error::{Error, Result};
use crate::incidents::{
    AffectedComponent, KIND_MAINTENANCE, RelatedItem, SCHEDULED, read_json,
};

const DAY_MS: i64 = 86_400_000;
const WINDOW_MS: i64 = 30 * DAY_MS;
const DISK_HISTORY_MS: i64 = 7 * DAY_MS;
const MIN_DISK_HISTORY_MS: i64 = DAY_MS;
const STALE_DISK_MS: i64 = DAY_MS;
const DISK_SOON_MS: i64 = 7 * DAY_MS;
const CERT_SOON_MS: i64 = 14 * DAY_MS;
const MAINTENANCE_SOON_MS: i64 = 2 * DAY_MS;
const LISTED_AGENTS: usize = 3;

pub const LATEST_AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum UpcomingKind {
    DiskFull,
    CertExpiry,
    Maintenance,
    AgentOutdated,
}

#[derive(Serialize, ToSchema, Debug, PartialEq)]
pub struct UpcomingItem {
    pub kind: UpcomingKind,
    pub at: Option<i64>,
    pub soon: bool,
    pub title: String,
    pub detail: String,
    pub target_id: Option<String>,
}

#[derive(Serialize, ToSchema, Debug)]
#[schema(as = OverviewUpcoming)]
pub struct Upcoming {
    pub items: Vec<UpcomingItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    is_release: bool,
}

impl FromStr for Version {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim().trim_start_matches('v');
        let (core, is_release) = match text.find(['-', '+']) {
            Some(index) => (&text[..index], text[index..].starts_with('+')),
            None => (text, true),
        };

        let mut parts = core.split('.').map(|part| part.parse::<u64>().map_err(|_| ()));
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(());
        };

        Ok(Self {
            major: major?,
            minor: minor?,
            patch: patch?,
            is_release,
        })
    }
}

#[derive(Deserialize)]
struct PageSection {
    #[serde(default)]
    components: Vec<RelatedItem>,
}

pub async fn list_upcoming(
    database: &Database,
    analytics: &Analytics,
    now: DateTime<Utc>,
) -> Result<Upcoming> {
    let now_ms = now.timestamp_millis();

    let stored = async {
        Ok::<_, Error>(tokio::try_join!(
            database.list_servers(),
            database.list_uptime_checks(),
            database.list_open_incidents(),
            database.list_status_pages(),
        )?)
    };

    let ((servers, checks, incidents, pages), growth, expiries) = tokio::try_join!(
        stored,
        list_disk_growth(analytics, now_ms - DISK_HISTORY_MS, MIN_DISK_HISTORY_MS),
        list_cert_expiries(analytics),
    )?;

    let mut items = Vec::new();
    items.extend(list_full_disks(&servers, &growth, now_ms));
    items.extend(list_expiring_certs(&checks, &expiries, now_ms));
    items.extend(list_maintenance(&incidents, &pages, now));

    let versions: Vec<(&str, String)> = servers
        .iter()
        .filter_map(|server| {
            let host: Host = serde_json::from_value(server.host.clone()).ok()?;
            Some((server.name.as_str(), host.version))
        })
        .collect();
    items.extend(find_outdated_agents(&versions, LATEST_AGENT_VERSION));

    items.sort_by_key(|item| (item.at.is_none(), item.at));

    Ok(Upcoming { items })
}

fn list_full_disks(
    servers: &[Server],
    growth: &[DiskGrowth],
    now_ms: i64,
) -> Vec<UpcomingItem> {
    let names: HashMap<&str, &str> = servers
        .iter()
        .map(|server| (server.id.as_str(), server.name.as_str()))
        .collect();

    growth
        .iter()
        .filter(|disk| !PSEUDO_FILESYSTEMS.contains(&disk.filesystem.as_str()))
        .filter_map(|disk| {
            let name = names.get(disk.server_id.as_str())?;
            if now_ms - disk.taken_at > STALE_DISK_MS {
                return None;
            }

            let full_at = project_full_at(disk)?;
            let remaining = full_at - now_ms;
            if remaining <= 0 || remaining > WINDOW_MS {
                return None;
            }

            let percent = (disk.used as f64 / disk.total as f64 * 100.0).round();
            let per_day = format_bytes((disk.bytes_per_ms * DAY_MS as f64) as u64);

            Some(UpcomingItem {
                kind: UpcomingKind::DiskFull,
                at: Some(full_at),
                soon: remaining <= DISK_SOON_MS,
                title: format!("{name} disk fills {}", fills_in(remaining)),
                detail: format!(
                    "{} is at {percent}% and grew {per_day} a day this week.",
                    disk.mount
                ),
                target_id: Some(disk.server_id.clone()),
            })
        })
        .collect()
}

fn project_full_at(disk: &DiskGrowth) -> Option<i64> {
    if disk.bytes_per_ms <= 0.0 || disk.total == 0 {
        return None;
    }

    let free = disk.total.saturating_sub(disk.used) as f64;
    Some(disk.taken_at + (free / disk.bytes_per_ms) as i64)
}

fn fills_in(remaining_ms: i64) -> String {
    if remaining_ms < DAY_MS {
        return "within a day".to_string();
    }

    match (remaining_ms as f64 / DAY_MS as f64).round() as i64 {
        1 => "in about a day".to_string(),
        days => format!("in about {days} days"),
    }
}

fn list_expiring_certs(
    checks: &[UptimeCheck],
    expiries: &HashMap<String, i64>,
    now_ms: i64,
) -> Vec<UpcomingItem> {
    checks
        .iter()
        .filter(|check| !check.paused)
        .filter_map(|check| {
            let expires_at = *expiries.get(&check.id)?;
            let remaining = expires_at - now_ms;
            if remaining <= 0 || remaining > WINDOW_MS {
                return None;
            }

            let host = Url::parse(&check.target)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_else(|| check.target.clone());

            Some(UpcomingItem {
                kind: UpcomingKind::CertExpiry,
                at: Some(expires_at),
                soon: remaining <= CERT_SOON_MS,
                title: format!(
                    "Certificate for {host} expires {}",
                    expires_in(remaining)
                ),
                detail: format!("Seen by the {} check.", check.name),
                target_id: Some(check.id.clone()),
            })
        })
        .collect()
}

fn expires_in(remaining_ms: i64) -> String {
    match remaining_ms / DAY_MS {
        0 => "within a day".to_string(),
        1 => "in 1 day".to_string(),
        days => format!("in {days} days"),
    }
}

fn list_maintenance(
    incidents: &[Incident],
    pages: &[StatusPage],
    now: DateTime<Utc>,
) -> Vec<UpcomingItem> {
    incidents
        .iter()
        .filter(|incident| {
            incident.kind == KIND_MAINTENANCE && incident.status == SCHEDULED
        })
        .filter_map(|incident| {
            let starts_at = incident.starts_at?;
            let remaining = (starts_at - now).num_milliseconds();
            if remaining <= 0 || remaining > WINDOW_MS {
                return None;
            }

            let mut detail = match incident.ends_at {
                Some(ends_at) if ends_at > starts_at => {
                    let secs = (ends_at - starts_at).num_seconds();
                    format!(
                        "Scheduled maintenance for {}",
                        format_duration(u32::try_from(secs).unwrap_or(u32::MAX))
                    )
                }
                _ => "Scheduled maintenance".to_string(),
            };
            detail.push('.');
            if is_announced(incident, pages) {
                detail.push_str(" Your status page announces it.");
            }

            Some(UpcomingItem {
                kind: UpcomingKind::Maintenance,
                at: Some(starts_at.timestamp_millis()),
                soon: remaining <= MAINTENANCE_SOON_MS,
                title: incident.title.clone(),
                detail,
                target_id: Some(incident.id.clone()),
            })
        })
        .collect()
}

fn is_announced(incident: &Incident, pages: &[StatusPage]) -> bool {
    let hidden: HashSet<String> = read_json(&incident.hidden_page_ids);
    let affected: HashSet<(String, String)> =
        read_json::<Vec<AffectedComponent>>(&incident.components)
            .into_iter()
            .map(|component| (component.kind, component.id))
            .collect();

    pages.iter().filter(|page| !hidden.contains(&page.id)).any(|page| {
        read_json::<Vec<PageSection>>(&page.sections)
            .into_iter()
            .flat_map(|section| section.components)
            .any(|component| affected.contains(&(component.kind, component.id)))
    })
}

fn find_outdated_agents(agents: &[(&str, String)], latest: &str) -> Option<UpcomingItem> {
    let latest_version = latest.parse::<Version>().ok()?;

    let behind: Vec<(&str, Version, &str)> = agents
        .iter()
        .filter_map(|(name, version)| {
            let parsed = version.parse::<Version>().ok()?;
            (parsed < latest_version).then_some((*name, parsed, version.as_str()))
        })
        .collect();
    if behind.is_empty() {
        return None;
    }

    let mut versions: Vec<(Version, &str)> =
        behind.iter().map(|&(_, version, text)| (version, text)).collect();
    versions.sort();
    versions.dedup_by_key(|(version, _)| *version);
    let versions: Vec<&str> = versions.into_iter().map(|(_, text)| text).collect();

    let names: Vec<&str> = behind.iter().map(|&(name, _, _)| name).collect();

    Some(UpcomingItem {
        kind: UpcomingKind::AgentOutdated,
        at: None,
        soon: false,
        title: match names.len() {
            1 => format!("1 agent is behind {latest}"),
            count => format!("{count} agents are behind {latest}"),
        },
        detail: format!(
            "{} {} {}.",
            list_names(&names),
            if names.len() == 1 { "runs" } else { "run" },
            join_names(&versions)
        ),
        target_id: None,
    })
}

fn list_names(names: &[&str]) -> String {
    if names.len() <= LISTED_AGENTS {
        return join_names(names);
    }

    format!(
        "{} and {} more",
        names[..LISTED_AGENTS].join(", "),
        names.len() - LISTED_AGENTS
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    const MIGRATION_DISKS: &str =
        include_str!("../../daroyan-migration/duckdb/0009_disk_states.sql");

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
    }

    fn make_server(id: &str, name: &str) -> Server {
        Server {
            id: id.to_string(),
            name: name.to_string(),
            labels: json!([]),
            host: json!({}),
            token_hash: String::new(),
            last_report: None,
            last_seen_at: None,
            created_at: at(1, 0),
            updated_at: at(1, 0),
        }
    }

    fn make_growth(used: u64, bytes_per_day: f64) -> DiskGrowth {
        DiskGrowth {
            server_id: "srv_1".to_string(),
            mount: "/var/lib/clickhouse".to_string(),
            filesystem: "ext4".to_string(),
            taken_at: at(3, 0).timestamp_millis(),
            used,
            total: 1000,
            bytes_per_ms: bytes_per_day / DAY_MS as f64,
        }
    }

    #[test]
    fn projects_when_a_disk_fills() {
        let disk = make_growth(880, 20.0);
        assert_eq!(project_full_at(&disk), Some(at(9, 0).timestamp_millis()));

        assert_eq!(project_full_at(&make_growth(880, 0.0)), None);
        assert_eq!(project_full_at(&make_growth(1000, 20.0)), Some(disk.taken_at));
    }

    #[test]
    fn words_the_days_left() {
        assert_eq!(fills_in(DAY_MS / 2), "within a day");
        assert_eq!(fills_in(DAY_MS + DAY_MS / 3), "in about a day");
        assert_eq!(fills_in(5 * DAY_MS + DAY_MS * 2 / 3), "in about 6 days");

        assert_eq!(expires_in(DAY_MS / 2), "within a day");
        assert_eq!(expires_in(DAY_MS + 1), "in 1 day");
        assert_eq!(expires_in(9 * DAY_MS + 1), "in 9 days");
    }

    #[test]
    fn lists_disks_that_fill_within_the_window() {
        let servers = [make_server("srv_1", "clickhouse-1")];
        let now = at(3, 0).timestamp_millis();
        let growth = [
            make_growth(880, 20.0),
            make_growth(100, 1.0),
            DiskGrowth {
                filesystem: "tmpfs".to_string(),
                ..make_growth(880, 20.0)
            },
            DiskGrowth {
                server_id: "srv_gone".to_string(),
                ..make_growth(880, 20.0)
            },
            DiskGrowth {
                taken_at: at(1, 0).timestamp_millis(),
                ..make_growth(880, 20.0)
            },
        ];

        let items = list_full_disks(&servers, &growth, now);

        assert_eq!(
            items,
            [UpcomingItem {
                kind: UpcomingKind::DiskFull,
                at: Some(at(9, 0).timestamp_millis()),
                soon: true,
                title: "clickhouse-1 disk fills in about 6 days".to_string(),
                detail: "/var/lib/clickhouse is at 88% and grew 20 B a day this week."
                    .to_string(),
                target_id: Some("srv_1".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn fits_disk_growth_from_a_day_of_samples() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-upcoming-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();

        analytics
            .run(|conn| {
                conn.execute_batch(MIGRATION_DISKS)?;

                let disk = "INSERT INTO disk_states VALUES (?, ?, ?, 'ext4', 1000, ?)";
                for (hour, used) in [(0, 800), (12, 810), (24, 820)] {
                    let taken_at = at(2, 0).timestamp_millis() + hour * 3_600_000;
                    conn.execute(disk, duckdb::params!["srv_1", taken_at, "/", used])?;
                    conn.execute(disk, duckdb::params!["srv_1", taken_at, "/flat", 500])?;
                }

                for (hour, used) in [(0, 100), (6, 200)] {
                    let taken_at = at(3, 0).timestamp_millis() + hour * 3_600_000;
                    conn.execute(disk, duckdb::params!["srv_2", taken_at, "/", used])?;
                }

                Ok(())
            })
            .unwrap();

        let growth = list_disk_growth(&analytics, at(1, 0).timestamp_millis(), DAY_MS)
            .await
            .unwrap();

        assert_eq!(growth.len(), 1);
        assert_eq!(growth[0].mount, "/");
        assert_eq!(growth[0].used, 820);
        assert_eq!(growth[0].taken_at, at(3, 0).timestamp_millis());
        assert!((growth[0].bytes_per_ms * DAY_MS as f64 - 20.0).abs() < 1e-6);
    }

    #[test]
    fn compares_versions_as_semver() {
        let parse = |text: &str| text.parse::<Version>().unwrap();

        assert!(parse("0.0.9") < parse("0.0.13"));
        assert!(parse("0.2.0") > parse("0.1.99"));
        assert!(parse("1.0.0-rc.1") < parse("1.0.0"));
        assert_eq!(parse("v0.0.13"), parse("0.0.13+linux"));
        assert!("0.13".parse::<Version>().is_err());
        assert!("dev".parse::<Version>().is_err());
    }

    #[test]
    fn groups_agents_behind_the_latest_version() {
        let agents = [
            ("api-1", "0.0.13".to_string()),
            ("smtp-1", "0.0.11".to_string()),
            ("staging-1", "0.0.11".to_string()),
        ];
        let item = find_outdated_agents(&agents, "0.0.13").unwrap();
        assert_eq!(item.title, "2 agents are behind 0.0.13");
        assert_eq!(item.detail, "smtp-1 and staging-1 run 0.0.11.");
        assert_eq!((item.at, item.soon, item.target_id), (None, false, None));

        let one = find_outdated_agents(&agents[..2], "0.0.13").unwrap();
        assert_eq!(one.title, "1 agent is behind 0.0.13");
        assert_eq!(one.detail, "smtp-1 runs 0.0.11.");

        let many = [
            ("a", "0.0.9".to_string()),
            ("b", "0.0.12".to_string()),
            ("c", "0.0.9".to_string()),
            ("d", "0.0.10".to_string()),
            ("e", "0.0.12".to_string()),
        ];
        let item = find_outdated_agents(&many, "0.0.13").unwrap();
        assert_eq!(item.title, "5 agents are behind 0.0.13");
        assert_eq!(item.detail, "a, b, c and 2 more run 0.0.9, 0.0.10 and 0.0.12.");

        assert_eq!(find_outdated_agents(&agents[..1], "0.0.13"), None);
    }
}
