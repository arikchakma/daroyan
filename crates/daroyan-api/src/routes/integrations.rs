use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Redirect;
use daroyan_core::alerts::connect::{
    ConnectStatus, Connected, Provider, SlackInstall, build_authorize_url,
    build_slack_manifest_url, exchange_discord_code, exchange_slack_code,
    list_slack_channels, read_telegram_bot_username, spawn_telegram_poller,
};
use daroyan_core::alerts::engine::deliver_notice;
use daroyan_core::alerts::integrations::{IntegrationConfig, validate_integration};
use daroyan_core::alerts::notify::{Details, Notice, Tone, send_notice};
use daroyan_core::db::entities::integrations::Model as Integration;
use daroyan_core::db::ids::IntegrationId;
use daroyan_core::db::queries::integrations::{IntegrationChanges, NewIntegration};
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::integration::IntegrationView;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_integrations, create_integration))
        .routes(routes!(update_integration, delete_integration))
        .routes(routes!(test_integration))
        .routes(routes!(test_integration_config))
        .routes(routes!(list_providers))
        .routes(routes!(connect_slack))
        .routes(routes!(connect_discord))
        .routes(routes!(connect_telegram))
        .routes(routes!(read_telegram_connect))
        .routes(routes!(finish_slack))
        .routes(routes!(finish_discord))
        .routes(routes!(list_slack_install_channels))
        .routes(routes!(finish_slack_install))
}

#[derive(Deserialize, ToSchema)]
struct CreateIntegrationBody {
    name: String,
    config: IntegrationConfig,
}

#[derive(Deserialize, ToSchema)]
struct UpdateIntegrationBody {
    name: Option<String>,
    config: Option<IntegrationConfig>,
}

#[derive(Deserialize, ToSchema)]
struct TestIntegrationBody {
    config: IntegrationConfig,
}

async fn find_integration(state: &AppState, id: &IntegrationId) -> Result<Integration> {
    state
        .database
        .find_integration(id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Integration not found"))
}

fn read_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::new(ErrorName::ValidationError, "Add a name"));
    }
    Ok(name.to_string())
}

fn make_test_notice(state: &AppState) -> Notice {
    Notice {
        event: "test",
        tone: Tone::Note,
        title: "Test from Daroyan".to_string(),
        body: "This integration works. Alerts for its rules will arrive here."
            .to_string(),
        url: state.config.app_url.clone(),
        data: json!({}),
        details: Details::default(),
    }
}

#[utoipa::path(get, path = "/v1/integrations", responses((status = 200, body = Vec<IntegrationView>)))]
async fn list_integrations(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<IntegrationView>>> {
    let integrations = state.database.list_integrations().await?;
    Ok(Json(integrations.into_iter().map(IntegrationView::from).collect()))
}

#[utoipa::path(
    post,
    path = "/v1/integrations",
    request_body = CreateIntegrationBody,
    responses((status = 201, body = IntegrationView))
)]
async fn create_integration(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<CreateIntegrationBody>,
) -> Result<(StatusCode, Json<IntegrationView>)> {
    validate_integration(&body.config)?;
    let integration = state
        .database
        .create_integration(NewIntegration {
            id: new_id(Prefix::Integration),
            name: read_name(&body.name)?,
            kind: body.config.kind().to_string(),
            config: serde_json::to_value(&body.config)?,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(integration.into())))
}

#[utoipa::path(
    patch,
    path = "/v1/integrations/{integration_id}",
    params(("integration_id" = String, Path)),
    request_body = UpdateIntegrationBody,
    responses((status = 200, body = IntegrationView))
)]
async fn update_integration(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(integration_id): Path<IntegrationId>,
    Json(body): Json<UpdateIntegrationBody>,
) -> Result<Json<IntegrationView>> {
    find_integration(&state, &integration_id).await?;
    if let Some(config) = &body.config {
        validate_integration(config)?;
    }
    let integration = state
        .database
        .update_integration(
            &integration_id,
            IntegrationChanges {
                name: body.name.as_deref().map(read_name).transpose()?,
                kind_and_config: body
                    .config
                    .map(|config| -> Result<_> {
                        Ok((config.kind().to_string(), serde_json::to_value(&config)?))
                    })
                    .transpose()?,
            },
        )
        .await?;
    Ok(Json(integration.into()))
}

