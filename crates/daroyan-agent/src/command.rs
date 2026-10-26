use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, bail};
use daroyan_protocol::message::{Command, CommandAction, CommandResult};
use log::info;
use tokio::task::spawn_blocking;

use crate::config::Config;
use crate::service::resolve_controller;

pub type HeldProcesses = Arc<Mutex<HashSet<String>>>;

pub async fn run_command(
    command: Command,
    config: &Config,
    held: &HeldProcesses,
) -> CommandResult {
    let outcome = apply_action(command.action, config, held).await;
    CommandResult {
        id: command.id,
        ok: outcome.is_ok(),
        error: outcome.err().map(|error| format!("{error:#}")),
    }
}

async fn apply_action(
    action: CommandAction,
    config: &Config,
    held: &HeldProcesses,
) -> Result<()> {
    let (verb, name) = match action {
        CommandAction::StartProcess { name } => ("start", name),
        CommandAction::StopProcess { name } => ("stop", name),
        CommandAction::RestartProcess { name } => ("restart", name),
    };
    if !config.agent.commands {
        bail!("commands are turned off on this server");
    }
    let Some(check) = config.processes.iter().find(|check| check.name == name) else {
        bail!("{name} is not a watched process");
    };
    let controller = resolve_controller(check);

    info!("the dashboard asked to {verb} {name}");
    if verb == "stop" {
        hold(held).insert(name.clone());
    } else {
        hold(held).remove(&name);
    }
    spawn_blocking(move || controller.run(verb))
        .await
        .map_err(|error| anyhow!(error))?
}

pub fn hold(held: &HeldProcesses) -> std::sync::MutexGuard<'_, HashSet<String>> {
    held.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
