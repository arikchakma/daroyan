use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rand::Rng;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::alerts::integrations::IntegrationConfig;
use crate::alerts::notify::{Details, Notice, Tone, send_notice};
use crate::config::Config;
use crate::db::Database;
use crate::db::queries::integrations::NewIntegration;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};

const STATE_TTL: Duration = Duration::from_secs(600);
const STATE_LENGTH: usize = 24;
const TIMEOUT: Duration = Duration::from_secs(15);
const SLACK_SCOPES: &str =
    "chat:write,chat:write.public,channels:read,users:read,users:read.email";
const SLACK_CHANNEL_LIMIT: &str = "500";
const DISCORD_SCOPES: &str = "webhook.incoming";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Slack,
    Discord,
    Telegram,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connected {
    pub name: String,
    pub config: IntegrationConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectStatus {
    Pending,
    Connected(String),
}

struct PendingConnect {
    provider: Provider,
    created_at: Instant,
    integration_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackInstall {
    pub api_url: String,
    pub bot_token: String,
    pub team_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackChannel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Default)]
pub struct ConnectStates {
    states: Arc<Mutex<HashMap<String, PendingConnect>>>,
    installs: Arc<Mutex<HashMap<String, (SlackInstall, Instant)>>>,
    is_polling: Arc<AtomicBool>,
}

impl ConnectStates {
    pub fn start(&self, provider: Provider) -> String {
        let state = make_state();
        let mut states = self.lock();
        states.retain(|_, pending| pending.created_at.elapsed() < STATE_TTL);
        states.insert(
            state.clone(),
            PendingConnect {
                provider,
                created_at: Instant::now(),
                integration_id: None,
            },
        );
        state
    }

    pub fn take(&self, state: &str, provider: Provider) -> bool {
        let mut states = self.lock();
        let is_valid = states.get(state).is_some_and(|pending| {
            pending.provider == provider && pending.created_at.elapsed() < STATE_TTL
        });
        if is_valid && provider != Provider::Telegram {
            states.remove(state);
        }
        is_valid
    }

    pub fn finish(&self, state: &str, integration_id: String) {
        if let Some(pending) = self.lock().get_mut(state) {
            pending.integration_id = Some(integration_id);
        }
    }

    pub fn read_status(&self, state: &str, provider: Provider) -> Option<ConnectStatus> {
        let states = self.lock();
        let pending = states.get(state).filter(|pending| {
            pending.provider == provider && pending.created_at.elapsed() < STATE_TTL
        })?;
        Some(match &pending.integration_id {
            Some(id) => ConnectStatus::Connected(id.clone()),
            None => ConnectStatus::Pending,
        })
    }

    pub fn list_pending(&self, provider: Provider) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(_, pending)| {
                pending.provider == provider
                    && pending.integration_id.is_none()
                    && pending.created_at.elapsed() < STATE_TTL
            })
            .map(|(state, _)| state.clone())
            .collect()
    }

    pub fn keep_slack_install(&self, install: SlackInstall) -> String {
        let id = make_state();
        let mut installs = self.installs.lock().unwrap_or_else(PoisonError::into_inner);
        installs.retain(|_, (_, created_at)| created_at.elapsed() < STATE_TTL);
        installs.insert(id.clone(), (install, Instant::now()));
        id
    }

    pub fn find_slack_install(&self, id: &str) -> Option<SlackInstall> {
        let installs = self.installs.lock().unwrap_or_else(PoisonError::into_inner);
        installs
            .get(id)
            .filter(|(_, created_at)| created_at.elapsed() < STATE_TTL)
            .map(|(install, _)| install.clone())
    }

    pub fn remove_slack_install(&self, id: &str) {
        self.installs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, PendingConnect>> {
        self.states.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn spawn_telegram_poller(
    database: Database,
    states: ConnectStates,
    bot_token: String,
) {
    if states.is_polling.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        let mut offset = 0;
        while !states.list_pending(Provider::Telegram).is_empty() {
            match read_telegram_starts(&bot_token, offset).await {
                Ok((next_offset, starts)) => {
                    offset = next_offset;
                    for start in starts {
                        if let Err(error) =
                            save_telegram_start(&database, &states, &bot_token, start)
                                .await
                        {
                            tracing::warn!(error = ?error, "save a telegram chat");
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(error = ?error, "read telegram updates");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
        states.is_polling.store(false, Ordering::SeqCst);
    });
}

async fn save_telegram_start(
    database: &Database,
    states: &ConnectStates,
    bot_token: &str,
    start: TelegramStart,
) -> Result<()> {
    if !states.list_pending(Provider::Telegram).contains(&start.state) {
        return Ok(());
    }
    let config = IntegrationConfig::Telegram {
        bot_token: bot_token.to_string(),
        chat_id: start.chat_id.clone(),
    };
    let integration = database
        .create_integration(NewIntegration {
            id: new_id(Prefix::Integration),
            name: start.chat_name.clone(),
            kind: config.kind().to_string(),
            config: serde_json::to_value(&config)?,
        })
        .await?;
    states.finish(&start.state, integration.id);
    let notice = Notice {
        event: "test",
        tone: Tone::Note,
        title: "Daroyan is connected".to_string(),
        body: format!(
            "Alerts for the rules you pick will arrive in {}.",
            start.chat_name
        ),
        url: None,
        data: Value::Null,
        details: Details::default(),
    };
    if let Err(error) = send_notice(&config, &notice, None).await {
        tracing::warn!(error = %error, "send the telegram welcome");
    }
    Ok(())
}

fn make_state() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::rng();
    (0..STATE_LENGTH)
        .map(|_| char::from(ALPHABET[rng.random_range(0..ALPHABET.len())]))
        .collect()
}

pub fn get_callback_url(config: &Config, provider: Provider) -> Result<String> {
    let app_url = config.app_url.as_deref().ok_or_else(|| {
        Error::new(ErrorName::Conflict, "Set APP_URL on the API to connect in one click")
    })?;
    let name = match provider {
        Provider::Slack => "slack",
        Provider::Discord => "discord",
        Provider::Telegram => "telegram",
    };
    Ok(format!("{}/v1/integrations/{name}/callback", app_url.trim_end_matches('/')))
}

pub fn build_authorize_url(
    config: &Config,
    provider: Provider,
    state: &str,
) -> Result<String> {
    let redirect_uri = get_callback_url(config, provider)?;
    let missing =
        || Error::new(ErrorName::Conflict, "This integration is not set up on the API");
    let url = match provider {
        Provider::Slack => Url::parse_with_params(
            &format!("{}/oauth/v2/authorize", config.slack_api_url.trim_end_matches('/')),
            [
                ("client_id", config.slack_client_id.as_deref().ok_or_else(missing)?),
                ("scope", SLACK_SCOPES),
                ("redirect_uri", &redirect_uri),
                ("state", state),
            ],
        ),
        Provider::Discord => Url::parse_with_params(
            "https://discord.com/oauth2/authorize",
            [
                ("client_id", config.discord_client_id.as_deref().ok_or_else(missing)?),
                ("response_type", "code"),
                ("scope", DISCORD_SCOPES),
                ("redirect_uri", &redirect_uri),
                ("state", state),
            ],
        ),
        Provider::Telegram => return Err(missing()),
    };
    url.map(String::from).map_err(|error| Error::Internal(error.into()))
}

pub fn build_slack_manifest_url(config: &Config) -> Result<String> {
    let redirect_uri = get_callback_url(config, Provider::Slack)?;
    let manifest = json!({
        "display_information": {
            "name": "Daroyan",
            "description": "Posts Daroyan alerts to Slack channels",
            "background_color": "#18181b",
        },
        "features": { "bot_user": { "display_name": "daroyan", "always_online": true } },
        "oauth_config": {
            "redirect_urls": [redirect_uri],
            "scopes": { "bot": SLACK_SCOPES.split(',').collect::<Vec<_>>() },
        },
        "settings": {
            "interactivity": {
                "is_enabled": true,
                "request_url": format!("{}/v1/integrations/slack/interactions", config.app_url.as_deref().unwrap_or_default().trim_end_matches('/')),
            },
            "org_deploy_enabled": false,
            "socket_mode_enabled": false,
            "token_rotation_enabled": false,
        },
    });
    Url::parse_with_params(
        "https://api.slack.com/apps",
        [("new_app", "1"), ("manifest_json", &manifest.to_string())],
    )
    .map(String::from)
    .map_err(|error| Error::Internal(error.into()))
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

fn reject(message: impl Into<String>) -> Error {
    Error::new(ErrorName::ValidationError, message)
}

pub async fn exchange_slack_code(config: &Config, code: &str) -> Result<SlackInstall> {
    let (Some(client_id), Some(client_secret)) =
        (config.slack_client_id.as_deref(), config.slack_client_secret.as_deref())
    else {
        return Err(reject("Slack is not set up on the API"));
    };
    let redirect_uri = get_callback_url(config, Provider::Slack)?;
    let api_url = config.slack_api_url.trim_end_matches('/').to_string();
    let response: Value = build_client()
        .post(format!("{api_url}/api/oauth.v2.access"))
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .map_err(|_| reject("Slack did not answer. Try again"))?
        .json()
        .await
        .map_err(|_| reject("Slack sent an unexpected answer"))?;
    read_slack_access(&response, api_url)
}

fn read_slack_access(response: &Value, api_url: String) -> Result<SlackInstall> {
    if response["ok"] != true {
        let reason = response["error"].as_str().unwrap_or("unknown_error");
        return Err(reject(format!("Slack refused the connection ({reason})")));
    }
    let bot_token = response["access_token"]
        .as_str()
        .filter(|token| token.starts_with("xoxb-"))
        .ok_or_else(|| reject("Slack did not return a bot token"))?;
    Ok(SlackInstall {
        api_url,
        bot_token: bot_token.to_string(),
        team_name: response["team"]["name"].as_str().unwrap_or("Slack").to_string(),
    })
}

pub async fn list_slack_channels(install: &SlackInstall) -> Result<Vec<SlackChannel>> {
    let response: Value = build_client()
        .post(format!("{}/api/conversations.list", install.api_url))
        .bearer_auth(&install.bot_token)
        .form(&[
            ("types", "public_channel"),
            ("exclude_archived", "true"),
            ("limit", SLACK_CHANNEL_LIMIT),
        ])
        .send()
        .await
        .map_err(|_| reject("Slack did not answer. Try again"))?
        .json()
        .await
        .map_err(|_| reject("Slack sent an unexpected answer"))?;
    read_slack_channels(&response)
}

fn read_slack_channels(response: &Value) -> Result<Vec<SlackChannel>> {
    if response["ok"] != true {
        let reason = response["error"].as_str().unwrap_or("unknown_error");
        return Err(reject(format!("Slack refused to list channels ({reason})")));
    }
    let mut channels: Vec<SlackChannel> = response["channels"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|channel| {
            Some(SlackChannel {
                id: channel["id"].as_str()?.to_string(),
                name: channel["name"].as_str()?.to_string(),
            })
        })
        .collect();
    channels.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(channels)
}

pub async fn exchange_discord_code(config: &Config, code: &str) -> Result<Connected> {
    let (Some(client_id), Some(client_secret)) =
        (config.discord_client_id.as_deref(), config.discord_client_secret.as_deref())
    else {
        return Err(reject("Discord is not set up on the API"));
    };
    let redirect_uri = get_callback_url(config, Provider::Discord)?;
    let response: Value = build_client()
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .map_err(|_| reject("Discord did not answer. Try again"))?
        .json()
        .await
        .map_err(|_| reject("Discord sent an unexpected answer"))?;
    read_discord_token(&response)
}

fn read_discord_token(response: &Value) -> Result<Connected> {
    let hook = &response["webhook"];
    let url = hook["url"].as_str().ok_or_else(|| {
        let reason = response["error_description"]
            .as_str()
            .or_else(|| response["error"].as_str())
            .unwrap_or("no webhook");
        reject(format!("Discord refused the connection ({reason})"))
    })?;
    Ok(Connected {
        name: hook["name"].as_str().unwrap_or("Discord channel").to_string(),
        config: IntegrationConfig::Discord { webhook_url: url.to_string() },
    })
}

#[derive(Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct TelegramBot {
    username: String,
}

pub async fn read_telegram_bot_username(bot_token: &str) -> Result<String> {
    let response: TelegramResponse<TelegramBot> = build_client()
        .get(format!("https://api.telegram.org/bot{bot_token}/getMe"))
        .send()
        .await
        .map_err(|_| reject("Telegram did not answer. Try again"))?
        .json()
        .await
        .map_err(|_| reject("Telegram sent an unexpected answer"))?;
    match (response.ok, response.result) {
        (true, Some(bot)) => Ok(bot.username),
        _ => Err(reject(format!(
            "Telegram refused the bot token ({})",
            response.description.unwrap_or_default()
        ))),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelegramStart {
    pub update_id: i64,
    pub state: String,
    pub chat_id: String,
    pub chat_name: String,
}

pub async fn read_telegram_starts(
    bot_token: &str,
    offset: i64,
) -> Result<(i64, Vec<TelegramStart>)> {
    let response: TelegramResponse<Vec<Value>> = build_client()
        .get(format!("https://api.telegram.org/bot{bot_token}/getUpdates"))
        .query(&[("offset", offset.to_string()), ("timeout", "10".to_string())])
        .send()
        .await
        .map_err(|_| reject("Telegram did not answer"))?
        .json()
        .await
        .map_err(|_| reject("Telegram sent an unexpected answer"))?;
    if !response.ok {
        return Err(reject(format!(
            "Telegram refused to list updates ({})",
            response.description.unwrap_or_default()
        )));
    }
    let updates = response.result.unwrap_or_default();
    let next_offset = updates
        .iter()
        .filter_map(|update| update["update_id"].as_i64())
        .max()
        .map_or(offset, |id| id + 1);
    Ok((next_offset, updates.iter().filter_map(read_telegram_start).collect()))
}

fn read_telegram_start(update: &Value) -> Option<TelegramStart> {
    let message = update.get("message").or_else(|| update.get("my_chat_member"))?;
    let text = message["text"].as_str()?;
    let mut words = text.split_whitespace();
    let command = words.next()?;
    if command != "/start" && !command.starts_with("/start@") {
        return None;
    }
    let chat = &message["chat"];
    let chat_name = chat["title"]
        .as_str()
        .map(str::to_string)
        .or_else(|| chat["username"].as_str().map(|name| format!("@{name}")))
        .or_else(|| chat["first_name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "Telegram chat".to_string());
    Some(TelegramStart {
        update_id: update["update_id"].as_i64()?,
        state: words.next()?.to_string(),
        chat_id: chat["id"].as_i64()?.to_string(),
        chat_name,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn states_are_single_use_and_scoped_to_a_provider() {
        let states = ConnectStates::default();
        let state = states.start(Provider::Slack);
        assert!(!states.take(&state, Provider::Discord));
        assert!(states.take(&state, Provider::Slack));
        assert!(!states.take(&state, Provider::Slack));

        let telegram = states.start(Provider::Telegram);
        assert_eq!(
            states.list_pending(Provider::Telegram),
            std::slice::from_ref(&telegram)
        );
        assert_eq!(
            states.read_status(&telegram, Provider::Telegram),
            Some(ConnectStatus::Pending)
        );
        states.finish(&telegram, "int_1".to_string());
        assert_eq!(
            states.read_status(&telegram, Provider::Telegram),
            Some(ConnectStatus::Connected("int_1".to_string()))
        );
        assert!(states.list_pending(Provider::Telegram).is_empty());
    }

    #[test]
    fn reads_the_slack_and_discord_answers() {
        let slack = read_slack_access(
            &json!({ "ok": true, "access_token": "xoxb-1", "team": { "name": "Acme" } }),
            "https://slack.com".to_string(),
        )
        .unwrap();
        assert_eq!(slack.team_name, "Acme");
        assert!(
            read_slack_access(
                &json!({ "ok": false, "error": "invalid_code" }),
                String::new()
            )
            .is_err()
        );
        let channels = read_slack_channels(&json!({
            "ok": true,
            "channels": [{ "id": "C2", "name": "ops" }, { "id": "C1", "name": "alerts" }]
        }))
        .unwrap();
        assert_eq!(
            channels[0],
            SlackChannel { id: "C1".to_string(), name: "alerts".to_string() }
        );

        let discord = read_discord_token(&json!({
            "webhook": { "name": "Daroyan", "url": "https://discord.com/api/webhooks/1/abc" }
        }))
        .unwrap();
        assert_eq!(
            discord.config,
            IntegrationConfig::Discord {
                webhook_url: "https://discord.com/api/webhooks/1/abc".to_string()
            }
        );
        assert!(read_discord_token(&json!({ "error": "invalid_grant" })).is_err());
    }

    #[test]
    fn reads_a_start_command_from_a_group() {
        let start = read_telegram_start(&json!({
            "update_id": 7,
            "message": {
                "text": "/start@daroyan_bot abc123",
                "chat": { "id": -100200, "title": "Ops Group", "type": "supergroup" }
            }
        }))
        .unwrap();
        assert_eq!(start.state, "abc123");
        assert_eq!(start.chat_id, "-100200");
        assert_eq!(start.chat_name, "Ops Group");
        assert!(
            read_telegram_start(&json!({ "update_id": 8, "message": { "text": "hello", "chat": { "id": 1 } } }))
                .is_none()
        );
    }
}
