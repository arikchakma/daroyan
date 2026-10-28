use daroyan_protocol::log::{
    LogFormat, LogLine, LogStream, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_INFO,
    SEVERITY_WARN,
};
use serde_json::{Map, Value};

const SEVERITY_FATAL: u8 = 21;
const LEVEL_KEYS: [&str; 5] = ["level", "severity", "lvl", "levelname", "log.level"];
const MESSAGE_KEYS: [&str; 3] = ["msg", "message", "MESSAGE"];
const TRACE_KEYS: [&str; 3] = ["trace_id", "traceId", "trace.id"];
const SPAN_KEYS: [&str; 3] = ["span_id", "spanId", "span.id"];

pub struct RawLine {
    pub at: u64,
    pub stream: LogStream,
    pub text: String,
    pub severity: Option<u8>,
}

pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            out.push(character);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    out
}

fn read_word_severity(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "fatal" | "panic" | "crit" | "critical" | "emerg" | "alert" => {
            Some(SEVERITY_FATAL)
        }
        "error" | "err" => Some(SEVERITY_ERROR),
        "warn" | "warning" => Some(SEVERITY_WARN),
        "info" | "notice" => Some(SEVERITY_INFO),
        "debug" | "trace" => Some(SEVERITY_DEBUG),
        _ => None,
    }
}

fn read_value_severity(value: &Value) -> Option<u8> {
    match value {
        Value::String(word) => read_word_severity(word),
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

fn find_prefix_severity(text: &str) -> Option<u8> {
    let head: String = text.chars().take(48).collect();
    head.split(|character: char| !character.is_ascii_alphabetic())
        .find_map(read_word_severity)
}

fn take_string(map: &mut Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match map.remove(*key)? {
        Value::String(text) => Some(text),
        other => Some(other.to_string()),
    })
}

pub fn to_log_line(service: &str, format: LogFormat, raw: RawLine) -> LogLine {
    let text = strip_ansi(raw.text.trim_end());
    let fallback =
        if raw.stream == LogStream::Stderr { SEVERITY_WARN } else { SEVERITY_INFO };
    if format != LogFormat::Plain
        && let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(text.trim())
    {
        let severity = LEVEL_KEYS
            .iter()
            .find_map(|key| map.remove(*key).as_ref().and_then(read_value_severity))
            .or(raw.severity)
            .unwrap_or(fallback);
        let message =
            take_string(&mut map, &MESSAGE_KEYS).unwrap_or_else(|| text.clone());
        return LogLine {
            at: raw.at,
            service: service.to_string(),
            stream: raw.stream,
            severity,
            message,
            trace_id: take_string(&mut map, &TRACE_KEYS),
            span_id: take_string(&mut map, &SPAN_KEYS),
            attributes: map,
            raw: Some(text),
            format,
        };
    }
    LogLine {
        at: raw.at,
        service: service.to_string(),
        stream: raw.stream,
        severity: raw
            .severity
            .or_else(|| find_prefix_severity(&text))
            .unwrap_or(fallback),
        message: text,
        attributes: Map::default(),
        trace_id: None,
        span_id: None,
        raw: None,
        format,
    }
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::{LogFormat, LogStream};

    use super::{RawLine, strip_ansi, to_log_line};

    fn parse(stream: LogStream, text: &str) -> daroyan_protocol::log::LogLine {
        to_log_line(
            "api",
            LogFormat::Auto,
            RawLine {
                at: 1,
                stream,
                text: text.to_string(),
                severity: None,
            },
        )
    }

    #[test]
    fn reads_json_lines() {
        let line = parse(
            LogStream::Stdout,
            r#"{"level":"error","msg":"SES failed","attempt":2,"trace_id":"abc"}"#,
        );
        assert_eq!(line.severity, 17);
        assert_eq!(line.message, "SES failed");
        assert_eq!(line.trace_id.as_deref(), Some("abc"));
        assert_eq!(line.attributes["attempt"], 2);
        assert!(!line.attributes.contains_key("level"));
        assert_eq!(parse(LogStream::Stdout, r#"{"level":40,"msg":"slow"}"#).severity, 13);
    }

    #[test]
    fn reads_level_words_and_streams() {
        assert_eq!(parse(LogStream::Stdout, "2026-10-16 ERROR db down").severity, 17);
        assert_eq!(parse(LogStream::Stdout, "[WARN] retrying").severity, 13);
        assert_eq!(parse(LogStream::Stdout, "GET /health 200").severity, 9);
        assert_eq!(parse(LogStream::Stderr, "something odd").severity, 13);
        assert_eq!(
            parse(LogStream::Stdout, "\u{1b}[31mERROR\u{1b}[0m boom").message,
            "ERROR boom"
        );
        assert_eq!(strip_ansi("\u{1b}[2mhi\u{1b}[0m"), "hi");
    }
}
