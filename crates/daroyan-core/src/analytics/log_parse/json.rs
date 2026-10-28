use chrono::DateTime;
use serde_json::{Map, Value};

use super::http::raise_for_status;
use super::{
    ParsedLine, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_FATAL, SEVERITY_INFO,
    SEVERITY_WARN, read_level_word,
};

pub(super) const LEVEL_KEYS: [&str; 5] =
    ["level", "lvl", "severity", "levelname", "log.level"];

pub(super) const MESSAGE_KEYS: [&str; 2] = ["msg", "message"];

pub(super) const TIME_KEYS: [&str; 4] = ["time", "ts", "timestamp", "@timestamp"];

const BODY_KEYS: [&str; 4] = ["body", "event", "log", "@message"];

const LEVEL_TEXT_KEYS: [&str; 3] = ["severity_text", "severityText", "@level"];

const LEVEL_NUMBER_KEYS: [&str; 2] = ["severity_number", "severityNumber"];

pub(super) fn read_level_value(value: &Value) -> Option<u8> {
    match value {
        Value::String(word) => read_level_word(word),
        Value::Number(number) => match number.as_u64()? {
            0..=20 => Some(SEVERITY_DEBUG),
            21..=30 => Some(SEVERITY_INFO),
            31..=40 => Some(SEVERITY_WARN),
            41..=50 => Some(SEVERITY_ERROR),
            _ => Some(SEVERITY_FATAL),
        },
        _ => None,
    }
}

pub(super) fn read_time_value(value: &Value) -> Option<i64> {
    match value {
        Value::String(text)
            if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            read_time_value(&Value::from(text.parse::<i64>().ok()?))
        }
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|time| time.timestamp_millis()),
        Value::Object(object) => object.get("$date").and_then(read_time_value),
        Value::Number(number) => match number.as_i64() {
            Some(raw) if raw > 100_000_000_000_000_000 => Some(raw / 1_000_000),
            Some(raw) if raw > 100_000_000_000_000 => Some(raw / 1_000),
            Some(raw) if raw > 100_000_000_000 => Some(raw),
            Some(raw) => raw.checked_mul(1000),
            None => {
                let seconds = number
                    .as_f64()
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)?;
                Some((seconds * 1000.0).round() as i64)
            }
        },
        _ => None,
    }
}

pub(super) fn read_fields(mut fields: Map<String, Value>, fallback: &str) -> ParsedLine {
    let severity = LEVEL_KEYS
        .iter()
        .find_map(|key| fields.remove(*key).as_ref().and_then(read_level_value));
    let at = TIME_KEYS
        .iter()
        .find_map(|key| fields.remove(*key).as_ref().and_then(read_time_value))
        .or_else(|| {
            fields
                .get("t")
                .filter(|value| value.is_object())
                .and_then(read_time_value)
        });
    let mut severity = severity;
    let mut message = MESSAGE_KEYS
        .iter()
        .find_map(|key| match fields.remove(*key)? {
            Value::String(text) => Some(text),
            other => Some(other.to_string()),
        })
        .unwrap_or_else(|| fallback.to_string());
    normalize_json_line(&mut fields, &mut message, &mut severity);
    ParsedLine { at, severity, message, attributes: fields }
}

pub(super) fn split_logfmt(text: &str) -> Option<Map<String, Value>> {
    let mut fields = Map::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        if key.is_empty()
            || !key.chars().all(|character| {
                character.is_alphanumeric() || matches!(character, '_' | '.' | '-')
            })
        {
            return None;
        }
        let (value, remainder) = match after.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"')?;
                (quoted[..end].to_string(), &quoted[end + 1..])
            }
            None => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                (after[..end].to_string(), &after[end..])
            }
        };
        if !remainder.is_empty() && !remainder.starts_with(char::is_whitespace) {
            return None;
        }
        fields.insert(key.to_string(), Value::from(value));
        rest = remainder.trim_start();
    }
    (fields.len() >= 2).then_some(fields)
}

pub(super) const GENERIC_MESSAGES: [&str; 5] =
    ["handled request", "request completed", "request finished", "request", "access"];

pub(super) fn find_path<'a>(
    attributes: &'a Map<String, Value>,
    path: &str,
) -> Option<&'a Value> {
    if let Some(value) = attributes.get(path) {
        return Some(value);
    }
    let mut parts = path.split('.');
    let mut value = attributes.get(parts.next()?)?;
    for part in parts {
        value = value.get(part)?;
    }
    Some(value)
}

