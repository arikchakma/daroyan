use chrono::{DateTime, Duration, Utc};
use daroyan_protocol::message::CommandAction;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use crate::agents::Agents;
use crate::alerts::engine::parse_rule;
use crate::alerts::integrations::IntegrationConfig;
use crate::alerts::notify::{
    ChatMessage, post_json, read_slack, record_action, update_slack_message,
};
use crate::auth::{Role, find_access};
use crate::commands::send_command;
use crate::config::Config;
use crate::db::Database;
use crate::db::entities::alerts::Model as Alert;
use crate::db::ids::{
    AlertId, AlertRuleId, CronId, IntegrationId, ServerId, UptimeCheckId,
};
use crate::db::queries::alert_mutes::NewAlertMute;
use crate::id::{Prefix, new_id};
use crate::incidents::{RuleFire, declare_incident};

pub const MUTE_HOUR: &str = "mute_hour";
pub const MUTE_DAY: &str = "mute_day";
pub const DECLARE: &str = "declare_incident";
pub const RESTART: &str = "restart_process";
const MAX_AGE_SECS: i64 = 300;

pub fn verify_signature(
    secret: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
    now: DateTime<Utc>,
) -> bool {
    let Ok(sent_at) = timestamp.parse::<i64>() else { return false };
    if (now.timestamp() - sent_at).abs() > MAX_AGE_SECS {
        return false;
    }
    let Some(expected) =
        signature.strip_prefix("v0=").and_then(|hex| hex::decode(hex).ok())
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(format!("v0:{timestamp}:").as_bytes());
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

#[derive(Debug, Deserialize)]
pub struct Interaction {
    #[serde(rename = "type")]
    pub kind: String,
    pub user: SlackUser,
    #[serde(default)]
    pub channel: Option<SlackChannel>,
    #[serde(default)]
    pub message: Option<SlackMessage>,
    #[serde(default)]
    pub actions: Vec<SlackAction>,
    #[serde(default)]
    pub response_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SlackUser {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackChannel {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackMessage {
    pub ts: String,
}

#[derive(Debug, Deserialize)]
pub struct SlackAction {
    pub action_id: String,
    #[serde(default)]
    pub value: Option<String>,
}

pub struct ActionContext<'a> {
    pub database: &'a Database,
    pub agents: &'a Agents,
    pub config: &'a Config,
}

struct Clicked<'a> {
    action_id: &'a str,
    alert: Alert,
    slack_user: &'a str,
    email: String,
}

impl ActionContext<'_> {
    pub async fn handle(&self, interaction: Interaction) {
        let Some(action) = interaction.actions.first() else { return };
        if interaction.kind != "block_actions"
            || ![MUTE_HOUR, MUTE_DAY, DECLARE, RESTART]
                .contains(&action.action_id.as_str())
        {
            return;
        }
        let (Some(channel), Some(message)) = (&interaction.channel, &interaction.message)
        else {
            return;
        };
        let outcome = self.run(&interaction, action, &channel.id, &message.ts).await;
        if let Err(reason) = outcome {
            tracing::info!(action = %action.action_id, reason, "refused a slack action");
            if let Some(url) = &interaction.response_url {
                let reply = json!({ "response_type": "ephemeral", "replace_original": false, "text": reason });
                if let Err(error) = post_json(url, &reply).await {
                    tracing::warn!(error, "reply to a slack action");
                }
            }
        }
    }

    async fn run(
        &self,
        interaction: &Interaction,
        action: &SlackAction,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), String> {
        let database = self.database;
        let delivery = database
            .find_delivery_by_message(channel_id, message_id)
            .await
            .map_err(|_| busy())?
            .ok_or("Daroyan does not know this message.")?;
        let integration = database
            .find_integration(&IntegrationId::from(delivery.integration_id.as_str()))
            .await
            .map_err(|_| busy())?
            .ok_or("The Slack integration for this message was removed.")?;
        let Ok(IntegrationConfig::Slack { api_url, bot_token, .. }) =
            serde_json::from_value::<IntegrationConfig>(integration.config)
        else {
            return Err(
                "The Slack integration for this message is unreadable.".to_string()
            );
        };
        let alert_id = action.value.as_deref().unwrap_or_default();
        let alert = database
            .find_alert(&AlertId::from(alert_id))
            .await
            .map_err(|_| busy())?
            .ok_or("This alert no longer exists.")?;
        if alert.status != "firing" {
            return Err("This alert is no longer firing.".to_string());
        }
        let profile = read_slack(
            &api_url,
            &bot_token,
            "users.info",
            &[("user", &interaction.user.id)],
        )
        .await
        .map_err(|_| read_email_error())?;
        let email = profile["user"]["profile"]["email"]
            .as_str()
            .ok_or_else(read_email_error)?
            .to_lowercase();
        let access = find_access(database, self.config, &email)
            .await
            .map_err(|_| busy())?
            .ok_or_else(|| {
                format!(
                    "{email} is not a member of this Daroyan. Ask an admin to invite you."
                )
            })?;
        let needs_admin = action.action_id != DECLARE;
        if needs_admin && access.role != Role::Admin {
            return Err("Only Daroyan admins can do this. Ask an admin.".to_string());
        }
        let clicked = Clicked {
            action_id: &action.action_id,
            alert,
            slack_user: &interaction.user.id,
            email,
        };
        let note = self.apply(&clicked).await?;
        let Some(payload) = delivery
            .chat_payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        else {
            return Ok(());
        };
        let body = record_action(&payload, clicked.action_id, &note);
        let message = ChatMessage {
            channel_id: channel_id.to_string(),
            message_id: message_id.to_string(),
            payload: None,
        };
        update_slack_message(&api_url, &bot_token, &message, &body).await?;
        database
            .update_chat_payload(&delivery.id, body.to_string())
            .await
            .map_err(|_| busy())?;
        Ok(())
    }

    async fn apply(&self, clicked: &Clicked<'_>) -> Result<String, String> {
        let database = self.database;
        let alert = &clicked.alert;
        let rule = database
            .find_alert_rule(&AlertRuleId::from(alert.rule_id.as_str()))
            .await
            .map_err(|_| busy())?
            .ok_or("The alert rule was deleted.")?;
        let who = format!("<@{}>", clicked.slack_user);
        match clicked.action_id {
            MUTE_HOUR | MUTE_DAY => {
                let user = database
                    .list_users()
                    .await
                    .map_err(|_| busy())?
                    .into_iter()
                    .find(|user| user.email.eq_ignore_ascii_case(&clicked.email))
                    .ok_or_else(|| {
                        format!(
                            "Sign in to Daroyan once as {}, then try again.",
                            clicked.email
                        )
                    })?;
                let (length, label) = if clicked.action_id == MUTE_DAY {
                    (Duration::days(1), "1 day")
                } else {
                    (Duration::hours(1), "1 hour")
                };
                database
                    .create_alert_mute(NewAlertMute {
                        id: new_id(Prefix::AlertMute),
                        rule_id: rule.id.clone(),
                        server_id: alert.server_id.clone(),
                        check_id: alert.check_id.clone(),
                        cron_id: alert.cron_id.clone(),
                        ends_at: Some(Utc::now() + length),
                        reason: Some("Muted from Slack".to_string()),
                        creator_id: user.id,
                    })
                    .await
                    .map_err(|_| busy())?;
                Ok(format!("🔕 {who} muted this for {label}."))
            }
            DECLARE => {
                let (target_kind, target_id, target_name) =
                    self.find_target(alert).await?;
                let declared_by = database
                    .list_users()
                    .await
                    .map_err(|_| busy())?
                    .into_iter()
                    .find(|user| user.email.eq_ignore_ascii_case(&clicked.email))
                    .map_or_else(|| clicked.email.clone(), |user| user.name);
                let integration_ids = parse_rule(&rule)
                    .map(|parsed| parsed.integration_ids)
                    .unwrap_or_default();
                let incident = declare_incident(
                    database,
                    self.config.app_url.as_deref(),
                    RuleFire {
                        rule: &rule,
                        target_kind,
                        target_id: &target_id,
                        target_name: &target_name,
                        integration_ids: &integration_ids,
                    },
                    Some(&declared_by),
                )
                .await
                .map_err(|_| busy())?;
                Ok(match &self.config.app_url {
                    Some(app_url) => format!(
                        "🚨 {who} declared an incident: <{}/incidents/{}|{}>",
                        app_url.trim_end_matches('/'),
                        incident.id,
                        incident.title
                    ),
                    None => format!("🚨 {who} declared an incident: {}", incident.title),
                })
            }
            _ => {
                let server_id = alert
                    .server_id
                    .as_deref()
                    .ok_or("Only processes can be restarted.")?;
                let server = database
                    .find_server(&ServerId::from(server_id))
                    .await
                    .map_err(|_| busy())?
                    .ok_or("The server was removed.")?;
                send_command(
                    database,
                    self.agents,
                    &server,
                    CommandAction::RestartProcess { name: alert.subject.clone() },
                )
                .await
                .map_err(|error| format!("The restart failed: {error}"))?;
                Ok(format!("🔄 {who} asked the agent to restart {}.", alert.subject))
            }
        }
    }

    async fn find_target(
        &self,
        alert: &Alert,
    ) -> Result<(&'static str, String, String), String> {
        let database = self.database;
        if let Some(id) = &alert.server_id {
            let server = database
                .find_server(&ServerId::from(id.as_str()))
                .await
                .map_err(|_| busy())?;
            return Ok((
                "server",
                id.clone(),
                server.map_or_else(|| id.clone(), |server| server.name),
            ));
        }
        if let Some(id) = &alert.check_id {
            let check = database
                .find_uptime_check(&UptimeCheckId::from(id.as_str()))
                .await
                .map_err(|_| busy())?;
            return Ok((
                "check",
                id.clone(),
                check.map_or_else(|| id.clone(), |check| check.name),
            ));
        }
        if let Some(id) = &alert.cron_id {
            let cron = database
                .find_cron(&CronId::from(id.as_str()))
                .await
                .map_err(|_| busy())?;
            return Ok((
                "cron",
                id.clone(),
                cron.map_or_else(|| id.clone(), |cron| cron.name),
            ));
        }
        Err("This alert has no target.".to_string())
    }
}

fn busy() -> String {
    "Daroyan could not finish this. Try again in a moment.".to_string()
}

fn read_email_error() -> String {
    "Daroyan could not read your Slack email. Reconnect Slack in Settings → Integrations so Daroyan can read emails, then try again.".to_string()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sign(secret: &str, timestamp: &str, body: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("v0:{timestamp}:{body}").as_bytes());
        format!("v0={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_slack_signatures() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 9, 0, 0).unwrap();
        let timestamp = now.timestamp().to_string();
        let body = "payload=%7B%22type%22%3A%22block_actions%22%7D";
        let signature = sign("secret", &timestamp, body);
        assert!(verify_signature("secret", &timestamp, body.as_bytes(), &signature, now));
        assert!(!verify_signature("other", &timestamp, body.as_bytes(), &signature, now));
        assert!(!verify_signature(
            "secret",
            &timestamp,
            b"payload=changed",
            &signature,
            now
        ));
        assert!(!verify_signature("secret", &timestamp, body.as_bytes(), "v0=zz", now));
        assert!(!verify_signature("secret", &timestamp, body.as_bytes(), "abc", now));
        let late = now + Duration::seconds(MAX_AGE_SECS + 1);
        assert!(!verify_signature(
            "secret",
            &timestamp,
            body.as_bytes(),
            &signature,
            late
        ));
    }

    #[test]
    fn reads_a_block_action() {
        let interaction: Interaction = serde_json::from_str(
            r#"{"type":"block_actions","user":{"id":"U1","username":"arik"},"channel":{"id":"C1"},"message":{"ts":"1.2"},"actions":[{"action_id":"mute_hour","value":"alt_1","type":"button"}],"response_url":"https://hooks.slack.test/1"}"#,
        )
        .unwrap();
        assert_eq!(interaction.actions[0].action_id, MUTE_HOUR);
        assert_eq!(interaction.actions[0].value.as_deref(), Some("alt_1"));
        assert_eq!(interaction.message.unwrap().ts, "1.2");
    }
}
