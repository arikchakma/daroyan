use std::borrow::Cow;
use std::fmt;
use std::net::IpAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use crate::alerts::integrations::IntegrationConfig;
use crate::net::{ReachableResolver, blocked_error, is_reachable};

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const SLACK_MAX_BLOCKS: usize = 50;
const SLACK_MAX_TEXT: usize = 3000;
const SLACK_MAX_FIELD: usize = 2000;
const SLACK_MAX_FIELDS: usize = 10;
const SLACK_MAX_BUTTON: usize = 75;
const DISCORD_MAX_TITLE: usize = 256;
const DISCORD_MAX_DESCRIPTION: usize = 4096;
const DISCORD_MAX_FIELD: usize = 1024;
const DISCORD_MAX_FIELDS: usize = 25;
const TELEGRAM_MAX_SNIPPET: usize = 2500;
const MAX_LINE: usize = 300;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(ReachableResolver))
        .build()
        .unwrap_or_default()
});

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Critical,
    Warning,
    Resolved,
    #[default]
    Note,
}

impl Tone {
    fn color(self) -> u32 {
        match self {
            Self::Critical => 0x00D9_3025,
            Self::Warning => 0x00F5_B83D,
            Self::Resolved => 0x001F_8A4C,
            Self::Note => 0x008A_8A8A,
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            Self::Critical => "🔴 ",
            Self::Warning => "🟠 ",
            Self::Resolved => "✅ ",
            Self::Note => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    text: String,
    bold: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text(Vec<Span>);

impl Text {
    #[must_use]
    pub fn plain(mut self, text: impl Into<String>) -> Self {
        self.0.push(Span { text: text.into(), bold: false });
        self
    }

    #[must_use]
    pub fn bold(mut self, text: impl Into<String>) -> Self {
        self.0.push(Span { text: text.into(), bold: true });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|span| span.text.is_empty())
    }

    fn render(&self, escape: impl Fn(&str) -> String, open: &str, close: &str) -> String {
        self.0
            .iter()
            .map(|span| {
                if span.bold && !span.text.trim().is_empty() {
                    format!("{open}{}{close}", escape(&span.text))
                } else {
                    escape(&span.text)
                }
            })
            .collect()
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self::default().plain(text)
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self::default().plain(text)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|span| formatter.write_str(&span.text))
    }
}

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Field {
    pub label: String,
    pub value: String,
}

impl Field {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), value: value.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Snippet {
    pub label: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStyle {
    #[default]
    Default,
    Primary,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
    pub style: LinkStyle,
}

impl Link {
    pub fn new(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
            style: LinkStyle::Default,
        }
    }

