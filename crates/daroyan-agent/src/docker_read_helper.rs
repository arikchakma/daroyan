#[allow(dead_code)]
mod docker_read_protocol;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use docker_read_protocol::{ALLOW_FILE, DockerRead, ListTarget, parse_targets};

const SAFE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("daroyan-docker-read: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.into_string().map_err(|_| "arguments must be UTF-8".to_string()))
        .collect::<Result<Vec<String>, String>>()?;

    let allow_file = Path::new(ALLOW_FILE);
    check_owned_by_root(allow_file)?;
    let text = fs::read_to_string(allow_file)
        .map_err(|error| format!("read {ALLOW_FILE}: {error}"))?;
    let targets = parse_targets(&text);
    let read = DockerRead::parse(&args, &targets)?;

    let docker = find_docker()?;
    check_configured(&docker, &targets, read.containers())?;

    let error = Command::new(&docker)
        .args(read.docker_args())
        .env_clear()
        .env("PATH", SAFE_PATH)
        .exec();

    Err(format!("run {}: {error}", docker.display()))
}

fn check_owned_by_root(path: &Path) -> Result<(), String> {
    let directory = path.parent().ok_or("the allow file has no folder")?;
    for checked in [path, directory] {
        let metadata = fs::symlink_metadata(checked)
            .map_err(|error| format!("read {}: {error}", checked.display()))?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
        {
            return Err(format!(
                "{} must be owned by root and writable only by root",
                checked.display()
            ));
        }
    }

    Ok(())
}

fn find_docker() -> Result<PathBuf, String> {
    env::split_paths(SAFE_PATH)
        .map(|directory| directory.join("docker"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "docker is not installed".to_string())
}

fn check_configured(
    docker: &Path,
    targets: &[ListTarget],
    containers: &[String],
) -> Result<(), String> {
    if containers.is_empty() {
        return Ok(());
    }

    let mut allowed = HashSet::new();
    for target in targets {
        let output = Command::new(docker)
            .args(DockerRead::List(target.clone()).docker_args())
            .env_clear()
            .env("PATH", SAFE_PATH)
            .output()
            .map_err(|error| format!("list containers: {error}"))?;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            allowed.extend(line.split(' ').take(2).map(str::to_string));
        }
    }

    match containers.iter().find(|container| !allowed.contains(*container)) {
        Some(container) => Err(format!("{container} is not a container in the config")),
        None => Ok(()),
    }
}
