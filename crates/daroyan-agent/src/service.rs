use std::process::Command;

use anyhow::{Context, Result, bail};

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

    pub fn read_exit_code(&self) -> Option<i32> {
        let output = match self {
            Self::Systemd(unit) => Command::new("systemctl")
                .args(["show", unit, "--property=ExecMainStatus", "--value"])
                .output(),
            Self::Docker(container) => Command::new("docker")
                .args([
                    "inspect",
                    "--format",
                    "{{.State.ExitCode}}",
                    &resolve_container(container),
                ])
                .output(),
            Self::None => return None,
        };
        String::from_utf8_lossy(&output.ok()?.stdout).trim().parse().ok()
    }
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
