use std::collections::HashMap;

use chrono::{DateTime, Utc};
use daroyan_protocol::report::{Container, ProcessState};
use serde_json::json;

use crate::alerts::message::format_span;
use crate::alerts::notify::{Details, Link, Notice, Snippet, Text, Tone};

pub const DEPLOY_WINDOW_SECS: i64 = 60;
const QUIET_SECS: i64 = 20;
const MAX_WAIT_SECS: i64 = 300;
const RESTART_MEMORY_SECS: i64 = 900;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub name: String,
    pub old: Container,
    pub new: Container,
    pub gap_secs: u64,
    pub at: DateTime<Utc>,
}

struct Tracked {
    container: Container,
    stopped_at: Option<DateTime<Utc>>,
    restarted_at: Option<DateTime<Utc>>,
}

struct Batch {
    since: DateTime<Utc>,
    replacements: Vec<Replacement>,
}

#[derive(Default)]
pub struct DeployWatch {
    tracked: HashMap<(String, String), Tracked>,
    pending: HashMap<String, Batch>,
}

impl DeployWatch {
    pub fn observe(
        &mut self,
        server_id: &str,
        processes: &[ProcessState],
        now: DateTime<Utc>,
    ) {
        for process in processes {
            let key = (server_id.to_string(), process.name.clone());
            match &process.container {
                Some(container) => {
                    let tracked = self.tracked.get(&key);
                    let mut restarted_at =
                        tracked.and_then(|tracked| tracked.restarted_at);
                    if let Some(tracked) = tracked {
                        if tracked.container.id == container.id {
                            if tracked.stopped_at.is_some() {
                                restarted_at = Some(now);
                            }
                        } else {
                            let gap = tracked
                                .stopped_at
                                .map_or(0, |at| (now - at).num_seconds());
                            if gap <= DEPLOY_WINDOW_SECS {
                                queue(
                                    &mut self
                                        .pending
                                        .entry(server_id.to_string())
                                        .or_insert_with(|| Batch {
                                            since: now,
                                            replacements: Vec::new(),
                                        })
                                        .replacements,
                                    &process.name,
                                    &tracked.container,
                                    container,
                                    gap,
                                    now,
                                );
                            }
                        }
                    }
                    self.tracked.insert(
                        key,
                        Tracked {
                            container: container.clone(),
                            stopped_at: None,
                            restarted_at,
                        },
                    );
                }
                None if process.count == 0 => {
                    if let Some(tracked) = self.tracked.get_mut(&key) {
                        tracked.stopped_at.get_or_insert(now);
                    }
                }
                None => {}
            }
        }
    }

    pub fn may_be_replaced(
        &self,
        server_id: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> bool {
        self.tracked
            .get(&(server_id.to_string(), name.to_string()))
            .is_some_and(|tracked| {
                tracked
                    .restarted_at
                    .is_none_or(|at| (now - at).num_seconds() >= RESTART_MEMORY_SECS)
            })
    }

    pub fn take_ready(&mut self, now: DateTime<Utc>) -> Vec<(String, Vec<Replacement>)> {
        let ready: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, batch)| {
                (now - batch.since).num_seconds() >= MAX_WAIT_SECS
                    || batch
                        .replacements
                        .iter()
                        .all(|item| (now - item.at).num_seconds() >= QUIET_SECS)
            })
            .map(|(server_id, _)| server_id.clone())
            .collect();
        ready
            .into_iter()
            .filter_map(|server_id| {
                let list: Vec<Replacement> = self
                    .pending
                    .remove(&server_id)?
                    .replacements
                    .into_iter()
                    .filter(|item| self.is_up(&server_id, item))
                    .collect();
                (!list.is_empty()).then_some((server_id, list))
            })
            .collect()
    }

    fn is_up(&self, server_id: &str, replacement: &Replacement) -> bool {
        self.tracked
            .get(&(server_id.to_string(), replacement.name.clone()))
            .is_some_and(|tracked| {
                tracked.container.id == replacement.new.id && tracked.stopped_at.is_none()
            })
    }
}

