use reqwest::Url;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{Error, ErrorName, Result};

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntegrationConfig {
    Slack { api_url: String, bot_token: String, channel_id: String, channel_name: String },
    Discord { webhook_url: String },
    Telegram { bot_token: String, chat_id: String },
    Webhook { url: String },
}

impl IntegrationConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Slack { .. } => "slack",
            Self::Discord { .. } => "discord",
            Self::Telegram { .. } => "telegram",
            Self::Webhook { .. } => "webhook",
        }
    }

    pub fn describe_target(&self) -> String {
        match self {
            Self::Slack { channel_name, .. } => format!("#{channel_name}"),
            Self::Discord { webhook_url } => mask_url(webhook_url),
            Self::Telegram { chat_id, .. } => format!("Chat {chat_id}"),
            Self::Webhook { url } => mask_url(url),
        }
    }
}

pub fn validate_integration(config: &IntegrationConfig) -> Result<()> {
    let invalid = |message: &str| Err(Error::new(ErrorName::ValidationError, message));
    match config {
        IntegrationConfig::Slack { api_url, bot_token, channel_id, .. } => {
            if !bot_token.starts_with("xoxb-") || channel_id.trim().is_empty() {
                return invalid("Connect Slack by signing in");
            }
            if !Url::parse(api_url)
                .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                return invalid("The Slack API URL must be http or https");
            }
        }
        IntegrationConfig::Discord { webhook_url } => {
            if !is_url_on(
                webhook_url,
                &["discord.com", "discordapp.com"],
                "/api/webhooks/",
            ) {
                return invalid(
                    "Use a Discord webhook URL, https://discord.com/api/webhooks/…",
                );
            }
        }
        IntegrationConfig::Telegram { bot_token, chat_id } => {
            let is_token = bot_token.split_once(':').is_some_and(|(bot_id, secret)| {
                !bot_id.is_empty()
                    && bot_id.bytes().all(|byte| byte.is_ascii_digit())
                    && secret.len() >= 20
                    && secret.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
                    })
            });
            if !is_token {
                return invalid("Use the bot token from BotFather, like 123456:ABC-DEF…");
            }
            if chat_id.trim().is_empty() {
                return invalid("Add the chat ID");
            }
        }
        IntegrationConfig::Webhook { url } => {
            if !Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                return invalid("Use an http or https URL");
            }
        }
    }
    Ok(())
}

fn is_url_on(value: &str, hosts: &[&str], path: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some_and(|host| hosts.contains(&host))
            && url.path().starts_with(path)
    })
}

fn mask_url(value: &str) -> String {
    let Ok(url) = Url::parse(value) else {
        return "Invalid URL".to_string();
    };
    let host = url.host_str().unwrap_or_default();
    let path = url.path();
    let tail: String = path
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if path.len() <= 1 { host.to_string() } else { format!("{host}/…{tail}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_each_kind() {
        let slack = IntegrationConfig::Slack {
            api_url: "https://slack.com".to_string(),
            bot_token: "xoxb-1-abc".to_string(),
            channel_id: "C123".to_string(),
            channel_name: "ops".to_string(),
        };
        assert!(validate_integration(&slack).is_ok());
        assert_eq!(slack.describe_target(), "#ops");
        let not_slack = IntegrationConfig::Slack {
            api_url: "https://slack.com".to_string(),
            bot_token: "nope".to_string(),
            channel_id: "C123".to_string(),
            channel_name: "ops".to_string(),
        };
        assert!(validate_integration(&not_slack).is_err());
        let telegram = IntegrationConfig::Telegram {
            bot_token: "123456:ABCdefGHIjklMNOpqrSTUvwx".to_string(),
            chat_id: "-100200".to_string(),
        };
        assert!(validate_integration(&telegram).is_ok());
        assert_eq!(telegram.describe_target(), "Chat -100200");
        let bad_token = IntegrationConfig::Telegram {
            bot_token: "nope".to_string(),
            chat_id: "1".to_string(),
        };
        assert!(validate_integration(&bad_token).is_err());
        let webhook = IntegrationConfig::Webhook { url: "ftp://x".to_string() };
        assert!(validate_integration(&webhook).is_err());
    }
}
