use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use log::warn;
use serde::Deserialize;

const KEEP_FOR: Duration = Duration::from_secs(600);
const RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerAction {
    pub action: String,
    pub exit_code: Option<i32>,
    at: Instant,
}

#[derive(Clone, Default)]
pub struct DockerEvents {
    actions: Arc<Mutex<HashMap<String, Vec<DockerAction>>>>,
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

    pub fn list_actions(&self, container_id: &str) -> Vec<DockerAction> {
        lock(&self.actions).get(container_id).cloned().unwrap_or_default()
    }

    fn follow(&self) {
        let mut use_sudo = false;
        loop {
            let (program, prefix): (&str, &[&str]) =
                if use_sudo { ("sudo", &["-n", "docker"]) } else { ("docker", &[]) };
            let child = Command::new(program)
                .args(prefix)
                .args(["events", "--filter", "type=container", "--format", "{{json .}}"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            match child {
                Ok(mut child) => {
                    if let Some(stdout) = child.stdout.take() {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            self.record(&line, Instant::now());
                        }
                    }
                    let _ = child.wait();
                }
                Err(error) => warn!("follow docker events: {error}"),
            }
            use_sudo = !use_sudo;
            thread::sleep(RETRY_AFTER);
        }
    }

    fn record(&self, line: &str, at: Instant) {
        let Ok(event) = serde_json::from_str::<RawEvent>(line) else { return };
        let action = event.action.split(':').next().unwrap_or_default().to_string();
        let exit_code = event
            .actor
            .attributes
            .get("exitCode")
            .and_then(|code| code.parse().ok());
        let mut actions = lock(&self.actions);
        actions.retain(|_, list| {
            list.retain(|item| at.duration_since(item.at) < KEEP_FOR);
            !list.is_empty()
        });
        actions.entry(event.actor.id).or_default().push(DockerAction {
            action,
            exit_code,
            at,
        });
    }
}

pub fn was_stopped(actions: &[DockerAction]) -> bool {
    let last_die = actions.iter().rposition(|item| item.action == "die");
    let before = &actions[..last_die.unwrap_or(actions.len())];
    before
        .iter()
        .any(|item| item.action == "stop" || item.action == "kill")
        && !before.iter().any(|item| item.action == "oom")
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
        let actions = stopped.list_actions("c1");
        assert!(was_stopped(&actions));
        assert!(!was_oom(&actions));
        assert_eq!(read_exit_code(&actions), Some(143));

        let crashed = DockerEvents::default();
        record(&crashed, "die", Some("1"));
        assert!(!was_stopped(&crashed.list_actions("c1")));

        let oom = DockerEvents::default();
        record(&oom, "oom", None);
        record(&oom, "kill", None);
        record(&oom, "die", Some("137"));
        let actions = oom.list_actions("c1");
        assert!(was_oom(&actions));
        assert!(!was_stopped(&actions));
    }

    #[test]
    fn keeps_the_action_name_before_a_colon() {
        let events = DockerEvents::default();
        record(&events, "exec_start: sh", None);
        assert_eq!(events.list_actions("c1")[0].action, "exec_start");
        events.record("not json", Instant::now());
        assert_eq!(events.list_actions("c1").len(), 1);
    }
}