pub struct DeployNote<'a> {
    pub server_id: &'a str,
    pub server_name: &'a str,
    pub labels: &'a [String],
    pub replacements: &'a [Replacement],
    pub app_url: Option<&'a str>,
}

impl DeployNote<'_> {
    pub fn notice(&self) -> Notice {
        let names: Vec<&str> =
            self.replacements.iter().map(|item| item.name.as_str()).collect();
        let longest =
            self.replacements.iter().map(|item| item.gap_secs).max().unwrap_or(0);
        let count = self.replacements.len();
        let (noun, verb, running) = match count {
            1 => ("The container", "was", "It is running.".to_string()),
            2 => ("The containers", "were", "Both are running.".to_string()),
            _ => ("The containers", "were", format!("All {count} are running.")),
        };
        let summary = if longest == 0 {
            Text::from(format!("{noun} {verb} replaced. {running}"))
        } else {
            Text::from(format!("{noun} stopped and started again within "))
                .bold(format_span(longest))
                .plain(format!(". {running}"))
        };
        let rows: Vec<[String; 3]> = self
            .replacements
            .iter()
            .map(|item| {
                let containers = if item.old.name == item.new.name {
                    String::new()
                } else {
                    format!("{} → {}", item.old.name, item.new.name)
                };
                [
                    item.name.clone(),
                    containers,
                    describe_versions(&item.old.image, &item.new.image),
                ]
            })
            .collect();
        let widths: Vec<usize> = (0..3)
            .map(|column| {
                rows.iter().map(|row| row[column].chars().count()).max().unwrap_or(0)
            })
            .collect();
        let lines = rows
            .iter()
            .map(|row| {
                let cells: Vec<String> = row
                    .iter()
                    .zip(&widths)
                    .filter(|(_, width)| **width > 0)
                    .map(|(cell, width)| format!("{cell:<width$}"))
                    .collect();
                format!("{}  up", cells.join("  "))
            })
            .collect();
        let context = std::iter::once(self.server_name)
            .chain(self.labels.iter().map(String::as_str))
            .chain(std::iter::once("No alert was opened."))
            .collect::<Vec<_>>()
            .join(" · ");
        Notice {
            event: "deploy",
            tone: Tone::Note,
            title: format!("🚀 Deployed: {}", join_names(&names)),
            body: format!("{} · {} replaced", self.server_name, join_names(&names)),
            url: self.server_url(),
            data: json!({
                "server": { "id": self.server_id, "name": self.server_name },
                "replacements": self.replacements.iter().map(|item| json!({
                    "name": item.name,
                    "old": { "container": item.old.name, "image": item.old.image },
                    "new": { "container": item.new.name, "image": item.new.image },
                    "gap_secs": item.gap_secs,
                })).collect::<Vec<_>>(),
            }),
            details: Details {
                summary,
                snippet: Some(Snippet { label: None, lines }),
                links: self
                    .server_url()
                    .map(|url| Link::new("Open server", url).primary())
                    .into_iter()
                    .collect(),
                context: Some(context),
                ..Details::default()
            },
        }
    }

    fn server_url(&self) -> Option<String> {
        self.app_url.map(|url| {
            format!("{}/servers/{}", url.trim_end_matches('/'), self.server_id)
        })
    }
}

fn queue(
    list: &mut Vec<Replacement>,
    name: &str,
    old: &Container,
    new: &Container,
    gap: i64,
    now: DateTime<Utc>,
) {
    let earlier = list
        .iter()
        .position(|item| item.name == name)
        .map(|index| list.remove(index));
    list.push(Replacement {
        name: name.to_string(),
        old: earlier.map_or_else(|| old.clone(), |item| item.old),
        new: new.clone(),
        gap_secs: gap.unsigned_abs(),
        at: now,
    });
}

