use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use chrono::DateTime;
use daroyan_protocol::report::Container;

use crate::config::{DockerTarget, ProcessCheck, Source};

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

    pub fn restart_missing(&self) -> Result<()> {
        let Self::Docker { target, min_running } = self else {
            return self.run("restart");
        };

        let containers = target.pick_missing(*min_running);
        if containers.is_empty() {
            bail!("no stopped container of {target} to start");
        }

        run_on_each("restart", &containers)
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
                .and_then(|container| {
                    run_with_sudo_fallback(
                        "docker",
                        &[
                            "inspect",
                            "--format",
                            "{{.RestartCount}} {{json .State}}",
                            container,
                        ],
                    )
                    .ok()
                })
                .map(|output| parse_docker_exit(&output))
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

fn parse_docker_exit(output: &str) -> ExitInfo {
    let Some((restarts, state)) = output.trim().split_once(' ') else {
        return ExitInfo::default();
    };
    let Ok(state) = serde_json::from_str::<serde_json::Value>(state) else {
        return ExitInfo::default();
    };

    let read_time = |key: &str| DateTime::parse_from_rfc3339(state[key].as_str()?).ok();
    let code = state["ExitCode"].as_i64().and_then(|code| i32::try_from(code).ok());

    ExitInfo {
        code,
        signal: code
            .filter(|code| *code > 128)
            .and_then(|code| read_signal_name(code - 128)),
        oom: state["OOMKilled"].as_bool().unwrap_or_default(),
        ran_secs: read_time("StartedAt").zip(read_time("FinishedAt")).and_then(
            |(started, finished)| u64::try_from((finished - started).num_seconds()).ok(),
        ),
        restarts: restarts.parse().ok(),
    }
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
    pub fn list_ids(&self) -> Vec<String> {
        self.list_containers("{{.ID}}", None)
    }

    pub fn list_running_ids(&self) -> Vec<String> {
        self.list_containers("{{.ID}}", Some("status=running"))
    }

    pub fn list_names(&self) -> Vec<String> {
        self.list_containers("{{.Names}}", None)
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

        let mut args = vec!["inspect", "--format", "{{.Name}} {{.State.StartedAt}}"];
        args.extend(names.iter().map(String::as_str));
        let picked = run_with_sudo_fallback("docker", &args)
            .map(|output| pick_last_started(&output, count))
            .unwrap_or_default();

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
            let running = self.list_containers("{{.Names}}", Some("status=running"));
            if !running.is_empty() {
                return running;
            }
        }

        self.pick_newest(min_running)
    }

    fn pick_missing(&self, min_running: usize) -> Vec<String> {
        let names = self.list_names();
        if names.is_empty() {
            return Vec::new();
        }

        let mut args = vec![
            "inspect",
            "--format",
            "{{.Name}} {{.Image}} {{.State.Running}} {{.State.StartedAt}}",
        ];
        args.extend(names.iter().map(String::as_str));

        run_with_sudo_fallback("docker", &args)
            .map(|output| pick_stopped_of_running_images(&output, min_running))
            .unwrap_or_default()
    }

    fn list_containers(&self, format: &str, status: Option<&str>) -> Vec<String> {
        let filters: Vec<String> = self
            .build_filters()
            .into_iter()
            .chain(status.map(str::to_string))
            .collect();
        let mut args = vec!["ps", "--all", "--no-trunc", "--format", format];
        for filter in &filters {
            args.extend(["--filter", filter.as_str()]);
        }

        run_with_sudo_fallback("docker", &args)
            .map(|output| {
                output
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
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

pub fn inspect_container(id: &str) -> Option<Container> {
    let output =
        run_with_sudo_fallback("docker", &["inspect", "--format", "{{json .}}", id])
            .ok()?;
    parse_container(&output)
}

fn parse_container(output: &str) -> Option<Container> {
    let details: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    let labels = &details["Config"]["Labels"];
    let label = |key: &str| labels[key].as_str().map(str::to_string);

    Some(Container {
        id: details["Id"].as_str()?.to_string(),
        name: details["Name"].as_str()?.trim_start_matches('/').to_string(),
        image: details["Config"]["Image"].as_str().unwrap_or_default().to_string(),
        project: label("com.docker.compose.project"),
        service: label("com.docker.compose.service"),
        image_id: details["Image"].as_str().map(str::to_string),
        config_hash: label("com.docker.compose.config-hash"),
    })
}

struct ContainerRun<'a> {
    name: &'a str,
    image: &'a str,
    is_running: bool,
    started: DateTime<chrono::FixedOffset>,
}

impl<'a> ContainerRun<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let mut parts = line.split(' ');

        Some(Self {
            name: parts.next()?.trim_start_matches('/'),
            image: parts.next()?,
            is_running: parts.next()? == "true",
            started: DateTime::parse_from_rfc3339(parts.next()?).ok()?,
        })
    }
}

fn pick_stopped_of_running_images(output: &str, min_running: usize) -> Vec<String> {
    let containers: Vec<ContainerRun> =
        output.lines().filter_map(ContainerRun::parse).collect();
    let (running, mut stopped): (Vec<_>, Vec<_>) =
        containers.into_iter().partition(|container| container.is_running);

    let running_images: HashSet<&str> =
        running.iter().map(|container| container.image).collect();
    stopped.retain(|container| running_images.contains(container.image));
    stopped.sort_by_key(|container| std::cmp::Reverse(container.started));

    stopped
        .into_iter()
        .take(min_running.saturating_sub(running.len()))
        .map(|container| container.name.to_string())
        .collect()
}

fn pick_last_started(output: &str, count: usize) -> Vec<String> {
    let mut containers: Vec<_> = output
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter_map(|(name, started)| {
            Some((DateTime::parse_from_rfc3339(started).ok()?, name))
        })
        .collect();
    containers.sort_by_key(|(started, _)| std::cmp::Reverse(*started));

    containers
        .into_iter()
        .take(count)
        .map(|(_, name)| name.trim_start_matches('/').to_string())
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

    #[test]
    fn picks_the_container_started_last() {
        let output = "/green 2026-11-05T15:08:51.1Z\n/blue 2026-11-05T15:26:41.5Z\n/old 0001-01-01T00:00:00Z\n";
        assert_eq!(pick_last_started(output, 1), ["blue"]);
        assert_eq!(pick_last_started(output, 2), ["blue", "green"]);
        assert!(pick_last_started("garbage", 1).is_empty());
    }

    #[test]
    fn restarts_only_stopped_containers_of_the_running_image() {
        let output = "\
/green-1 sha256:new true 2026-11-05T10:05:00Z
/green-2 sha256:new false 2026-11-05T10:05:01Z
/blue-1 sha256:old false 2026-11-05T09:00:00Z
/blue-2 sha256:old false 2026-11-05T09:00:01Z
";
        assert_eq!(pick_stopped_of_running_images(output, 2), ["green-2"]);
        assert_eq!(pick_stopped_of_running_images(output, 4), ["green-2"]);
        assert!(pick_stopped_of_running_images(output, 1).is_empty());

        let all_stopped = output.replace("true", "false");
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
        let output = r#"{"Id":"4f2a9c","Name":"/inboxjot-api-1","Image":"sha256:02b8","Config":{"Image":"ghcr.io/inboxjot/api:0.0.19","Labels":{"com.docker.compose.project":"inboxjot","com.docker.compose.service":"api","com.docker.compose.config-hash":"df47"}}}"#;
        assert_eq!(
            parse_container(output),
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
        assert_eq!(parse_container("[]"), None);
    }

    #[test]
    fn reads_docker_exits() {
        let output = r#"2 {"Status":"exited","OOMKilled":true,"ExitCode":137,"StartedAt":"2026-10-26T04:42:03.123456789Z","FinishedAt":"2026-10-28T08:42:03.5Z"}"#;
        assert_eq!(
            parse_docker_exit(output),
            ExitInfo {
                code: Some(137),
                signal: Some("SIGKILL".to_string()),
                oom: true,
                ran_secs: Some(187_200),
                restarts: Some(2),
            }
        );

        let clean = r#"0 {"OOMKilled":false,"ExitCode":0,"StartedAt":"0001-01-01T00:00:00Z","FinishedAt":"0001-01-01T00:00:00Z"}"#;
        let exit = parse_docker_exit(clean);
        assert_eq!((exit.code, exit.signal, exit.oom), (Some(0), None, false));
        assert_eq!(parse_docker_exit("garbage"), ExitInfo::default());
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