pub(super) fn find_first(
    attributes: &Map<String, Value>,
    paths: &[&str],
) -> Option<Value> {
    paths.iter().find_map(|path| match find_path(attributes, path)? {
        Value::Array(items) => items.first().cloned(),
        value => Some(value.clone()),
    })
}

pub(super) fn read_status(value: &Value) -> Option<u16> {
    match value {
        Value::Number(number) => {
            number.as_u64().and_then(|status| u16::try_from(status).ok())
        }
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
    .filter(|status| (100..600).contains(status))
}

pub fn normalize_json_line(
    attributes: &mut Map<String, Value>,
    message: &mut String,
    severity: &mut Option<u8>,
) {
    read_body(attributes, message);
    read_extra_level(attributes, severity);
    read_mongodb(attributes, severity);
    let aliases: [(&str, &[&str]); 6] = [
        (
            "http.method",
            &["method", "httpMethod", "request.method", "req.method", "RequestMethod"],
        ),
        ("url.path", &["path", "url", "request.uri", "req.url", "RequestPath"]),
        ("http.status", &["status", "statusCode", "res.statusCode", "DownstreamStatus"]),
        ("duration_ms", &["durationMs", "responseTime"]),
        (
            "client.ip",
            &[
                "request.client_ip",
                "request.remote_ip",
                "req.remoteAddress",
                "ClientHost",
            ],
        ),
        (
            "user_agent.original",
            &["request.headers.User-Agent", "req.headers.user-agent"],
        ),
    ];
    for (target, paths) in aliases {
        if attributes.contains_key(target) {
            continue;
        }
        if let Some(value) = find_first(attributes, paths) {
            attributes.insert(target.to_string(), value);
        }
    }
    let is_caddy_access = attributes
        .get("logger")
        .and_then(Value::as_str)
        .is_some_and(|logger| logger.starts_with("http.log.access"));
    if is_caddy_access
        && !attributes.contains_key("duration_ms")
        && let Some(seconds) = attributes.get("duration").and_then(Value::as_f64)
    {
        attributes.insert(
            "duration_ms".into(),
            Value::from((seconds * 1_000_000.0).round() / 1000.0),
        );
    }
    if attributes.contains_key("RequestMethod")
        && !attributes.contains_key("duration_ms")
        && let Some(nanos) = attributes.get("Duration").and_then(Value::as_f64)
    {
        attributes
            .insert("duration_ms".into(), Value::from((nanos / 1000.0).round() / 1000.0));
    }
    if let Some(Value::String(url)) = attributes.get("url.path").cloned()
        && let Some((path, query)) = url.split_once('?')
    {
        attributes.insert("url.path".into(), Value::from(path));
        attributes.entry("url.query").or_insert_with(|| Value::from(query));
    }

    let status = attributes.get("http.status").and_then(read_status);
    if let Some(status) = status {
        raise_for_status(severity, status);
    }
    let method = attributes.get("http.method").and_then(Value::as_str);
    let path = attributes.get("url.path").and_then(Value::as_str);
    if let (Some(method), Some(path), Some(status)) = (method, path, status)
        && is_generic_message(message)
    {
        *message = format!("{method} {path} {status}");
    }
}

fn is_generic_message(message: &str) -> bool {
    let message = message.trim();
    message.is_empty()
        || message.starts_with('{')
        || GENERIC_MESSAGES.contains(&message.to_lowercase().as_str())
}

fn read_body(attributes: &mut Map<String, Value>, message: &mut String) {
    let trimmed = message.trim();
    if !trimmed.is_empty() && !trimmed.starts_with('{') {
        return;
    }
    let body = BODY_KEYS.iter().find_map(|key| match attributes.get(*key)? {
        Value::String(text) => Some((*key, text.trim_end().to_string())),
        _ => None,
    });
    if let Some((key, body)) = body {
        attributes.remove(key);
        *message = body;
    }
}

fn read_extra_level(attributes: &mut Map<String, Value>, severity: &mut Option<u8>) {
    let number = LEVEL_NUMBER_KEYS.iter().find_map(|key| {
        let value = attributes
            .get(*key)?
            .as_u64()
            .filter(|value| (1..=24).contains(value))?;
        attributes.remove(*key);
        u8::try_from(value).ok()
    });
    let text = LEVEL_TEXT_KEYS.iter().find_map(|key| {
        let level = read_level_value(attributes.get(*key)?)?;
        attributes.remove(*key);
        Some(level)
    });
    let nested = find_path(attributes, "log.level").and_then(read_level_value);
    if let Some(level) = number.or(text).or(nested) {
        *severity = Some(level);
    }
}

fn read_mongodb(attributes: &mut Map<String, Value>, severity: &mut Option<u8>) {
    let is_mongodb = attributes.get("t").and_then(|time| time.get("$date")).is_some();
    let Some(letter) = attributes.get("s").and_then(Value::as_str).filter(|_| is_mongodb)
    else {
        return;
    };
    *severity = match letter {
        "F" => Some(SEVERITY_FATAL),
        "E" => Some(SEVERITY_ERROR),
        "W" => Some(SEVERITY_WARN),
        "I" => Some(SEVERITY_INFO),
        _ if letter.starts_with('D') => Some(SEVERITY_DEBUG),
        _ => *severity,
    };
    attributes.remove("s");
    attributes.remove("t");
    attributes.insert("db.system".into(), Value::from("mongodb"));
    if let Some(component) = attributes.remove("c") {
        attributes.insert("mongodb.component".into(), component);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::normalize_json_line;
    use crate::analytics::log_parse::tests::parse;

    fn normalize(
        value: Value,
        message: &str,
    ) -> (Map<String, Value>, String, Option<u8>) {
        let mut attributes = value.as_object().cloned().unwrap();
        let mut message = message.to_string();
        let mut severity = Some(9);
        normalize_json_line(&mut attributes, &mut message, &mut severity);
        (attributes, message, severity)
    }

    #[test]
    fn reads_opentelemetry_and_ecs_json() {
        let otel = parse(
            r#"{"timeUnixNano":"1793152202123000000","severity_text":"ERROR","severity_number":17,"body":"charge failed"}"#,
        );
        assert_eq!(otel.message, "charge failed");
        assert_eq!(otel.severity, Some(17));
        let (_, message, severity) = normalize(
            json!({ "severityText": "WARN", "body": "slow" }),
            r#"{"severityText":"WARN"}"#,
        );
        assert_eq!((message.as_str(), severity), ("slow", Some(13)));

        let ecs = parse(
            r#"{"@timestamp":"2026-10-28T01:50:02.123Z","log.level":"error","message":"payment failed","ecs.version":"1.6.0"}"#,
        );
        assert_eq!(ecs.message, "payment failed");
        assert_eq!(ecs.severity, Some(17));
        assert_eq!(ecs.at, Some(1_793_152_202_123));
        let (_, _, severity) =
            normalize(json!({ "log": { "level": "warn" } }), "disk almost full");
        assert_eq!(severity, Some(13));
    }

    #[test]
    fn reads_structlog_and_docker_json() {
        let structlog = parse(
            r#"{"event":"job failed","level":"error","timestamp":"2026-10-28T01:50:02Z"}"#,
        );
        assert_eq!(structlog.message, "job failed");
        assert_eq!(structlog.severity, Some(17));
        let docker = parse(
            r#"{"log":"server started\n","stream":"stdout","time":"2026-10-28T01:50:02.123Z"}"#,
        );
        assert_eq!(docker.message, "server started");
        assert_eq!(docker.attributes["stream"], "stdout");
        let (attributes, message, _) =
            normalize(json!({ "event": "signup", "user": 1 }), "user signed up");
        assert_eq!(message, "user signed up");
        assert_eq!(attributes["event"], "signup");
    }

    #[test]
    fn reads_mongodb_json() {
        let mongo = parse(
            r#"{"t":{"$date":"2026-10-28T01:50:02.123+00:00"},"s":"E","c":"NETWORK","id":22988,"ctx":"conn12","msg":"Error receiving request from client"}"#,
        );
        assert_eq!(mongo.severity, Some(17));
        assert_eq!(mongo.at, Some(1_793_152_202_123));
        assert_eq!(mongo.message, "Error receiving request from client");
        assert_eq!(mongo.attributes["mongodb.component"], "NETWORK");
        assert_eq!(mongo.attributes["db.system"], "mongodb");
        assert!(!mongo.attributes.contains_key("s"));
    }

    #[test]
    fn reads_traefik_access_json() {
        let traefik = parse(
            r#"{"level":"info","time":"2026-10-28T01:50:02Z","msg":"","RequestMethod":"GET","RequestPath":"/api/users?page=2","DownstreamStatus":502,"Duration":12345678,"ClientHost":"10.0.0.1"}"#,
        );
        assert_eq!(traefik.message, "GET /api/users 502");
        assert_eq!(traefik.severity, Some(17));
        assert_eq!(traefik.attributes["url.query"], "page=2");
        assert_eq!(traefik.attributes["duration_ms"], 12.346);
        assert_eq!(traefik.attributes["client.ip"], "10.0.0.1");
    }
}