    #[must_use]
    pub fn primary(self) -> Self {
        Self { style: LinkStyle::Primary, ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Details {
    pub summary: Text,
    pub fields: Vec<Field>,
    pub snippet: Option<Snippet>,
    pub note: Text,
    pub links: Vec<Link>,
    pub context: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub event: &'static str,
    pub tone: Tone,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub data: Value,
    pub details: Details,
}

impl Notice {
    pub fn summarize(&self) -> String {
        match self.body.lines().next() {
            Some(line) => format!("{} · {line}", self.title),
            None => self.title.clone(),
        }
    }

    fn summary(&self) -> Cow<'_, Text> {
        if self.details.summary.is_empty() {
            Cow::Owned(Text::from(self.body.as_str()))
        } else {
            Cow::Borrowed(&self.details.summary)
        }
    }

    fn links(&self) -> Cow<'_, [Link]> {
        if !self.details.links.is_empty() {
            return Cow::Borrowed(&self.details.links);
        }
        Cow::Owned(
            self.url
                .iter()
                .map(|url| Link::new("Open in Daroyan", url.as_str()).primary())
                .collect(),
        )
    }
}

pub async fn send_notice(
    config: &IntegrationConfig,
    notice: &Notice,
) -> Result<(), String> {
    let request = match config {
        IntegrationConfig::Slack { api_url, bot_token, channel_id, .. } => CLIENT
            .post(format!("{}/api/chat.postMessage", api_url.trim_end_matches('/')))
            .bearer_auth(bot_token)
            .json(&build_slack_message(channel_id, notice)),
        IntegrationConfig::Discord { webhook_url } => {
            CLIENT.post(webhook_url).json(&build_discord_message(notice))
        }
        IntegrationConfig::Telegram { bot_token, chat_id } => CLIENT
            .post(format!("https://api.telegram.org/bot{bot_token}/sendMessage"))
            .json(&build_telegram_message(chat_id, notice)),
        IntegrationConfig::Webhook { url } => CLIENT.post(url).json(&json!({
            "event": notice.event,
            "title": notice.title,
            "body": notice.body,
            "url": notice.url,
            "data": notice.data,
            "details": notice.details,
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

fn build_slack_message(channel_id: &str, notice: &Notice) -> Value {
    let section = |text: String| json!({ "type": "section", "text": { "type": "mrkdwn", "text": cut(&text, SLACK_MAX_TEXT) } });
    let details = &notice.details;
    let mut blocks = vec![section(format!(
        "*{}{}*",
        notice.tone.emoji(),
        escape_slack(&notice.title)
    ))];
    let summary = notice.summary();
    if !summary.is_empty() {
        blocks.push(section(summary.render(escape_slack, "*", "*")));
    }
    for fields in details.fields.chunks(SLACK_MAX_FIELDS) {
        let fields: Vec<Value> = fields
            .iter()
            .map(|field| {
                let text = format!(
                    "*{}*\n{}",
                    escape_slack(&field.label),
                    escape_slack(&field.value)
                );
                json!({ "type": "mrkdwn", "text": cut(&text, SLACK_MAX_FIELD) })
            })
            .collect();
        blocks.push(json!({ "type": "section", "fields": fields }));
    }
    if let Some(snippet) = &details.snippet {
        if let Some(label) = &snippet.label {
            blocks.push(section(format!("*{}*", escape_slack(label))));
        }
        let code = format_code(&snippet.lines, escape_slack, SLACK_MAX_TEXT - 6);
        blocks.push(section(format!("```{code}```")));
    }
    if !details.note.is_empty() {
        blocks.push(section(details.note.render(escape_slack, "*", "*")));
    }
    let buttons: Vec<Value> = notice
        .links()
        .iter()
        .enumerate()
        .map(|(index, link)| {
            let mut button = json!({
                "type": "button",
                "action_id": format!("link_{index}"),
                "text": { "type": "plain_text", "text": cut(&link.label, SLACK_MAX_BUTTON) },
                "url": link.url,
            });
            match link.style {
                LinkStyle::Primary => button["style"] = json!("primary"),
                LinkStyle::Danger => button["style"] = json!("danger"),
                LinkStyle::Default => {}
            }
            button
        })
        .collect();
    if !buttons.is_empty() {
        blocks.push(json!({ "type": "actions", "elements": buttons }));
    }
    if let Some(context) = &details.context {
        blocks.push(json!({
            "type": "context",
            "elements": [{ "type": "mrkdwn", "text": cut(&escape_slack(context), SLACK_MAX_TEXT) }],
        }));
    }
    blocks.truncate(SLACK_MAX_BLOCKS);
    // The blocks sit in an attachment for the colored bar. A top-level `text`
    // would show above it, so the notification text goes in `fallback`.
    json!({
        "channel": channel_id,
        "attachments": [{
            "color": format!("#{:06X}", notice.tone.color()),
            "fallback": format!("{}{}: {}", notice.tone.emoji(), notice.title, summary),
            "blocks": blocks,
        }],
    })
}

fn build_discord_message(notice: &Notice) -> Value {
    let details = &notice.details;
    let mut fields: Vec<Value> = details
        .fields
        .iter()
        .map(|field| {
            json!({
                "name": cut(&field.label, DISCORD_MAX_TITLE),
                "value": cut(&escape_markdown(&field.value), DISCORD_MAX_FIELD),
                "inline": true,
            })
        })
        .collect();
    if let Some(snippet) = &details.snippet {
        let code = format_code(&snippet.lines, str::to_string, DISCORD_MAX_FIELD - 6);
        fields.push(json!({
            "name": snippet.label.as_deref().map_or("\u{200b}".to_string(), |label| cut(label, DISCORD_MAX_TITLE)),
            "value": format!("```{code}```"),
            "inline": false,
        }));
    }
    let links = notice
        .links()
        .iter()
        .map(|link| format!("[{}]({})", escape_markdown(&link.label), link.url))
        .collect::<Vec<_>>()
        .join(" · ");
    let after = [details.note.render(escape_markdown, "**", "**"), links]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !after.is_empty() {
        fields.push(json!({
            "name": "\u{200b}",
            "value": cut(&after, DISCORD_MAX_FIELD),
            "inline": false,
        }));
    }
    fields.truncate(DISCORD_MAX_FIELDS);
    let mut embed = json!({
        "title": cut(&format!("{}{}", notice.tone.emoji(), notice.title), DISCORD_MAX_TITLE),
        "description": cut(&notice.summary().render(escape_markdown, "**", "**"), DISCORD_MAX_DESCRIPTION),
        "color": notice.tone.color(),
        "fields": fields,
    });
    if let Some(context) = &details.context {
        embed["footer"] = json!({ "text": cut(context, DISCORD_MAX_DESCRIPTION) });
    }
    json!({ "embeds": [embed] })
}

fn build_telegram_message(chat_id: &str, notice: &Notice) -> Value {
    let details = &notice.details;
    let mut parts =
        vec![format!("<b>{}{}</b>", notice.tone.emoji(), escape_html(&notice.title))];
    let summary = notice.summary();
    if !summary.is_empty() {
        parts.push(summary.render(escape_html, "<b>", "</b>"));
    }
    if !details.fields.is_empty() {
        parts.push(
            details
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "<b>{}</b>: {}",
                        escape_html(&field.label),
                        escape_html(&field.value)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if let Some(snippet) = &details.snippet {
        let code = format_code(&snippet.lines, escape_html, TELEGRAM_MAX_SNIPPET);
        parts.push(match &snippet.label {
            Some(label) => format!("<b>{}</b>\n<pre>{code}</pre>", escape_html(label)),
            None => format!("<pre>{code}</pre>"),
        });
    }
    if !details.note.is_empty() {
        parts.push(details.note.render(escape_html, "<b>", "</b>"));
    }
    let links = notice
        .links()
        .iter()
        .map(|link| {
            format!(
                "<a href=\"{}\">{}</a>",
                escape_html(&link.url),
                escape_html(&link.label)
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if !links.is_empty() {
        parts.push(links);
    }
    if let Some(context) = &details.context {
        parts.push(format!("<i>{}</i>", escape_html(context)));
    }
    json!({
        "chat_id": chat_id,
        "text": parts.join("\n\n"),
        "parse_mode": "HTML",
        "disable_web_page_preview": true,
    })
}

fn format_code(lines: &[String], escape: impl Fn(&str) -> String, max: usize) -> String {
    let mut code = String::new();
    for line in lines {
        let line = escape(&cut(line, MAX_LINE)).replace("```", "``\u{200b}`");
        let needed = line.chars().count() + usize::from(!code.is_empty());
        if code.chars().count() + needed > max {
            break;
        }
        if !code.is_empty() {
            code.push('\n');
        }
        code.push_str(&line);
    }
    code
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
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
            if "\\*_~`|[]()<>#@".contains(character) {
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
            tone: Tone::Note,
            title: "Firing: High CPU".to_string(),
            body: "worker-1 · CPU: 96%\nCPU above 90% for 5m".to_string(),
            url: None,
            data: json!({}),
            details: Details::default(),
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

    fn make_notice(details: Details) -> Notice {
        Notice {
            event: "alert.firing",
            tone: Tone::Critical,
            title: "Critical: InboxJot error spike".to_string(),
            body: "Inboxjot Server · Logs: 11 lines".to_string(),
            url: Some("https://daroyan.test/servers/server_1".to_string()),
            data: json!({}),
            details,
        }
    }

    fn make_details() -> Details {
        Details {
            summary: Text::default().bold("11 error lines").plain(" in the last 5m."),
            fields: vec![
                Field::new("Server", "Inboxjot Server · production"),
                Field::new("Source", "<!channel>"),
            ],
            snippet: Some(Snippet {
                label: Some("Top errors".to_string()),
                lines: vec!["11× HTTP export failed with status code: 404".to_string()],
            }),
            note: Text::default(),
            links: vec![
                Link::new("Open logs", "https://daroyan.test/logs").primary(),
                Link::new("View alert", "https://daroyan.test/alerts/rule_1"),
            ],
            context: Some("Checked every 60 s".to_string()),
        }
    }

    #[test]
    fn builds_slack_blocks() {
        let message = build_slack_message("C1", &make_notice(make_details()));
        assert_eq!(message.get("text"), None);
        let attachment = &message["attachments"][0];
        assert_eq!(attachment["color"], "#D93025");
        assert_eq!(
            attachment["fallback"],
            "🔴 Critical: InboxJot error spike: 11 error lines in the last 5m."
        );
        let blocks = attachment["blocks"].as_array().unwrap();
        let types: Vec<&str> =
            blocks.iter().map(|block| block["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            ["section", "section", "section", "section", "section", "actions", "context"]
        );
        assert_eq!(blocks[0]["text"]["text"], "*🔴 Critical: InboxJot error spike*");
        assert_eq!(blocks[1]["text"]["text"], "*11 error lines* in the last 5m.");
        assert_eq!(blocks[2]["fields"][1]["text"], "*Source*\n&lt;!channel&gt;");
        assert_eq!(blocks[3]["text"]["text"], "*Top errors*");
        assert_eq!(
            blocks[4]["text"]["text"],
            "```11× HTTP export failed with status code: 404```"
        );
        assert_eq!(blocks[5]["elements"][0]["style"], "primary");
        assert_eq!(blocks[5]["elements"][1].get("style"), None);
        assert_eq!(blocks[5]["elements"][1]["url"], "https://daroyan.test/alerts/rule_1");
    }

    #[test]
    fn falls_back_to_the_body_and_url() {
        let message = build_slack_message("C1", &make_notice(Details::default()));
        let blocks = message["attachments"][0]["blocks"].as_array().unwrap();
        assert_eq!(blocks[1]["text"]["text"], "Inboxjot Server · Logs: 11 lines");
        assert_eq!(blocks[2]["elements"][0]["text"]["text"], "Open in Daroyan");
    }

    #[test]
    fn stays_inside_slack_limits() {
        let details = Details {
            summary: Text::from("x".repeat(5000)),
            fields: (0..120).map(|index| Field::new(format!("F{index}"), "v")).collect(),
            snippet: Some(Snippet {
                label: None,
                lines: (0..100).map(|_| format!("```{}", "y".repeat(1000))).collect(),
            }),
            ..make_details()
        };
        let message = build_slack_message("C1", &make_notice(details));
        let blocks = message["attachments"][0]["blocks"].as_array().unwrap();
        assert!(blocks.len() <= SLACK_MAX_BLOCKS);
        for block in blocks {
            if let Some(text) = block["text"]["text"].as_str() {
                assert!(text.chars().count() <= SLACK_MAX_TEXT);
            }
            for field in block["fields"].as_array().into_iter().flatten() {
                assert!(
                    field["text"].as_str().unwrap().chars().count() <= SLACK_MAX_FIELD
                );
            }
        }
        let code =
            format_code(&["a".repeat(1000), "```b".to_string()], str::to_string, 3000);
        assert_eq!(code.lines().next().unwrap().chars().count(), MAX_LINE);
        assert!(code.ends_with("``\u{200b}`b"));
    }

    #[test]
    fn builds_discord_and_telegram_messages() {
        let notice = make_notice(make_details());
        let discord = build_discord_message(&notice);
        let embed = &discord["embeds"][0];
        assert_eq!(embed["description"], "**11 error lines** in the last 5m.");
        assert_eq!(embed["fields"][1]["value"], "\\<!channel\\>");
        assert_eq!(embed["fields"][2]["name"], "Top errors");
        assert_eq!(
            embed["fields"][3]["value"],
            "[Open logs](https://daroyan.test/logs) · [View alert](https://daroyan.test/alerts/rule_1)"
        );
        assert_eq!(embed["footer"]["text"], "Checked every 60 s");
        let telegram = build_telegram_message("42", &notice);
        let text = telegram["text"].as_str().unwrap();
        assert!(text.starts_with(
            "<b>🔴 Critical: InboxJot error spike</b>\n\n<b>11 error lines</b>"
        ));
        assert!(text.contains("<b>Source</b>: &lt;!channel&gt;"));
        assert!(text.contains("<b>Top errors</b>\n<pre>11× HTTP export"));
        assert!(text.ends_with("<i>Checked every 60 s</i>"));
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
            tone: Tone::Note,
            title: "Test".to_string(),
            body: String::new(),
            url: None,
            data: json!({}),
            details: Details::default(),
        };
        let error = send_notice(&config, &notice).await.unwrap_err();
        assert!(error.starts_with("HTTP 500: xxx"));
    }
}
