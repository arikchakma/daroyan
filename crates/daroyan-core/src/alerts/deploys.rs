use std::collections::HashMap;

use chrono::{DateTime, Utc};
use daroyan_protocol::report::{Container, ProcessState};
use serde_json::json;

use crate::alerts::message::format_span;
use crate::alerts::notify::{Details, Link, Notice, Snippet, Text, Tone};

pub const DEPLOY_WINDOW_SECS: i64 = 60;
const QUIET_SECS: i64 = 20;

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
}

#[derive(Default)]
pub struct DeployWatch {
    tracked: HashMap<(String, String), Tracked>,
    pending: HashMap<String, Vec<Replacement>>,
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
                    if let Some(tracked) = self.tracked.get(&key)
                        && tracked.container.id != container.id
                    {
                        let gap =
                            tracked.stopped_at.map_or(0, |at| (now - at).num_seconds());
                        if gap <= DEPLOY_WINDOW_SECS {
                            self.pending.entry(server_id.to_string()).or_default().push(
                                Replacement {
                                    name: process.name.clone(),
                                    old: tracked.container.clone(),
                                    new: container.clone(),
                                    gap_secs: gap.unsigned_abs(),
                                    at: now,
                                },
                            );
                        }
                    }
                    self.tracked.insert(
                        key,
                        Tracked { container: container.clone(), stopped_at: None },
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

    pub fn is_container(&self, server_id: &str, name: &str) -> bool {
        self.tracked.contains_key(&(server_id.to_string(), name.to_string()))
    }

    pub fn take_ready(&mut self, now: DateTime<Utc>) -> Vec<(String, Vec<Replacement>)> {
        let ready: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, list)| {
                list.iter().all(|item| (now - item.at).num_seconds() >= QUIET_SECS)
            })
            .map(|(server_id, _)| server_id.clone())
            .collect();
        ready
            .into_iter()
            .filter_map(|server_id| {
                let list = self.pending.remove(&server_id)?;
                Some((server_id, list))
            })
            .collect()
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
        assert!(watch.is_container("srv_1", "Inboxjot API"));
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
