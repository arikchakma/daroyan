use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use chrono::DateTime;
use daroyan_protocol::report::Container;

use serde::Deserialize;

use crate::config::{DockerTarget, ProcessCheck, Source};
use crate::docker_read::{self, DockerRead, ListTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Controller {
    Systemd(String),
    Docker { target: DockerTarget, min_running: usize },
    None,
}

impl Controller {
    pub fn is_controllable(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn run(&self, verb: &str) -> Result<()> {
        match self {
            Self::Systemd(unit) => {
                run_with_sudo_fallback("systemctl", &[verb, unit]).map(drop)
            }
            Self::Docker { target, min_running } => {
                let containers = target.pick_for(verb, *min_running);
                if containers.is_empty() {
                    bail!("no container matches {target}");
                }

                run_on_each(verb, &containers)
            }
            Self::None => bail!(
                "it is watch-only, so Daroyan cannot {verb} it. Watch it with systemd or docker to control it"
            ),
        }
    }

    pub fn find_missing(&self) -> Vec<String> {
        match self {
            Self::Docker { target, min_running } => target.pick_missing(*min_running),
            _ => Vec::new(),
        }
    }

    pub fn read_exit(&self) -> ExitInfo {
        match self {
            Self::Systemd(unit) => Command::new("systemctl")
                .args([
                    "show",
                    unit,
                    "--property=Result,ExecMainCode,ExecMainStatus,NRestarts,ExecMainStartTimestampMonotonic,ExecMainExitTimestampMonotonic",
                ])
                .output()
                .map(|output| parse_systemd_exit(&String::from_utf8_lossy(&output.stdout)))
                .unwrap_or_default(),
            Self::Docker { target, .. } => target
                .pick_newest(1)
                .first()
                .and_then(|container| inspect_containers(std::slice::from_ref(container)).pop())
                .map(|facts| facts.read_exit())
                .unwrap_or_default(),
            Self::None => ExitInfo::default(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub signal: Option<String>,
    pub oom: bool,
    pub ran_secs: Option<u64>,
    pub restarts: Option<u32>,
}

fn parse_systemd_exit(output: &str) -> ExitInfo {
    let properties: HashMap<&str, &str> =
        output.lines().filter_map(|line| line.split_once('=')).collect();
    let read = |key: &str| properties.get(key)?.parse::<i64>().ok();
    let status = read("ExecMainStatus").and_then(|status| i32::try_from(status).ok());
    let is_signal = matches!(properties.get("ExecMainCode"), Some(&"2" | &"3"));
    let ran_secs = read("ExecMainStartTimestampMonotonic")
        .zip(read("ExecMainExitTimestampMonotonic"))
        .filter(|(started, exited)| *started > 0 && exited >= started)
        .and_then(|(started, exited)| u64::try_from((exited - started) / 1_000_000).ok());

    ExitInfo {
        code: status.filter(|_| !is_signal),
        signal: status.filter(|_| is_signal).and_then(read_signal_name),
        oom: properties.get("Result") == Some(&"oom-kill"),
        ran_secs,
        restarts: read("NRestarts").and_then(|count| u32::try_from(count).ok()),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ContainerFacts {
    id: String,
    name: String,
    image: String,
    image_id: Option<String>,
    running: bool,
    started_at: String,
    finished_at: String,
    exit_code: Option<i32>,
    oom_killed: bool,
    restart_count: Option<u32>,
    project: Option<String>,
    service: Option<String>,
    config_hash: Option<String>,
}

impl ContainerFacts {
    fn short_name(&self) -> &str {
        self.name.trim_start_matches('/')
    }

    fn started(&self) -> Option<DateTime<chrono::FixedOffset>> {
        DateTime::parse_from_rfc3339(&self.started_at).ok()
    }

    fn read_exit(&self) -> ExitInfo {
        let finished = DateTime::parse_from_rfc3339(&self.finished_at).ok();
        let ran_secs = self.started().zip(finished).and_then(|(started, finished)| {
            u64::try_from((finished - started).num_seconds()).ok()
        });

        ExitInfo {
            code: self.exit_code,
            signal: self
                .exit_code
                .filter(|code| *code > 128)
                .and_then(|code| read_signal_name(code - 128)),
            oom: self.oom_killed,
            ran_secs,
            restarts: self.restart_count,
        }
    }
}

impl From<ContainerFacts> for Container {
    fn from(facts: ContainerFacts) -> Self {
        Self {
            name: facts.short_name().to_string(),
            id: facts.id,
            image: facts.image,
            project: facts.project.filter(|project| !project.is_empty()),
            service: facts.service.filter(|service| !service.is_empty()),
            image_id: facts.image_id,
            config_hash: facts.config_hash.filter(|hash| !hash.is_empty()),
        }
    }
}

fn parse_facts(output: &str) -> Vec<ContainerFacts> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn inspect_containers(containers: &[String]) -> Vec<ContainerFacts> {
    if containers.is_empty() {
        return Vec::new();
    }

    docker_read::run(&DockerRead::Inspect(containers.to_vec()))
        .map(|output| parse_facts(&output))
        .unwrap_or_default()
}

pub struct Listed {
    pub id: String,
    pub name: String,
    pub running: bool,
}

fn parse_listing(output: &str) -> Vec<Listed> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(' ');
            Some(Listed {
                id: parts.next()?.to_string(),
                name: parts.next()?.to_string(),
                running: parts.next()? == "running",
            })
        })
        .collect()
}

fn read_signal_name(number: i32) -> Option<String> {
    let name = match number {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        _ => return None,
    };

    Some(name.to_string())
}

pub fn restart_containers(containers: &[String]) -> Result<()> {
    run_on_each("restart", containers)
}

fn run_on_each(verb: &str, containers: &[String]) -> Result<()> {
    let results: Vec<Result<String>> = thread::scope(|scope| {
        let runs: Vec<_> = containers
            .iter()
            .map(|container| {
                scope.spawn(move || run_with_sudo_fallback("docker", &[verb, container]))
            })
            .collect();

        runs.into_iter()
            .map(|run| {
                run.join().unwrap_or_else(|_| Err(anyhow!("docker {verb} panicked")))
            })
            .collect()
    });

    results.into_iter().find_map(Result::err).map_or(Ok(()), Err)
}

pub fn resolve_controller(check: &ProcessCheck) -> Controller {
    match &check.source {
        Source::Systemd(unit) => Controller::Systemd(unit.clone()),
        Source::Docker(target) => Controller::Docker {
            target: target.clone(),
            min_running: check.min_running.unwrap_or(1) as usize,
        },
        Source::Process(_) => Controller::None,
    }
}

impl DockerTarget {
    pub fn list(&self) -> Result<Vec<Listed>> {
        docker_read::run(&DockerRead::List(self.read_target()))
            .map(|output| parse_listing(&output))
    }

    pub fn list_states(&self) -> Result<Vec<(String, bool)>> {
        Ok(self
            .list()?
            .into_iter()
            .map(|listed| (listed.id, listed.running))
            .collect())
    }

    pub fn list_running_ids(&self) -> Vec<String> {
        self.list()
            .unwrap_or_default()
            .into_iter()
            .filter(|listed| listed.running)
            .map(|listed| listed.id)
            .collect()
    }

    pub fn list_names(&self) -> Vec<String> {
        self.list()
            .unwrap_or_default()
            .into_iter()
            .map(|listed| listed.name)
            .collect()
    }

    fn exact_name(&self) -> Option<&str> {
        match self {
            Self::Name(name) if !name.contains('*') => Some(name),
            _ => None,
        }
    }

    fn pick_newest(&self, count: usize) -> Vec<String> {
        if let Some(name) = self.exact_name() {
            return vec![name.to_string()];
        }

        let names = self.list_names();
        if names.len() <= count {
            return names;
        }

        let picked = pick_last_started(&inspect_containers(&names), count);
        if picked.is_empty() {
            return names.into_iter().take(count).collect();
        }

        picked
    }

    fn pick_for(&self, verb: &str, min_running: usize) -> Vec<String> {
        if let Some(name) = self.exact_name() {
            return vec![name.to_string()];
        }

        if verb != "start" {
            let running: Vec<String> = self
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|listed| listed.running)
                .map(|listed| listed.name)
                .collect();
            if !running.is_empty() {
                return running;
            }
        }

        self.pick_newest(min_running)
    }

    fn pick_missing(&self, min_running: usize) -> Vec<String> {
        let names = self.list_names();

        pick_stopped_of_running_images(&inspect_containers(&names), min_running)
    }

    pub fn read_target(&self) -> ListTarget {
        ListTarget {
            key: self.to_string(),
            filters: self.build_filters(),
        }
    }

    fn build_filters(&self) -> Vec<String> {
        match self {
            Self::Name(name) => vec![build_name_filter(name)],
            Self::Labels(labels) => labels
                .iter()
                .map(|(key, value)| format!("label={key}={value}"))
                .collect(),
        }
    }
}

fn build_name_filter(container: &str) -> String {
    let pattern: String = container
        .chars()
        .map(|character| match character {
            '*' => ".*".to_string(),
            character
                if character.is_ascii_alphanumeric()
                    || matches!(character, '_' | '-') =>
            {
                character.to_string()
            }
            character => format!("\\{character}"),
        })
        .collect();
    format!("name=^/?{pattern}$")
}

pub fn is_container_running(id: &str) -> bool {
    inspect_containers(&[id.to_string()])
        .first()
        .is_some_and(|facts| facts.running)
}

pub fn inspect_container(id: &str) -> Option<Container> {
    inspect_containers(&[id.to_string()]).pop().map(Container::from)
}

fn pick_stopped_of_running_images(
    containers: &[ContainerFacts],
    min_running: usize,
) -> Vec<String> {
    let (running, mut stopped): (Vec<_>, Vec<_>) =
        containers.iter().partition(|container| container.running);

    let running_images: HashSet<Option<&str>> = running
        .iter()
        .map(|container| container.image_id.as_deref())
        .collect();
    stopped.retain(|container| running_images.contains(&container.image_id.as_deref()));
    stopped.sort_by_key(|container| std::cmp::Reverse(container.started()));

    stopped
        .into_iter()
        .filter(|container| container.started().is_some())
        .take(min_running.saturating_sub(running.len()))
        .map(|container| container.short_name().to_string())
        .collect()
}

fn pick_last_started(containers: &[ContainerFacts], count: usize) -> Vec<String> {
    let mut started: Vec<_> = containers
        .iter()
        .filter_map(|container| Some((container.started()?, container.short_name())))
        .collect();
    started.sort_by_key(|(started, _)| std::cmp::Reverse(*started));

    started
        .into_iter()
        .take(count)
        .map(|(_, name)| name.to_string())
        .collect()
}

pub fn control_agent(verb: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args([verb, "daroyan"])
        .status()
        .context("run systemctl")?;
    if !status.success() && verb != "status" {
        bail!("systemctl {verb} daroyan failed with {status}");
    }
    Ok(())
}

fn run_with_sudo_fallback(program: &str, args: &[&str]) -> Result<String> {
    let error = match run_quietly(program, args) {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    if !is_permission_error(&error.to_string()) {
        return Err(error);
    }
    let sudo_args: Vec<&str> =
        ["-n", program].into_iter().chain(args.iter().copied()).collect();
    run_quietly("sudo", &sudo_args)
}

fn run_quietly(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("run {program}"))?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn is_permission_error(message: &str) -> bool {
    message.contains("Access denied")
        || message.contains("Interactive authentication required")
        || message
            .to_lowercase()
            .contains("permission denied while trying to connect")
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_name_filters() {
        use super::build_name_filter;
        assert_eq!(build_name_filter("redis-cache"), "name=^/?redis-cache$");
        assert_eq!(build_name_filter("linkity-api-*"), "name=^/?linkity-api-.*$");
        assert_eq!(build_name_filter("web.1"), "name=^/?web\\.1$");
    }

    use super::*;

    #[test]
    fn builds_docker_filters() {
        let labels = DockerTarget::Labels(
            [
                ("com.docker.compose.project", "shop"),
                ("com.docker.compose.service", "web"),
            ]
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .into(),
        );
        assert_eq!(
            labels.build_filters(),
            [
                "label=com.docker.compose.project=shop",
                "label=com.docker.compose.service=web"
            ]
        );
        assert_eq!(
            DockerTarget::Name("api-*".to_string()).build_filters(),
            ["name=^/?api-.*$"]
        );
    }

    fn facts(
        name: &str,
        image_id: &str,
        running: bool,
        started_at: &str,
    ) -> ContainerFacts {
        let line = format!(
            r#"{{"id":"{name}-id","name":"/{name}","image":"app:1","image_id":"{image_id}","running":{running},"started_at":"{started_at}","finished_at":"0001-01-01T00:00:00Z","exit_code":0,"oom_killed":false,"restart_count":0,"project":null,"service":null,"config_hash":null}}"#
        );
        parse_facts(&line).pop().unwrap()
    }

    #[test]
    fn picks_the_container_started_last() {
        let containers = [
            facts("green", "sha256:a", true, "2026-11-05T15:08:51.1Z"),
            facts("blue", "sha256:a", true, "2026-11-05T15:26:41.5Z"),
            facts("old", "sha256:a", false, "not a time"),
        ];
        assert_eq!(pick_last_started(&containers, 1), ["blue"]);
        assert_eq!(pick_last_started(&containers, 2), ["blue", "green"]);
        assert!(pick_last_started(&[], 1).is_empty());
    }

    #[test]
    fn restarts_only_stopped_containers_of_the_running_image() {
        let containers = [
            facts("green-1", "sha256:new", true, "2026-11-05T10:05:00Z"),
            facts("green-2", "sha256:new", false, "2026-11-05T10:05:01Z"),
            facts("blue-1", "sha256:old", false, "2026-11-05T09:00:00Z"),
            facts("blue-2", "sha256:old", false, "2026-11-05T09:00:01Z"),
        ];
        assert_eq!(pick_stopped_of_running_images(&containers, 2), ["green-2"]);
        assert_eq!(pick_stopped_of_running_images(&containers, 4), ["green-2"]);
        assert!(pick_stopped_of_running_images(&containers, 1).is_empty());

        let all_stopped: Vec<ContainerFacts> = containers
            .into_iter()
            .map(|container| ContainerFacts { running: false, ..container })
            .collect();
        assert!(pick_stopped_of_running_images(&all_stopped, 2).is_empty());
    }

    #[test]
    fn controls_an_exact_container_by_its_name() {
        let target = DockerTarget::Name("redis-cache".to_string());
        assert_eq!(target.pick_for("stop", 1), ["redis-cache"]);
        assert_eq!(target.pick_newest(3), ["redis-cache"]);
    }

    fn make_check(source: Source) -> ProcessCheck {
        ProcessCheck {
            name: "x".to_string(),
            source,
            user: None,
            restart: None,
            logs: None,
            min_running: None,
        }
    }

    #[test]
    fn detects_permission_errors() {
        assert!(is_permission_error("Failed to restart nginx.service: Access denied"));
        assert!(is_permission_error(
            "Failed to restart nginx.service: Interactive authentication required."
        ));
        assert!(is_permission_error(
            "permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock"
        ));
        assert!(is_permission_error(
            "permission denied while trying to connect to the docker API at unix:///var/run/docker.sock"
        ));
        assert!(!is_permission_error("Unit nginx.service not found."));
    }

    #[test]
    fn reads_systemd_exits() {
        let oom = "Result=oom-kill\nExecMainCode=2\nExecMainStatus=9\nNRestarts=3\nExecMainStartTimestampMonotonic=1000000\nExecMainExitTimestampMonotonic=187201000000\n";
        assert_eq!(
            parse_systemd_exit(oom),
            ExitInfo {
                code: None,
                signal: Some("SIGKILL".to_string()),
                oom: true,
                ran_secs: Some(187_200),
                restarts: Some(3),
            }
        );

        let failed = "Result=exit-code\nExecMainCode=1\nExecMainStatus=1\nNRestarts=0\nExecMainStartTimestampMonotonic=0\nExecMainExitTimestampMonotonic=0\n";
        let exit = parse_systemd_exit(failed);
        assert_eq!(
            (exit.code, exit.signal, exit.oom, exit.ran_secs),
            (Some(1), None, false, None)
        );
    }

    #[test]
    fn reads_containers() {
        let output = r#"{"id":"4f2a9c","name":"/inboxjot-api-1","image":"ghcr.io/inboxjot/api:0.0.19","image_id":"sha256:02b8","running":true,"started_at":"2026-10-26T04:42:03Z","finished_at":"0001-01-01T00:00:00Z","exit_code":0,"oom_killed":false,"restart_count":0,"project":"inboxjot","service":"api","config_hash":"df47"}
{"id":"5e1d","name":"/db","image":"alpine:3.20","image_id":"sha256:d9e8","running":true,"started_at":"2026-10-26T04:42:03Z","finished_at":"0001-01-01T00:00:00Z","exit_code":0,"oom_killed":false,"restart_count":0,"project":null,"service":null,"config_hash":null}
not json"#;
        let mut containers = parse_facts(output).into_iter().map(Container::from);
        assert_eq!(
            containers.next(),
            Some(Container {
                id: "4f2a9c".to_string(),
                name: "inboxjot-api-1".to_string(),
                image: "ghcr.io/inboxjot/api:0.0.19".to_string(),
                project: Some("inboxjot".to_string()),
                service: Some("api".to_string()),
                image_id: Some("sha256:02b8".to_string()),
                config_hash: Some("df47".to_string()),
            })
        );
        let db = containers.next().unwrap();
        assert_eq!((db.name.as_str(), db.project, db.config_hash), ("db", None, None));
        assert!(containers.next().is_none());
    }

    #[test]
    fn reads_docker_exits() {
        let killed = ContainerFacts {
            started_at: "2026-10-26T04:42:03.123456789Z".to_string(),
            finished_at: "2026-10-28T08:42:03.5Z".to_string(),
            exit_code: Some(137),
            oom_killed: true,
            restart_count: Some(2),
            ..facts("api", "sha256:a", false, "")
        };
        assert_eq!(
            killed.read_exit(),
            ExitInfo {
                code: Some(137),
                signal: Some("SIGKILL".to_string()),
                oom: true,
                ran_secs: Some(187_200),
                restarts: Some(2),
            }
        );

        let clean = facts("api", "sha256:a", false, "0001-01-01T00:00:00Z").read_exit();
        assert_eq!((clean.code, clean.signal, clean.oom), (Some(0), None, false));
    }

    #[test]
    fn the_source_decides_the_controller() {
        let target = DockerTarget::Name("redis-cache".to_string());
        let docker = make_check(Source::Docker(target.clone()));
        let process = make_check(Source::Process("redis-server".to_string()));
        assert_eq!(
            resolve_controller(&docker),
            Controller::Docker { target, min_running: 1 }
        );
        assert_eq!(resolve_controller(&process), Controller::None);
        assert!(Controller::None.run("restart").is_err());
    }
}