fn describe_versions(old: &str, new: &str) -> String {
    let split = |image: &str| -> (String, String) {
        let name_end = image.rfind('/').map_or(0, |index| index + 1);
        match image[name_end..].rfind(':') {
            Some(index) => (
                image[..name_end + index].to_string(),
                image[name_end + index + 1..].to_string(),
            ),
            None => (image.to_string(), "latest".to_string()),
        }
    };
    let ((old_repo, old_tag), (new_repo, new_tag)) = (split(old), split(new));
    match (old_repo == new_repo, old_tag == new_tag) {
        (true, true) => format!("{new_tag} (recreated)"),
        (true, false) => format!("{old_tag} → {new_tag}"),
        (false, _) => format!("{old} → {new}"),
    }
}

fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use daroyan_protocol::report::ProcessStatus;

    use super::*;

    fn at(secs: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 8, 0, secs).unwrap()
    }

    fn container(id: &str, name: &str, image: &str) -> Container {
        Container {
            id: id.to_string(),
            name: name.to_string(),
            image: image.to_string(),
            project: Some("inboxjot".to_string()),
            service: Some("api".to_string()),
        }
    }

    fn process(name: &str, container: Option<Container>) -> ProcessState {
        ProcessState {
            name: name.to_string(),
            status: if container.is_some() {
                ProcessStatus::Running
            } else {
                ProcessStatus::Stopped
            },
            count: u32::from(container.is_some()),
            started_at: None,
            cpu: 0.0,
            mem_used: 0,
            read_speed: 0,
            write_speed: 0,
            controllable: false,
            container,
        }
    }

    #[test]
    fn finds_a_replacement_within_the_window() {
        let mut watch = DeployWatch::default();
        let blue = container("c1", "inboxjot-api-blue", "ghcr.io/inboxjot/api:0.0.18");
        let green = container("c2", "inboxjot-api-green", "ghcr.io/inboxjot/api:0.0.19");
        watch.observe("srv_1", &[process("Inboxjot API", Some(blue.clone()))], at(0));
        assert!(watch.may_be_replaced("srv_1", "Inboxjot API", at(0)));
        watch.observe("srv_1", &[process("Inboxjot API", None)], at(10));
        watch.observe("srv_1", &[process("Inboxjot API", Some(green.clone()))], at(24));
        assert!(watch.take_ready(at(30)).is_empty());
        let ready = watch.take_ready(at(45));
        assert_eq!(
            ready,
            vec![(
                "srv_1".to_string(),
                vec![Replacement {
                    name: "Inboxjot API".to_string(),
                    old: blue,
                    new: green,
                    gap_secs: 14,
                    at: at(24),
                }],
            )]
        );
        assert!(watch.take_ready(at(59)).is_empty());
    }

    #[test]
    fn ignores_a_container_that_stayed_down() {
        let mut watch = DeployWatch::default();
        watch.observe(
            "srv_1",
            &[process("API", Some(container("c1", "api-1", "api:1")))],
            at(0),
        );
        watch.observe("srv_1", &[process("API", None)], at(1));
        let later = at(1) + chrono::Duration::seconds(DEPLOY_WINDOW_SECS + 30);
        watch.observe(
            "srv_1",
            &[process("API", Some(container("c2", "api-1", "api:2")))],
            later,
        );
        assert!(watch.take_ready(later + chrono::Duration::seconds(60)).is_empty());
    }

    #[test]
    fn stops_holding_a_container_that_restarts_in_place() {
        let mut watch = DeployWatch::default();
        let api = container("c1", "api-1", "api:1");
        watch.observe("srv_1", &[process("API", Some(api.clone()))], at(0));
        assert!(watch.may_be_replaced("srv_1", "API", at(0)));
        watch.observe("srv_1", &[process("API", None)], at(10));
        watch.observe("srv_1", &[process("API", Some(api))], at(20));
        assert!(!watch.may_be_replaced("srv_1", "API", at(30)));
        let later = at(20) + chrono::Duration::seconds(RESTART_MEMORY_SECS);
        assert!(watch.may_be_replaced("srv_1", "API", later));
        assert!(watch.take_ready(at(59)).is_empty());
    }

    #[test]
    fn keeps_one_replacement_per_name_and_skips_a_crashed_one() {
        let mut watch = DeployWatch::default();
        let ids = ["c1", "c2", "c3", "c4"];
        for (index, id) in ids.iter().enumerate() {
            let secs = u32::try_from(index).unwrap() * 10;
            watch.observe(
                "srv_1",
                &[process("API", Some(container(id, "api-1", "api:1")))],
                at(secs),
            );
        }
        let ready = watch.take_ready(at(55));
        assert_eq!(ready[0].1.len(), 1);
        assert_eq!(ready[0].1[0].old.id, "c1");
        assert_eq!(ready[0].1[0].new.id, "c4");

        watch.observe(
            "srv_1",
            &[process("API", Some(container("c5", "api-1", "api:2")))],
            at(56),
        );
        watch.observe("srv_1", &[process("API", None)], at(58));
        assert!(watch.take_ready(at(56) + chrono::Duration::seconds(24)).is_empty());
    }

    #[test]
    fn sends_a_busy_server_after_the_longest_wait() {
        let mut watch = DeployWatch::default();
        for step in 0..=(MAX_WAIT_SECS / 10 + 1) {
            let now = at(0) + chrono::Duration::seconds(step * 10);
            watch.observe(
                "srv_1",
                &[process("API", Some(container(&format!("c{step}"), "api-1", "api:1")))],
                now,
            );
            let ready = watch.take_ready(now);
            assert_eq!(ready.is_empty(), step * 10 < MAX_WAIT_SECS + 10, "{step}");
        }
    }

    #[test]
    fn builds_a_deploy_note() {
        let replacement =
            |name: &str, old: Container, new: Container, gap_secs| Replacement {
                name: name.to_string(),
                old,
                new,
                gap_secs,
                at: at(0),
            };
        let replacements = vec![
            replacement(
                "Inboxjot API",
                container("c1", "inboxjot-api-blue", "ghcr.io/inboxjot/api:0.0.18"),
                container("c2", "inboxjot-api-green", "ghcr.io/inboxjot/api:0.0.19"),
                14,
            ),
            replacement(
                "Inboxjot Worker",
                container("c3", "inboxjot-worker-1", "ghcr.io/inboxjot/worker:0.0.18"),
                container("c4", "inboxjot-worker-1", "ghcr.io/inboxjot/worker:0.0.19"),
                3,
            ),
            replacement(
                "Inboxjot SMTP",
                container("c5", "inboxjot-smtp-1", "localhost:5000/smtp"),
                container("c6", "inboxjot-smtp-1", "localhost:5000/smtp"),
                0,
            ),
        ];
        let labels = vec!["production".to_string()];
        let note = DeployNote {
            server_id: "srv_1",
            server_name: "Inboxjot Server",
            labels: &labels,
            replacements: &replacements,
            app_url: Some("https://daroyan.test"),
        };
        let notice = note.notice();
        assert_eq!(
            notice.title,
            "🚀 Deployed: Inboxjot API, Inboxjot Worker and Inboxjot SMTP"
        );
        assert_eq!(notice.tone, Tone::Note);
        assert_eq!(
            notice.details.summary.to_string(),
            "The containers stopped and started again within 14 s. All 3 are running."
        );
        assert_eq!(
            notice.details.snippet.unwrap().lines,
            [
                "Inboxjot API     inboxjot-api-blue → inboxjot-api-green  0.0.18 → 0.0.19     up",
                "Inboxjot Worker                                          0.0.18 → 0.0.19     up",
                "Inboxjot SMTP                                            latest (recreated)  up",
            ]
        );
        assert_eq!(
            notice.details.context.as_deref(),
            Some("Inboxjot Server · production · No alert was opened.")
        );
        assert_eq!(notice.details.links[0].url, "https://daroyan.test/servers/srv_1");

        let single = DeployNote { replacements: &replacements[2..], ..note }.notice();
        assert_eq!(
            single.details.summary.to_string(),
            "The container was replaced. It is running."
        );
    }
}