#[utoipa::path(
    delete,
    path = "/v1/integrations/{integration_id}",
    params(("integration_id" = String, Path)),
    responses((status = 204))
)]
async fn delete_integration(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(integration_id): Path<IntegrationId>,
) -> Result<StatusCode> {
    if !state.database.delete_integration(&integration_id).await? {
        return Err(Error::new(ErrorName::NotFound, "Integration not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/integrations/{integration_id}/test",
    params(("integration_id" = String, Path)),
    responses((status = 200, body = IntegrationView))
)]
async fn test_integration(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(integration_id): Path<IntegrationId>,
) -> Result<Json<IntegrationView>> {
    let integration = find_integration(&state, &integration_id).await?;
    deliver_notice(&state.database, &integration, &make_test_notice(&state), None)
        .await
        .map_err(|error| Error::new(ErrorName::ValidationError, error))?;
    Ok(Json(find_integration(&state, &integration_id).await?.into()))
}

#[utoipa::path(
    post,
    path = "/v1/integrations/test",
    request_body = TestIntegrationBody,
    responses((status = 204))
)]
async fn test_integration_config(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Json(body): Json<TestIntegrationBody>,
) -> Result<StatusCode> {
    validate_integration(&body.config)?;
    send_notice(&body.config, &make_test_notice(&state))
        .await
        .map_err(|error| Error::new(ErrorName::ValidationError, error))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
#[schema(as = IntegrationProviders)]
struct ProvidersView {
    slack: bool,
    discord: bool,
    telegram: bool,
    slack_manifest_url: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = IntegrationConnect)]
struct ConnectView {
    url: String,
    state: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = TelegramConnectStatus)]
struct TelegramStatusView {
    is_connected: bool,
    integration: Option<IntegrationView>,
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn is_oauth_ready(state: &AppState, provider: Provider) -> bool {
    let config = &state.config;
    let has_app = match provider {
        Provider::Slack => {
            config.slack_client_id.is_some() && config.slack_client_secret.is_some()
        }
        Provider::Discord => {
            config.discord_client_id.is_some() && config.discord_client_secret.is_some()
        }
        Provider::Telegram => config.telegram_bot_token.is_some(),
    };
    has_app && config.app_url.is_some()
}

#[utoipa::path(
    get,
    path = "/v1/integrations/providers",
    responses((status = 200, body = ProvidersView))
)]
async fn list_providers(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProvidersView>> {
    let slack = is_oauth_ready(&state, Provider::Slack);
    Ok(Json(ProvidersView {
        slack,
        discord: is_oauth_ready(&state, Provider::Discord),
        telegram: state.config.telegram_bot_token.is_some(),
        slack_manifest_url: (!slack)
            .then(|| build_slack_manifest_url(&state.config).ok())
            .flatten(),
    }))
}

fn start_oauth(state: &AppState, provider: Provider) -> Result<Json<ConnectView>> {
    let connect_state = state.connects.start(provider);
    let url = build_authorize_url(&state.config, provider, &connect_state)?;
    Ok(Json(ConnectView { url, state: connect_state }))
}

#[utoipa::path(
    post,
    path = "/v1/integrations/slack/connect",
    responses((status = 200, body = ConnectView))
)]
async fn connect_slack(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
) -> Result<Json<ConnectView>> {
    start_oauth(&state, Provider::Slack)
}

#[utoipa::path(
    post,
    path = "/v1/integrations/discord/connect",
    responses((status = 200, body = ConnectView))
)]
async fn connect_discord(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
) -> Result<Json<ConnectView>> {
    start_oauth(&state, Provider::Discord)
}

