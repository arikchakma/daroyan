use std::time::Duration;

use anyhow::{Context, bail};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use daroyan_core::commands::finish_command;
use daroyan_core::db::ids::ServerId;
use daroyan_core::error::{ErrorName, Result};
use daroyan_core::fleet_events::{AGENT_CONNECTED, AGENT_DISCONNECTED};
use daroyan_core::limits::new_socket_limiter;
use daroyan_core::servers::enroll::enroll_server;
use daroyan_core::servers::report::save_report;
use daroyan_protocol::enroll::{EnrollRequest, EnrollResponse};
use daroyan_protocol::message::{AgentMessage, ApiMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{self, Receiver};
use tokio::time::{Instant, interval, sleep_until};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AuthenticatedServer, read_bearer};
use crate::layers::client_ip::ClientIp;

const PING_INTERVAL: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(enroll_agent))
        .routes(routes!(connect_agent))
}

#[utoipa::path(
    post,
    path = "/v1/agent/enroll",
    request_body(content = Object),
    responses((status = 200, body = Object))
)]
async fn enroll_agent(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>> {
    state.limits.check_enroll(ip)?;
    state.limits.check_auth_blocked(ip)?;
    let install_token =
        read_bearer(&headers).inspect_err(|_| state.limits.record_auth_failure(ip))?;
    let enrolled = enroll_server(&state.database, &install_token, &body.host)
        .await
        .inspect_err(|error| {
            if error.name() == ErrorName::Unauthorized {
                state.limits.record_auth_failure(ip);
            }
        })?;
    state.hosts.forget();
    tracing::info!(server_id = %enrolled.server.id, name = %enrolled.server.name, "server enrolled");
    Ok(Json(EnrollResponse {
        server_id: enrolled.server.id,
        token: enrolled.token,
    }))
}

#[utoipa::path(get, path = "/v1/agent/connect", responses((status = 101)))]
async fn connect_agent(
    State(state): State<AppState>,
    agent: AuthenticatedServer,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    state.limits.check_connect(&agent.server.id)?;
    let server_id = ServerId(agent.server.id);
    Ok(upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| run_agent_socket(state, server_id, socket)))
}

async fn run_agent_socket(state: AppState, server_id: ServerId, socket: WebSocket) {
    let (sender, commands) = mpsc::channel(16);
    let generation = state.agents.register(server_id.as_str(), sender);
    state
        .analytics
        .buffer_server_event(server_id.as_str(), AGENT_CONNECTED);
    tracing::info!(server_id = %server_id, "agent connected");
    match relay_agent_socket(&state, &server_id, socket, commands).await {
        Ok(()) => tracing::info!(server_id = %server_id, "agent disconnected"),
        Err(error) => {
            tracing::info!(server_id = %server_id, error = %format!("{error:#}"), "agent dropped");
        }
    }
    let was_current = state.agents.unregister(server_id.as_str(), generation);
    if was_current {
        state
            .analytics
            .buffer_server_event(server_id.as_str(), AGENT_DISCONNECTED);
    }
}

async fn relay_agent_socket(
    state: &AppState,
    server_id: &ServerId,
    socket: WebSocket,
    mut commands: Receiver<ApiMessage>,
) -> anyhow::Result<()> {
    let (mut sink, mut stream) = socket.split();
    let limiter = new_socket_limiter();
    let mut ping = interval(PING_INTERVAL);
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping.tick() => sink.send(Message::Ping(Bytes::new())).await?,
            () = sleep_until(last_seen + IDLE_TIMEOUT) => {
                bail!("no message from the agent in {} seconds", IDLE_TIMEOUT.as_secs());
            }
            command = commands.recv() => match command {
                Some(command) => sink.send(Message::text(serde_json::to_string(&command)?)).await?,
                None => {
                    sink.send(Message::Close(None)).await.ok();
                    return Ok(());
                }
            },
            frame = stream.next() => {
                last_seen = Instant::now();
                match frame.context("the agent closed the connection")?? {
                    Message::Text(text) => {
                        if limiter.check().is_err() {
                            bail!("the agent sent too many messages");
                        }
                        handle_agent_message(state, server_id, &text).await;
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

async fn handle_agent_message(state: &AppState, server_id: &ServerId, text: &str) {
    let outcome = match serde_json::from_str::<AgentMessage>(text) {
        Ok(AgentMessage::Report(report)) => {
            save_report(&state.database, &state.analytics, server_id, report).await
        }
        Ok(AgentMessage::CommandResult(result)) => {
            finish_command(&state.database, server_id, result).await
        }
        Ok(AgentMessage::Logs(batch)) => {
            state.analytics.buffer_logs(server_id.as_str(), batch);
            Ok(())
        }
        Err(error) => {
            tracing::warn!(server_id = %server_id, error = %error, "ignored an agent message");
            Ok(())
        }
    };
    if let Err(error) = outcome {
        tracing::error!(server_id = %server_id, error = %error, "handle an agent message");
    }
}
