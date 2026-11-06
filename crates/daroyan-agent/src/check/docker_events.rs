use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use log::warn;
use serde::Deserialize;

use crate::docker_read::{self, DockerRead, EVENT_ACTIONS};
use crate::service::is_permission_error;

const KEEP_FOR: Duration = Duration::from_secs(600);
const ERROR_BYTES: u64 = 64 * 1024;
const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerAction {
    pub action: String,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    at: Instant,
}

#[derive(Clone, Default)]
pub struct DockerEvents {
    actions: Arc<Mutex<HashMap<String, Vec<DockerAction>>>>,
    changes: Arc<AtomicU64>,
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Actor")]
    actor: RawActor,
}

#[derive(Deserialize)]
struct RawActor {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Attributes", default)]
    attributes: HashMap<String, String>,
}

impl DockerEvents {
    pub fn start() -> Self {
        let events = Self::default();
        let watched = events.clone();
        thread::spawn(move || watched.follow());

        events
    }

    pub fn changes(&self) -> u64 {
        self.changes.load(Ordering::Acquire)
    }

    pub fn list_last_run(&self, container_id: &str) -> Vec<DockerAction> {
        let actions = lock(&self.actions);
        let Some(list) = actions.get(container_id) else { return Vec::new() };

        let start = list.iter().rposition(|item| item.action == "start");
        list[start.map_or(0, |index| index + 1)..].to_vec()
    }

    fn follow(&self) {
        loop {
            let use_sudo = docker_read::uses_sudo();
            let mut stream = docker_read::open(&DockerRead::Events, use_sudo);
            self.changes.fetch_add(1, Ordering::AcqRel);
            if let Some(stdout) = stream.take_stdout() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    self.record(&line, Instant::now());
                }
            }

            let mut error = String::new();
            if let Some(stderr) = stream.take_stderr() {
                let _ = stderr.take(ERROR_BYTES).read_to_string(&mut error);
            }
            let _ = stream.handle().wait();

            if !use_sudo && is_permission_error(&error) {
                docker_read::require_sudo();
                continue;
            }
            if !error.trim().is_empty() {
                warn!("follow docker events: {}", error.trim());
            }

            thread::sleep(RETRY_AFTER);
        }
    }

    fn record(&self, line: &str, at: Instant) {
        let Ok(event) = serde_json::from_str::<RawEvent>(line) else { return };

        let action = event.action.split(':').next().unwrap_or_default().to_string();
        if !EVENT_ACTIONS.contains(&action.as_str()) {
            return;
        }

        let exit_code = event
            .actor
            .attributes
            .get("exitCode")
            .and_then(|code| code.parse().ok());
        let signal = event
            .actor
            .attributes
            .get("signal")
            .filter(|signal| !signal.is_empty())
            .cloned();
        if matches!(action.as_str(), "start" | "die") {
            self.changes.fetch_add(1, Ordering::AcqRel);
        }

        let mut actions = lock(&self.actions);

        actions.retain(|_, list| {
            list.last().is_some_and(|item| at.duration_since(item.at) < KEEP_FOR)
        });

        let list = actions.entry(event.actor.id).or_default();
        list.retain(|item| at.duration_since(item.at) < KEEP_FOR);
        list.push(DockerAction { action, exit_code, signal, at });
    }
}

pub fn was_stopped(actions: &[DockerAction]) -> bool {
    let last_die = actions.iter().rposition(|item| item.action == "die");
    let before = &actions[..last_die.unwrap_or(actions.len())];

    before.iter().any(|item| {
        item.action == "stop" || (item.action == "kill" && is_ending_signal(item))
    }) && !before.iter().any(|item| item.action == "oom")
}

fn is_ending_signal(action: &DockerAction) -> bool {
    action.signal.as_deref().is_none_or(|signal| {
        matches!(signal, "9" | "15" | "KILL" | "TERM" | "SIGKILL" | "SIGTERM")
    })
}

pub fn was_oom(actions: &[DockerAction]) -> bool {
    actions.iter().any(|item| item.action == "oom")
}