#[utoipa::path(
    post,
    path = "/v1/integrations/telegram/connect",
    responses((status = 200, body = ConnectView))
)]
async fn connect_telegram(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
) -> Result<Json<ConnectView>> {
    let bot_token = state.config.telegram_bot_token.clone().ok_or_else(|| {
        Error::new(
            ErrorName::Conflict,
            "Set TELEGRAM_BOT_TOKEN on the API to connect in one click",
        )
    })?;
    let username = read_telegram_bot_username(&bot_token).await?;
    let connect_state = state.connects.start(Provider::Telegram);
    spawn_telegram_poller(state.database.clone(), state.connects.clone(), bot_token);
    Ok(Json(ConnectView {
        url: format!("https://t.me/{username}?startgroup={connect_state}"),
        state: connect_state,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/integrations/telegram/connect/{state}",
    params(("state" = String, Path)),
    responses((status = 200, body = TelegramStatusView))
)]
async fn read_telegram_connect(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(connect_state): Path<String>,
) -> Result<Json<TelegramStatusView>> {
    let status = state
        .connects
        .read_status(&connect_state, Provider::Telegram)
        .ok_or_else(|| {
            Error::new(ErrorName::NotFound, "This link expired. Start again")
        })?;
    let integration = match status {
        ConnectStatus::Pending => None,
        ConnectStatus::Connected(id) => Some(
            find_integration(&state, &IntegrationId::from(id.as_str()))
                .await?
                .into(),
        ),
    };
    Ok(Json(TelegramStatusView { is_connected: integration.is_some(), integration }))
}

#[utoipa::path(
    get,
    path = "/v1/integrations/slack/callback",
    params(("code" = Option<String>, Query), ("state" = Option<String>, Query)),
    responses((status = 303))
)]
async fn finish_slack(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Redirect {
    finish_oauth(&state, Provider::Slack, query).await
}

#[utoipa::path(
    get,
    path = "/v1/integrations/discord/callback",
    params(("code" = Option<String>, Query), ("state" = Option<String>, Query)),
    responses((status = 303))
)]
async fn finish_discord(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Redirect {
    finish_oauth(&state, Provider::Discord, query).await
}

async fn finish_oauth(
    state: &AppState,
    provider: Provider,
    query: CallbackQuery,
) -> Redirect {
    let app_url = state.config.app_url.clone().unwrap_or_default();
    let back = |params: &[(&str, &str)]| {
        let query = params
            .iter()
            .map(|(key, value)| format!("{key}={}", encode_query(value)))
            .collect::<Vec<_>>()
            .join("&");
        Redirect::to(&format!(
            "{}/integrations/connected?{query}",
            app_url.trim_end_matches('/')
        ))
    };
    let is_valid_state = query
        .state
        .as_deref()
        .is_some_and(|value| state.connects.take(value, provider));
    if !is_valid_state {
        return back(&[("error", "This link expired. Start again from Daroyan")]);
    }
    let Some(code) = query.code.filter(|_| query.error.is_none()) else {
        return back(&[("error", "The connection was cancelled")]);
    };
    if provider == Provider::Slack {
        return match exchange_slack_code(&state.config, &code).await {
            Ok(install) => {
                let install_id = state.connects.keep_slack_install(install);
                Redirect::to(&format!(
                    "{}/integrations/slack?install={install_id}",
                    app_url.trim_end_matches('/')
                ))
            }
            Err(error) => back(&[("error", &error.into_api().message)]),
        };
    }
    let connected = exchange_discord_code(&state.config, &code).await;
    let saved = match connected {
        Ok(connected) => save_connected(state, connected).await,
        Err(error) => Err(error),
    };
    match saved {
        Ok(integration) => back(&[("integration_id", &integration.id)]),
        Err(error) => back(&[("error", &error.into_api().message)]),
    }
}

async fn save_connected(state: &AppState, connected: Connected) -> Result<Integration> {
    state
        .database
        .create_integration(NewIntegration {
            id: new_id(Prefix::Integration),
            name: connected.name,
            kind: connected.config.kind().to_string(),
            config: serde_json::to_value(&connected.config)?,
        })
        .await
        .map_err(Into::into)
}

fn encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[derive(Serialize, ToSchema)]
#[schema(as = SlackChannel)]
struct SlackChannelView {
    id: String,
    name: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = SlackInstall)]
struct SlackInstallView {
    team_name: String,
    channels: Vec<SlackChannelView>,
}

#[derive(Deserialize, ToSchema)]
struct FinishSlackBody {
    channel_id: String,
}

fn find_slack_install(state: &AppState, install_id: &str) -> Result<SlackInstall> {
    state.connects.find_slack_install(install_id).ok_or_else(|| {
        Error::new(ErrorName::NotFound, "This Slack sign-in expired. Start again")
    })
}

#[utoipa::path(
    get,
    path = "/v1/integrations/slack/installs/{install_id}",
    params(("install_id" = String, Path)),
    responses((status = 200, body = SlackInstallView))
)]
async fn list_slack_install_channels(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(install_id): Path<String>,
) -> Result<Json<SlackInstallView>> {
    let install = find_slack_install(&state, &install_id)?;
    let channels = list_slack_channels(&install).await?;
    Ok(Json(SlackInstallView {
        team_name: install.team_name,
        channels: channels
            .into_iter()
            .map(|channel| SlackChannelView { id: channel.id, name: channel.name })
            .collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/v1/integrations/slack/installs/{install_id}",
    params(("install_id" = String, Path)),
    request_body = FinishSlackBody,
    responses((status = 201, body = IntegrationView))
)]
async fn finish_slack_install(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
    Path(install_id): Path<String>,
    Json(body): Json<FinishSlackBody>,
) -> Result<(StatusCode, Json<IntegrationView>)> {
    let install = find_slack_install(&state, &install_id)?;
    let channel = list_slack_channels(&install)
        .await?
        .into_iter()
        .find(|channel| channel.id == body.channel_id)
        .ok_or_else(|| {
            Error::new(ErrorName::ValidationError, "Pick a channel from the list")
        })?;
    let config = IntegrationConfig::Slack {
        api_url: install.api_url,
        bot_token: install.bot_token,
        channel_id: channel.id,
        channel_name: channel.name.clone(),
    };
    let welcome = Notice {
        event: "test",
        tone: Tone::Note,
        title: "Daroyan is connected".to_string(),
        body: format!("Alerts for the rules you pick will arrive in #{}.", channel.name),
        url: state.config.app_url.clone(),
        data: json!({}),
        details: Details::default(),
    };
    send_notice(&config, &welcome).await.map_err(|error| {
        Error::new(
            ErrorName::ValidationError,
            format!("Slack refused the message ({error})"),
        )
    })?;
    let integration =
        save_connected(&state, Connected { name: format!("#{}", channel.name), config })
            .await?;
    state.connects.remove_slack_install(&install_id);
    Ok((StatusCode::CREATED, Json(integration.into())))
}
