use std::net::IpAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde_json::{Value, json};

use crate::alerts::integrations::IntegrationConfig;
use crate::net::{ReachableResolver, blocked_error, is_reachable};

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(ReachableResolver))
        .build()
        .unwrap_or_default()
});

#[derive(Clone, Debug)]
pub struct Notice {
    pub event: &'static str,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub data: Value,
}

impl Notice {
    pub fn summarize(&self) -> String {
        match self.body.lines().next() {
            Some(line) => format!("{} · {line}", self.title),
            None => self.title.clone(),
        }
    }

    fn get_color(&self) -> u32 {
        match self.event {
            "alert.firing" => 0x00D9_3025,
            "alert.resolved" => 0x001F_8A4C,
            "alert.limited" => 0x00F5_B83D,
            _ => 0x008A_8A8A,
        }
    }
}

pub async fn send_notice(
    config: &IntegrationConfig,
    notice: &Notice,
) -> Result<(), String> {
    let link = notice.url.as_deref();
    let request = match config {
        IntegrationConfig::Slack { api_url, bot_token, channel_id, .. } => {
            let link = link
                .map(|url| format!("\n<{url}|Open in Daroyan →>"))
                .unwrap_or_default();
            CLIENT
                .post(format!("{}/api/chat.postMessage", api_url.trim_end_matches('/')))
                .bearer_auth(bot_token)
                .json(&json!({
                    "channel": channel_id,
                    "text": escape_slack(&notice.title),
                    "attachments": [{
                        "color": format!("#{:06X}", notice.get_color()),
                        "text": format!(
                            "*{}*\n{}{link}",
                            escape_slack(&notice.title),
                            escape_slack(&notice.body)
                        ),
                        "mrkdwn_in": ["text"],
                    }],
                }))
        }
        IntegrationConfig::Discord { webhook_url } => {
            let link = link
                .map(|url| format!("\n[Open in Daroyan →]({url})"))
                .unwrap_or_default();
            CLIENT.post(webhook_url).json(&json!({
                "embeds": [{
                    "title": notice.title,
                    "description": format!("{}{link}", escape_markdown(&notice.body)),
                    "color": notice.get_color(),
                }],
            }))
        }
        IntegrationConfig::Telegram { bot_token, chat_id } => {
            let link = link
                .map(|url| {
                    format!("\n<a href=\"{}\">Open in Daroyan →</a>", escape_html(url))
                })
                .unwrap_or_default();
            CLIENT
                .post(format!("https://api.telegram.org/bot{bot_token}/sendMessage"))
                .json(&json!({
                    "chat_id": chat_id,
                    "text": format!("<b>{}</b>\n{}{link}", escape_html(&notice.title), escape_html(&notice.body)),
                    "parse_mode": "HTML",
                    "disable_web_page_preview": true,
                }))
        }
        IntegrationConfig::Webhook { url } => CLIENT.post(url).json(&json!({
            "event": notice.event,
            "title": notice.title,
            "body": notice.body,
            "url": notice.url,
            "data": notice.data,
        })),
    };
    let request = request.build().map_err(|error| describe_error(&error))?;
    if let Some(ip) = request
        .url()
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        && !is_reachable(ip)
    {
        return Err(blocked_error(&ip.to_string()));
    }
    let mut response = CLIENT
        .execute(request)
        .await
        .map_err(|error| describe_error(&error))?;
    let status = response.status();
    let mut bytes = Vec::new();
    while bytes.len() < MAX_RESPONSE_BYTES
        && let Ok(Some(chunk)) = response.chunk().await
    {
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&bytes);
    let answer = serde_json::from_str::<Value>(&body).ok();
    let refusal =
        answer
            .as_ref()
            .filter(|answer| answer["ok"] == false)
            .and_then(|answer| {
                answer["error"].as_str().or_else(|| answer["description"].as_str())
            });
    if status.is_success() && refusal.is_none() {
        return Ok(());
    }
    let detail = refusal
        .map_or_else(|| body.chars().take(200).collect::<String>(), str::to_string);
    Err(format!("HTTP {}: {}", status.as_u16(), detail.trim()))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_slack(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_markdown(text: &str) -> String {
    text.chars()
        .fold(String::with_capacity(text.len()), |mut escaped, character| {
            if matches!(
                character,
                '\\' | '*'
                    | '_'
                    | '~'
                    | '`'
                    | '|'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '<'
                    | '>'
                    | '#'
                    | '@'
            ) {
                escaped.push('\\');
            }
            escaped.push(character);
            escaped
        })
}

fn describe_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return format!("No response within {} seconds", TIMEOUT.as_secs());
    }
    let mut message = "The request failed".to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        message = inner.to_string();
        source = inner.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_and_escapes() {
        let notice = Notice {
            event: "alert.firing",
            title: "Firing: High CPU".to_string(),
            body: "worker-1 · CPU: 96%\nCPU above 90% for 5m".to_string(),
            url: None,
            data: json!({}),
        };
        assert_eq!(notice.summarize(), "Firing: High CPU · worker-1 · CPU: 96%");
        assert_eq!(escape_html("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }

    #[test]
    fn escapes_links_and_mentions() {
        assert_eq!(
            escape_slack("<!channel> <https://evil|Open> & co"),
            "&lt;!channel&gt; &lt;https://evil|Open&gt; &amp; co"
        );
        assert_eq!(
            escape_markdown("[Open](https://evil) @everyone"),
            "\\[Open\\]\\(https://evil\\) \\@everyone"
        );
    }

    #[tokio::test]
    async fn stops_reading_a_huge_reply() {
        use axum::body::Body;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        crate::net::allow_private_targets(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let endless = futures_util::stream::repeat_with(|| {
            Ok::<_, std::io::Error>(axum::body::Bytes::from(vec![b'x'; 8192]))
        });
        let app = axum::Router::new().fallback(move || async move {
            (StatusCode::INTERNAL_SERVER_ERROR, Body::from_stream(endless))
                .into_response()
        });
        tokio::spawn(async move { axum::serve(listener, app).await });
        let config = IntegrationConfig::Webhook { url: format!("http://{address}/hook") };
        let notice = Notice {
            event: "test",
            title: "Test".to_string(),
            body: String::new(),
            url: None,
            data: json!({}),
        };
        let error = send_notice(&config, &notice).await.unwrap_err();
        assert!(error.starts_with("HTTP 500: xxx"));
    }
}
