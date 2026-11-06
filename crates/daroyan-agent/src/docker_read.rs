use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};

use crate::docker_read_protocol::HELPER_NAME;
pub use crate::docker_read_protocol::{
    ALLOW_FILE, DockerRead, EVENT_ACTIONS, ListTarget, LogsRead, render_targets,
};
use crate::service::is_permission_error;

static USES_SUDO: AtomicBool = AtomicBool::new(false);

pub fn helper_path() -> PathBuf {
    env::args_os()
        .next()
        .map(PathBuf::from)
        .filter(|program| program.is_absolute())
        .or_else(|| env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin/daroyan"))
        .with_file_name(HELPER_NAME)
}

pub fn uses_sudo() -> bool {
    USES_SUDO.load(Ordering::Acquire)
}

pub fn require_sudo() {
    USES_SUDO.store(true, Ordering::Release);
}

pub fn command(read: &DockerRead, sudo: bool) -> Command {
    if !sudo {
        let mut command = Command::new("docker");
        command.args(read.docker_args());
        return command;
    }

    let mut command = Command::new("sudo");
    command.arg("-n").arg(helper_path()).args(read.helper_args());
    command
}

pub fn run(read: &DockerRead) -> Result<String> {
    if !uses_sudo() {
        let output = command(read, false).output().context("run docker")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let error = String::from_utf8_lossy(&output.stderr);
        if !is_permission_error(&error) {
            bail!("{}", error.trim());
        }
        require_sudo();
    }

    let output = command(read, true).output().context("run sudo")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
