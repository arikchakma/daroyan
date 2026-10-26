use daroyan_protocol::message::{ApiMessage, Command, CommandAction, CommandResult};

use crate::agents::Agents;
use crate::db::Database;
use crate::db::entities::commands::Model as StoredCommand;
use crate::db::entities::servers::Model as Server;
use crate::db::ids::{CommandId, ServerId};
use crate::db::queries::commands::NewCommand;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};
use crate::servers::report::LastReport;

pub const STATUS_SENT: &str = "sent";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_FAILED: &str = "failed";
const MAX_ERROR_CHARS: usize = 4_000;

pub async fn send_command(
    database: &Database,
    agents: &Agents,
    server: &Server,
    action: CommandAction,
) -> Result<StoredCommand> {
    let (action_name, name) = match &action {
        CommandAction::StartProcess { name } => ("start_process", name),
        CommandAction::StopProcess { name } => ("stop_process", name),
        CommandAction::RestartProcess { name } => ("restart_process", name),
    };
    match find_controllable(server, name) {
        None => {
            return Err(Error::new(
                ErrorName::ValidationError,
                format!("{name} is not a watched process on this server"),
            ));
        }
        Some(false) => {
            return Err(Error::new(
                ErrorName::ValidationError,
                format!(
                    "{name} is not a systemd service or a container, so Daroyan cannot control it. Set unit or container on its check"
                ),
            ));
        }
        Some(true) => {}
    }
    if !agents.is_connected(&server.id) {
        return Err(Error::new(ErrorName::Conflict, "The server is offline"));
    }

    let command = database
        .create_command(NewCommand {
            id: new_id(Prefix::Command),
            server_id: server.id.clone(),
            action: action_name.to_string(),
            name: name.clone(),
            status: STATUS_SENT.to_string(),
        })
        .await?;
    let message = ApiMessage::Command(Command { id: command.id.clone(), action });
    if !agents.send(&server.id, message) {
        database
            .update_command_status(
                &ServerId::from(server.id.as_str()),
                &CommandId::from(command.id.as_str()),
                STATUS_FAILED,
                Some("The server disconnected".to_string()),
            )
            .await?;
        return Err(Error::new(ErrorName::Conflict, "The server is offline"));
    }
    Ok(command)
}

pub async fn finish_command(
    database: &Database,
    server_id: &ServerId,
    result: CommandResult,
) -> Result<()> {
    let status = if result.ok { STATUS_SUCCEEDED } else { STATUS_FAILED };
    database
        .update_command_status(
            server_id,
            &CommandId::from(result.id.as_str()),
            status,
            result
                .error
                .map(|error| error.chars().take(MAX_ERROR_CHARS).collect()),
        )
        .await?;
    Ok(())
}

fn find_controllable(server: &Server, name: &str) -> Option<bool> {
    server
        .last_report
        .clone()
        .and_then(|report| serde_json::from_value::<LastReport>(report).ok())?
        .processes
        .into_iter()
        .find(|process| process.name == name)
        .map(|process| process.controllable)
}