pub fn read_exit_code(actions: &[DockerAction]) -> Option<i32> {
    actions
        .iter()
        .rev()
        .find(|item| item.action == "die")
        .and_then(|item| item.exit_code)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(events: &DockerEvents, action: &str, exit_code: Option<&str>) {
        let attributes =
            exit_code.map_or_else(String::new, |code| format!(r#","exitCode":"{code}""#));
        record_with(events, action, &attributes);
    }

    fn record_with(events: &DockerEvents, action: &str, attributes: &str) {
        events.record(
            &format!(r#"{{"Type":"container","Action":"{action}","Actor":{{"ID":"c1","Attributes":{{"name":"api"{attributes}}}}}}}"#),
            Instant::now(),
        );
    }

    #[test]
    fn tells_a_stop_from_a_crash() {
        let stopped = DockerEvents::default();
        record(&stopped, "kill", None);
        record(&stopped, "die", Some("143"));
        record(&stopped, "stop", None);

        let actions = stopped.list_last_run("c1");
        assert!(was_stopped(&actions));
        assert!(!was_oom(&actions));
        assert_eq!(read_exit_code(&actions), Some(143));

        let crashed = DockerEvents::default();
        record(&crashed, "die", Some("1"));
        assert!(!was_stopped(&crashed.list_last_run("c1")));

        let oom = DockerEvents::default();
        record(&oom, "oom", None);
        record(&oom, "kill", None);
        record(&oom, "die", Some("137"));

        let actions = oom.list_last_run("c1");
        assert!(was_oom(&actions));
        assert!(!was_stopped(&actions));
    }

    #[test]
    fn forgets_actions_from_an_earlier_run() {
        let restarted = DockerEvents::default();
        record(&restarted, "kill", None);
        record(&restarted, "die", Some("143"));
        record(&restarted, "stop", None);
        record(&restarted, "start", None);
        record(&restarted, "restart", None);
        record(&restarted, "die", Some("1"));

        let actions = restarted.list_last_run("c1");
        assert!(!was_stopped(&actions));
        assert_eq!(read_exit_code(&actions), Some(1));

        let recovered = DockerEvents::default();
        record(&recovered, "oom", None);
        record(&recovered, "die", Some("137"));
        record(&recovered, "start", None);
        record(&recovered, "die", Some("1"));
        assert!(!was_oom(&recovered.list_last_run("c1")));
    }

    #[test]
    fn keeps_only_lifecycle_actions() {
        let events = DockerEvents::default();
        record(&events, "exec_start: sh", None);
        record(&events, "health_status: healthy", None);
        events.record("not json", Instant::now());

        assert!(events.list_last_run("c1").is_empty());

        record(&events, "die", Some("1"));
        assert_eq!(events.list_last_run("c1")[0].action, "die");
    }

    #[test]
    fn counts_starts_and_deaths_as_changes() {
        let events = DockerEvents::default();
        record(&events, "kill", None);
        record(&events, "stop", None);
        assert_eq!(events.changes(), 0);

        record(&events, "die", Some("0"));
        record(&events, "start", None);
        assert_eq!(events.changes(), 2);
    }

    #[test]
    fn treats_a_reload_signal_as_a_crash() {
        let reloaded = DockerEvents::default();
        record_with(&reloaded, "kill", r#","signal":"1""#);
        record(&reloaded, "die", Some("1"));
        assert!(!was_stopped(&reloaded.list_last_run("c1")));

        let killed = DockerEvents::default();
        record_with(&killed, "kill", r#","signal":"9""#);
        record(&killed, "die", Some("137"));
        assert!(was_stopped(&killed.list_last_run("c1")));
    }

    #[test]
    fn counts_a_stop_with_the_image_stop_signal() {
        for signal in ["3", "2"] {
            let stopped = DockerEvents::default();
            record_with(&stopped, "kill", &format!(r#","signal":"{signal}""#));
            record(&stopped, "stop", None);
            record(&stopped, "die", Some("0"));

            assert!(was_stopped(&stopped.list_last_run("c1")), "{signal}");
        }

        let forced = DockerEvents::default();
        record_with(&forced, "kill", r#","signal":"3""#);
        record_with(&forced, "kill", r#","signal":"9""#);
        record(&forced, "die", Some("137"));
        record(&forced, "stop", None);
        assert!(was_stopped(&forced.list_last_run("c1")));
    }
}
