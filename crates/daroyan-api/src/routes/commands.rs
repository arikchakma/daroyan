use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use daroyan_core::commands::send_command;
use daroyan_core::db::ids::{CommandId, ServerId};
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_protocol::message::CommandAction;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::routes::servers::find_server;
use crate::views::command::CommandView;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_command))
        .routes(routes!(get_command))
}

#[derive(Deserialize, ToSchema)]
enum ActionName {
    #[serde(rename = "start_process")]
    Start,
    #[serde(rename = "stop_process")]
    Stop,
    #[serde(rename = "restart_process")]
    Restart,
}

#[derive(Deserialize, ToSchema)]
struct CommandBody {
    action: ActionName,
    name: String,
}

#[utoipa::path(
    post,
    path = "/v1/servers/{server_id}/commands",
    params(("server_id" = String, Path)),
    request_body = CommandBody,
    responses((status = 201, body = CommandView))
)]
async fn create_command(
    State(state): State<AppState>,
    AdminUser(current): AdminUser,
    Path(server_id): Path<ServerId>,
    Json(body): Json<CommandBody>,
) -> Result<(StatusCode, Json<CommandView>)> {
    let server = find_server(&state, &server_id).await?;
    let name = body.name.trim().to_string();
    let action = match body.action {
        ActionName::Start => CommandAction::StartProcess { name },
        ActionName::Stop => CommandAction::StopProcess { name },
        ActionName::Restart => CommandAction::RestartProcess { name },
    };
    let command =
        send_command(&state.database, &state.agents, &server, action, &current.user.id)
            .await?;
    Ok((StatusCode::CREATED, Json(command.into())))
}

#[derive(Deserialize)]
struct CommandPath {
    server_id: ServerId,
    command_id: CommandId,
}

#[utoipa::path(
    get,
    path = "/v1/servers/{server_id}/commands/{command_id}",
    params(("server_id" = String, Path), ("command_id" = String, Path)),
    responses((status = 200, body = CommandView))
)]
async fn get_command(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(path): Path<CommandPath>,
) -> Result<Json<CommandView>> {
    let command = state
        .database
        .find_command(&path.server_id, &path.command_id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Command not found"))?;
    Ok(Json(command.into()))
}
