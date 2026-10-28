use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, bail};
use daroyan_protocol::message::{Command, CommandAction, CommandResult};
use log::info;
use tokio::task::spawn_blocking;

use crate::config::Config;
use crate::service::resolve_controller;

#[derive(Default)]
pub struct Held {
    pub stopped: HashSet<String>,
    pub restarted: HashSet<String>,
}

pub type HeldProcesses = Arc<Mutex<Held>>;

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
    {
        let mut held = hold(held);
        if verb == "stop" {
            held.stopped.insert(name.clone());
        } else {
            held.stopped.remove(&name);
        }
        if verb == "restart" {
            held.restarted.insert(name.clone());
        }
    }
    let outcome = spawn_blocking(move || controller.run(verb))
        .await
        .map_err(|error| anyhow!(error))
        .and_then(|outcome| outcome);
    if outcome.is_err() {
        hold(held).restarted.remove(&name);
    }
    outcome
}

pub fn hold(held: &HeldProcesses) -> std::sync::MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
