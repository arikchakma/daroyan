use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::DateTime;
use daroyan_protocol::report::Container;

use crate::config::{ProcessCheck, Source};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Controller {
    Systemd(String),
    Docker(String),
    None,
}

impl Controller {
    pub fn is_controllable(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn run(&self, verb: &str) -> Result<()> {
        let output = match self {
            Self::Systemd(unit) => run_with_sudo_fallback("systemctl", &[verb, unit]),
            Self::Docker(container) => {
                run_with_sudo_fallback("docker", &[verb, &resolve_container(container)])
            }
            Self::None => bail!(
                "it is watch-only, so Daroyan cannot {verb} it. Watch it with systemd or docker to control it"
            ),
        };
        output.map(drop)
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
            Self::Docker(container) => run_with_sudo_fallback(
                "docker",
                &[
                    "inspect",
                    "--format",
                    "{{.RestartCount}} {{json .State}}",
                    &resolve_container(container),
                ],
            )
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

pub fn resolve_controller(check: &ProcessCheck) -> Controller {
    match &check.source {
        Source::Systemd(unit) => Controller::Systemd(unit.clone()),
        Source::Docker(container) => Controller::Docker(container.clone()),
        Source::Process(_) => Controller::None,
    }
}

fn resolve_container(container: &str) -> String {
    if !container.contains('*') {
        return container.to_string();
    }
    find_container_id(container).unwrap_or_else(|| container.to_string())
}

fn build_name_filter(container: &str) -> String {
    let pattern: String = container
        .chars()
        .map(|character| match character {
            '*' => ".*".to_string(),
            character
                if character.is_ascii_alphanumeric() || "_-".contains(character) =>
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
    })
}

pub fn find_container_ids(container: &str) -> Vec<String> {
    let filter = build_name_filter(container);
    run_with_sudo_fallback(
        "docker",
        &["ps", "--all", "--no-trunc", "--filter", &filter, "--format", "{{.ID}}"],
    )
    .map(|output| {
        output
            .lines()
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

pub fn find_container_id(container: &str) -> Option<String> {
    find_container_ids(container).into_iter().next()
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

    fn make_check(source: Source) -> ProcessCheck {
        ProcessCheck {
            name: "x".to_string(),
            source,
            user: None,
            restart: None,
            logs: false,
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
        let output = r#"{"Id":"4f2a9c","Name":"/inboxjot-api-1","Config":{"Image":"ghcr.io/inboxjot/api:0.0.19","Labels":{"com.docker.compose.project":"inboxjot","com.docker.compose.service":"api"}}}"#;
        assert_eq!(
            parse_container(output),
            Some(Container {
                id: "4f2a9c".to_string(),
                name: "inboxjot-api-1".to_string(),
                image: "ghcr.io/inboxjot/api:0.0.19".to_string(),
                project: Some("inboxjot".to_string()),
                service: Some("api".to_string()),
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
        let docker = make_check(Source::Docker("redis-cache".to_string()));
        let process = make_check(Source::Process("redis-server".to_string()));
        assert_eq!(
            resolve_controller(&docker),
            Controller::Docker("redis-cache".to_string())
        );
        assert_eq!(resolve_controller(&process), Controller::None);
        assert!(Controller::None.run("restart").is_err());
    }
}
