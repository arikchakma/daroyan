use chrono::{DateTime, NaiveDateTime};
use serde_json::{Map, Value};

const SEVERITY_DEBUG: u8 = 5;
const SEVERITY_INFO: u8 = 9;
const SEVERITY_WARN: u8 = 13;
const SEVERITY_ERROR: u8 = 17;
const SEVERITY_FATAL: u8 = 21;
const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Debug, Default, PartialEq)]
pub struct ParsedLine {
    pub at: Option<i64>,
    pub severity: Option<u8>,
    pub message: String,
    pub attributes: Map<String, Value>,
}

fn read_level_word(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "fatal" | "panic" | "crit" | "critical" | "emerg" | "alert" => {
            Some(SEVERITY_FATAL)
        }
        "error" | "err" => Some(SEVERITY_ERROR),
        "warn" | "warning" => Some(SEVERITY_WARN),
        "info" | "notice" | "log" | "statement" => Some(SEVERITY_INFO),
        "debug" | "trace" => Some(SEVERITY_DEBUG),
        _ => None,
    }
}

fn is_time(token: &str) -> bool {
    let clock = token
        .trim_end_matches(',')
        .split(['.', ','])
        .next()
        .unwrap_or_default();
    clock.len() == 8 && clock.as_bytes()[2] == b':' && clock.as_bytes()[5] == b':'
}

fn is_digits(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_digit())
}

fn rest_after(text: &str, count: usize) -> &str {
    let mut rest = text.trim_start();
    for _ in 0..count {
        rest = rest
            .find(char::is_whitespace)
            .map_or("", |end| rest[end..].trim_start());
    }
    rest
}

fn take_iso_timestamp(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(3).collect();
    let first = *tokens.first()?;
    if first.len() >= 19 && first.as_bytes().get(10) == Some(&b'T') {
        if let Ok(time) = DateTime::parse_from_rfc3339(first) {
            parsed.at = Some(time.timestamp_millis());
            return Some(1);
        }
        return NaiveDateTime::parse_from_str(first, "%Y-%m-%dT%H:%M:%S%.f")
            .ok()
            .map(|_| 1);
    }
    let second = *tokens.get(1)?;
    let is_date =
        first.len() == 10 && (first.as_bytes()[4] == b'-' || first.as_bytes()[4] == b'/');
    if !is_date || !is_time(second) {
        return None;
    }
    let joined = format!(
        "{} {}",
        first.replace('/', "-"),
        second.trim_end_matches(',').replace(',', ".")
    );
    let zone = tokens.get(2).copied().unwrap_or_default();
    if zone == "UTC" || zone == "Z" {
        if let Ok(time) = NaiveDateTime::parse_from_str(&joined, "%Y-%m-%d %H:%M:%S%.f") {
            parsed.at = Some(time.and_utc().timestamp_millis());
        }
        return Some(3);
    }
    let is_offset = zone.len() >= 5
        && (zone.starts_with('+') || zone.starts_with('-'))
        && zone[1..].bytes().all(|byte| byte.is_ascii_digit() || byte == b':');
    if is_offset {
        let with_zone = format!("{joined} {}", zone.replace(':', ""));
        if let Ok(time) = DateTime::parse_from_str(&with_zone, "%Y-%m-%d %H:%M:%S%.f %z")
        {
            parsed.at = Some(time.timestamp_millis());
        }
        return Some(3);
    }
    Some(2)
}

fn take_syslog_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(5).collect();
    if tokens.len() < 5
        || !MONTHS.contains(&tokens[0])
        || !is_digits(tokens[1])
        || !is_time(tokens[2])
    {
        return None;
    }
    let process = tokens[4].strip_suffix(':')?;
    read_process(process, parsed);
    Some(5)
}

fn read_process(token: &str, parsed: &mut ParsedLine) {
    match token.split_once('[') {
        Some((name, pid)) => {
            parsed.attributes.insert("process.name".into(), Value::from(name));
            if let Some(pid) =
                pid.strip_suffix(']').and_then(|pid| pid.parse::<u64>().ok())
            {
                parsed.attributes.insert("process.pid".into(), Value::from(pid));
            }
        }
        None => {
            parsed.attributes.insert("process.name".into(), Value::from(token));
        }
    }
}

fn take_redis_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(6).collect();
    let (pid, role) = tokens.first()?.split_once(':')?;
    if tokens.len() < 6
        || !is_digits(pid)
        || role.len() != 1
        || !MONTHS.contains(&tokens[2])
        || !is_time(tokens[4])
    {
        return None;
    }
    let severity = match tokens[5] {
        "#" => SEVERITY_WARN,
        "." => SEVERITY_DEBUG,
        "*" | "-" => SEVERITY_INFO,
        _ => return None,
    };
    parsed.severity = Some(severity);
    parsed
        .attributes
        .insert("process.pid".into(), Value::from(pid.parse::<u64>().ok()?));
    Some(6)
}

fn read_priority(pri: u64) -> u8 {
    match pri % 8 {
        0..=2 => SEVERITY_FATAL,
        3 => SEVERITY_ERROR,
        4 => SEVERITY_WARN,
        5 | 6 => SEVERITY_INFO,
        _ => SEVERITY_DEBUG,
    }
}

fn take_priority(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let end = text.strip_prefix('<')?.find('>')? + 1;
    let pri: u64 = text[1..end].parse().ok()?;
    parsed.severity = Some(read_priority(pri));
    Some(end + 1)
}

fn take_glog_header(text: &str, parsed: &mut ParsedLine) -> Option<usize> {
    let tokens: Vec<&str> = text.split_whitespace().take(4).collect();
    let first = *tokens.first()?;
    let severity = match first.chars().next()? {
        'I' => SEVERITY_INFO,
        'W' => SEVERITY_WARN,
        'E' => SEVERITY_ERROR,
        'F' => SEVERITY_FATAL,
        _ => return None,
    };
    if tokens.len() < 4
        || first.len() != 5
        || !is_digits(&first[1..])
        || !is_time(tokens[1])
        || !is_digits(tokens[2])
    {
        return None;
    }
    let location = tokens[3].strip_suffix(']')?;
    parsed.severity = Some(severity);
    parsed
        .attributes
        .insert("process.pid".into(), Value::from(tokens[2].parse::<u64>().ok()?));
    parsed
        .attributes
        .insert("code.location".into(), Value::from(location));
    Some(4)
}

fn take_prefix_token(text: &str, parsed: &mut ParsedLine, after_time: bool) -> bool {
    let Some(token) = text.split_whitespace().next() else {
        return false;
    };
    if let Some(inner) = token.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        if is_digits(inner) {
            parsed.attributes.insert(
                "process.pid".into(),
                Value::from(inner.parse::<u64>().unwrap_or_default()),
            );
        } else if let Some(severity) = read_level_word(inner) {
            parsed.severity = Some(severity);
        } else if inner.is_empty() || inner.len() > 40 {
            return false;
        } else {
            parsed.attributes.insert("thread".into(), Value::from(inner));
        }
        return true;
    }
    let word = token.trim_end_matches([':', ',']);
    let is_shouted = word.len() > 2 && word.bytes().all(|byte| byte.is_ascii_uppercase());
    if (is_shouted || after_time)
        && let Some(severity) = read_level_word(word)
    {
        parsed.severity = Some(severity);
        return true;
    }
    if let Some((pid, worker)) = token.trim_end_matches(':').split_once('#')
        && is_digits(pid)
        && is_digits(worker)
    {
        parsed.attributes.insert(
            "process.pid".into(),
            Value::from(pid.parse::<u64>().unwrap_or_default()),
        );
        return true;
    }
    if token.starts_with('*') && is_digits(&token[1..]) {
        return true;
    }
    false
}

fn take_logger(text: &str, parsed: &mut ParsedLine) -> bool {
    let mut tokens = text.split_whitespace();
    let (Some(logger), Some("-")) = (tokens.next(), tokens.next()) else {
        return false;
    };
    let is_name = logger
        .chars()
        .all(|character| character.is_alphanumeric() || "._$:/".contains(character));
    if !is_name {
        return false;
    }
    parsed.attributes.insert("logger".into(), Value::from(logger));
    true
}

fn strip_prefixes(text: &str, parsed: &mut ParsedLine) -> String {
    let mut rest = text.trim().to_string();
    let mut after_time = false;
    if let Some(end) = take_priority(&rest, parsed) {
        rest = rest[end..].trim_start().to_string();
    }
    for _ in 0..8 {
        if let Some(count) = take_iso_timestamp(&rest, parsed)
            .or_else(|| take_syslog_header(&rest, parsed))
            .or_else(|| take_redis_header(&rest, parsed))
            .or_else(|| take_glog_header(&rest, parsed))
        {
            rest = rest_after(&rest, count).to_string();
            after_time = true;
            continue;
        }
        if take_prefix_token(&rest, parsed, after_time) {
            rest = rest_after(&rest, 1).to_string();
            continue;
        }
        if parsed.severity.is_some() && take_logger(&rest, parsed) {
            rest = rest_after(&rest, 2).to_string();
            continue;
        }
        break;
    }
    rest
}

const LEVEL_KEYS: [&str; 4] = ["level", "lvl", "severity", "levelname"];
const MESSAGE_KEYS: [&str; 2] = ["msg", "message"];
const TIME_KEYS: [&str; 3] = ["time", "ts", "timestamp"];

fn read_level_value(value: &Value) -> Option<u8> {
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

fn read_time_value(value: &Value) -> Option<i64> {
    match value {
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|time| time.timestamp_millis()),
        Value::Number(number) => {
            let raw = number.as_i64()?;
            if raw > 100_000_000_000 { Some(raw) } else { raw.checked_mul(1000) }
        }
        _ => None,
    }
}

fn read_fields(mut fields: Map<String, Value>, fallback: &str) -> ParsedLine {
    let severity = LEVEL_KEYS
        .iter()
        .find_map(|key| fields.remove(*key).as_ref().and_then(read_level_value));
    let at = TIME_KEYS
        .iter()
        .find_map(|key| fields.remove(*key).as_ref().and_then(read_time_value));
    let message = MESSAGE_KEYS
        .iter()
        .find_map(|key| match fields.remove(*key)? {
            Value::String(text) => Some(text),
            other => Some(other.to_string()),
        })
        .unwrap_or_else(|| fallback.to_string());
    normalize_json_attributes(&mut fields);
    ParsedLine { at, severity, message, attributes: fields }
}

fn split_logfmt(text: &str) -> Option<Map<String, Value>> {
    let mut fields = Map::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        if key.is_empty()
            || !key
                .chars()
                .all(|character| character.is_alphanumeric() || "_.-".contains(character))
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

fn read_duration_ms(token: &str) -> Option<f64> {
    let (number, scale) = if let Some(number) = token.strip_suffix("ms") {
        (number, 1.0)
    } else if let Some(number) =
        token.strip_suffix("µs").or_else(|| token.strip_suffix("us"))
    {
        (number, 0.001)
    } else if let Some(number) = token.strip_suffix('s') {
        (number, 1000.0)
    } else {
        return None;
    };
    number
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value * scale)
}

fn insert_http(parsed: &mut ParsedLine, method: &str, path: &str, status: u16) {
    parsed.attributes.insert("http.method".into(), Value::from(method));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    parsed.attributes.insert("url.path".into(), Value::from(path));
    if !query.is_empty() {
        parsed.attributes.insert("url.query".into(), Value::from(query));
    }
    parsed.attributes.insert("http.status".into(), Value::from(status));
    let floor = match status {
        500.. => Some(SEVERITY_ERROR),
        400..500 => Some(SEVERITY_WARN),
        _ => None,
    };
    if let Some(floor) = floor {
        parsed.severity = Some(parsed.severity.unwrap_or(SEVERITY_INFO).max(floor));
    }
}

fn read_nginx_access(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (client, rest) = text.split_once(" - ")?;
    let request_start = rest.find("] \"")? + 3;
    let request_end = request_start + rest[request_start..].find('"')?;
    let mut request = rest[request_start..request_end].split_whitespace();
    let (method, path) = (request.next()?, request.next()?);
    let status: u16 = rest[request_end + 1..].split_whitespace().next()?.parse().ok()?;
    if !METHODS.contains(&method) {
        return None;
    }
    insert_http(parsed, method, path, status);
    parsed
        .attributes
        .insert("client.ip".into(), Value::from(client.trim()));
    Some(format!("{method} {path} {status}"))
}

fn read_request(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let index = tokens.iter().position(|token| METHODS.contains(token))?;
    let path = tokens.get(index + 1).filter(|path| path.starts_with('/'))?;
    let status: u16 = tokens
        .get(index + 2)?
        .parse()
        .ok()
        .filter(|status| (100..600).contains(status))?;
    let duration = tokens.get(index + 3).and_then(|token| read_duration_ms(token));
    insert_http(parsed, tokens[index], path, status);
    let mut used = 3;
    if let Some(duration) = duration {
        parsed.attributes.insert(
            "duration_ms".into(),
            Value::from((duration * 1000.0).round() / 1000.0),
        );
        used = 4;
    }
    let before = tokens[..index].join(" · ");
    let after = tokens[index + used..].join(" ");
    Some(match (before.is_empty(), after.is_empty()) {
        (false, false) => format!("{before} · {after}"),
        (false, true) => before,
        _ => after,
    })
}

fn read_ssh_failure(text: &str, parsed: &mut ParsedLine) {
    let Some(after) = text.split_once("Failed password for ").map(|(_, after)| after)
    else {
        return;
    };
    let after = after.strip_prefix("invalid user ").unwrap_or(after);
    if let Some(user) = after.split_whitespace().next() {
        parsed.attributes.insert("user".into(), Value::from(user));
    }
    if let Some(ip) = after
        .split_once(" from ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
    {
        parsed.attributes.insert("client.ip".into(), Value::from(ip));
    }
    parsed.severity = Some(parsed.severity.unwrap_or(SEVERITY_WARN).max(SEVERITY_WARN));
}

fn read_oom_kill(text: &str, parsed: &mut ParsedLine) {
    let Some((_, after)) = text.split_once("Killed process ") else {
        return;
    };
    let mut parts = after.split_whitespace();
    if let Some(pid) = parts.next().and_then(|pid| pid.parse::<u64>().ok()) {
        parsed.attributes.insert("process.pid".into(), Value::from(pid));
    }
    if let Some(name) = parts
        .next()
        .and_then(|name| name.strip_prefix('(')?.strip_suffix(')'))
    {
        parsed.attributes.insert("process.name".into(), Value::from(name));
    }
    parsed.severity = Some(SEVERITY_ERROR);
}

pub fn parse_text_line(text: &str) -> ParsedLine {
    let first_line = text.lines().next().unwrap_or_default();
    if let Some(fields) = split_logfmt(first_line) {
        return read_fields(fields, first_line);
    }
    let mut parsed = ParsedLine::default();
    let mut body = strip_prefixes(first_line, &mut parsed);
    let stream_tag = body.split_whitespace().take(2).collect::<Vec<_>>();
    if matches!(stream_tag.as_slice(), ["stdout" | "stderr", "F" | "P"]) {
        body = rest_after(&body, 2).to_string();
    }
    if body.starts_with('{')
        && let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(&body)
    {
        let mut inner = read_fields(fields, &body);
        inner.at = inner.at.or(parsed.at);
        return inner;
    }
    let message = read_nginx_access(&body, &mut parsed)
        .or_else(|| read_request(&body, &mut parsed))
        .unwrap_or(body);
    read_ssh_failure(&message, &mut parsed);
    read_oom_kill(&message, &mut parsed);
    let continuation: Vec<&str> = text.lines().skip(1).collect();
    parsed.message = if continuation.is_empty() {
        message
    } else {
        parsed
            .attributes
            .insert("stack.lines".into(), Value::from(continuation.len()));
        format!("{message}\n{}", continuation.join("\n"))
    };
    parsed
}

pub fn normalize_json_attributes(attributes: &mut Map<String, Value>) {
    let aliases = [
        ("http.method", ["method", "httpMethod"]),
        ("url.path", ["path", "url"]),
        ("http.status", ["status", "statusCode"]),
        ("duration_ms", ["durationMs", "responseTime"]),
    ];
    for (target, keys) in aliases {
        if attributes.contains_key(target) {
            continue;
        }
        if let Some(value) = keys.iter().find_map(|key| attributes.get(*key).cloned()) {
            attributes.insert(target.to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_text_line;

    #[test]
    fn strips_app_prefixes_and_reads_requests() {
        let parsed = parse_text_line(
            "2026-10-16T01:42:01.546Z [maily-api] INFO: 209.71.83.120 d9f7326e POST /v1/emails 202 84ms",
        );
        assert_eq!(parsed.message, "209.71.83.120 · d9f7326e");
        assert_eq!(parsed.severity, Some(9));
        assert_eq!(parsed.at, Some(1_792_114_921_546));
        assert_eq!(parsed.attributes["http.method"], "POST");
        assert_eq!(parsed.attributes["url.path"], "/v1/emails");
        assert_eq!(parsed.attributes["http.status"], 202);
        assert_eq!(parsed.attributes["duration_ms"], 84.0);
        assert_eq!(
            parse_text_line("93.184.216.34 GET /v1/links 500 2.1s").severity,
            Some(17)
        );
        let invalid = parse_text_line(
            "10.0.1.21 v36 POST /v1/emails 422 9ms validation failed: to[0] must be a valid email",
        );
        assert_eq!(invalid.severity, Some(13));
        assert_eq!(
            invalid.message,
            "10.0.1.21 · v36 · validation failed: to[0] must be a valid email"
        );
    }

    #[test]
    fn reads_nginx_lines() {
        let access = parse_text_line(
            r#"93.184.216.34 - - [16/Oct/2026:01:41:57 +0000] "GET /r/abc123 HTTP/1.1" 302 0 "-" "Mozilla/5.0""#,
        );
        assert_eq!(access.message, "GET /r/abc123 302");
        assert_eq!(access.attributes["client.ip"], "93.184.216.34");
        assert_eq!(access.attributes["http.status"], 302);
        let error = parse_text_line(
            "2026/10/16 01:41:55 [error] 8312#8312: *4931 connect() failed (111: Connection refused) while connecting to upstream",
        );
        assert_eq!(error.severity, Some(17));
        assert_eq!(error.attributes["process.pid"], 8312);
        assert!(error.message.starts_with("connect() failed"));
    }

    #[test]
    fn reads_system_lines() {
        let ssh = parse_text_line(
            "Oct 16 01:41:50 worker-1 sshd[23014]: Failed password for root from 45.155.205.233 port 50122 ssh2",
        );
        assert_eq!(
            ssh.message,
            "Failed password for root from 45.155.205.233 port 50122 ssh2"
        );
        assert_eq!(ssh.severity, Some(13));
        assert_eq!(ssh.attributes["user"], "root");
        assert_eq!(ssh.attributes["client.ip"], "45.155.205.233");
        assert_eq!(ssh.attributes["process.name"], "sshd");
        let cron = parse_text_line(
            "Oct 16 01:40:01 worker-1 CRON[22987]: (root) CMD (/usr/local/bin/backup.sh)",
        );
        assert_eq!(cron.message, "(root) CMD (/usr/local/bin/backup.sh)");
        assert_eq!(cron.attributes["process.pid"], 22987);
        let oom = parse_text_line(
            "Oct 16 01:39:44 worker-1 kernel: [812345.120] Out of memory: Killed process 4121 (node) total-vm:2148392kB",
        );
        assert_eq!(oom.severity, Some(17));
        assert_eq!(oom.attributes["process.name"], "node");
        let redis = parse_text_line(
            "77:M 16 Oct 2026 01:39:12.004 * Background saving started by pid 77",
        );
        assert_eq!(redis.message, "Background saving started by pid 77");
        assert_eq!(redis.attributes["process.pid"], 77);
        let postgres = parse_text_line(
            r#"2026-10-16 01:38:40.112 UTC [1882] ERROR:  duplicate key value violates unique constraint "links_slug_key""#,
        );
        assert_eq!(postgres.severity, Some(17));
        assert_eq!(postgres.attributes["process.pid"], 1882);
        assert!(postgres.message.starts_with("duplicate key value"));
    }

    #[test]
    fn keeps_plain_lines_and_stack_traces() {
        let plain = parse_text_line("something happened while syncing contacts");
        assert_eq!(plain.message, "something happened while syncing contacts");
        assert_eq!(plain.severity, None);
        assert!(plain.attributes.is_empty());
        let mixed_case = parse_text_line(
            "Error: read ECONNRESET\n    at TLSSocket.onClose\n    at node:net:337:12",
        );
        assert!(mixed_case.message.starts_with("Error: read ECONNRESET\n"));
        assert_eq!(mixed_case.attributes["stack.lines"], json!(2));
    }

    #[test]
    fn handles_formats_from_gpu_log() {
        let java = parse_text_line(
            "2024-01-15 10:30:00,123 [main] INFO  com.example.Foo - Started server on port 8080",
        );
        assert_eq!(java.message, "Started server on port 8080");
        assert_eq!(java.severity, Some(9));
        assert_eq!(java.attributes["thread"], "main");
        assert_eq!(java.attributes["logger"], "com.example.Foo");

        let logfmt = parse_text_line(
            r#"ts=2024-01-15T10:30:00Z level=warn msg="disk full" path=/var"#,
        );
        assert_eq!(logfmt.message, "disk full");
        assert_eq!(logfmt.severity, Some(13));
        assert_eq!(logfmt.at, Some(1_705_314_600_000));
        assert_eq!(logfmt.attributes["path"], "/var");
        assert!(
            !parse_text_line("2024-01-15 INFO msg=hi")
                .attributes
                .contains_key("msg")
        );

        let cri = parse_text_line(
            r#"2024-01-15T10:30:00.123456789Z stdout F {"level":"error","msg":"boom"}"#,
        );
        assert_eq!(cri.message, "boom");
        assert_eq!(cri.severity, Some(17));
        assert_eq!(cri.at, Some(1_705_314_600_123));

        let pino = parse_text_line(
            r#"{"level":30,"time":1705314600123,"pid":42,"msg":"hello","reqId":"abc"}"#,
        );
        assert_eq!(pino.message, "hello");
        assert_eq!(pino.severity, Some(9));
        assert_eq!(pino.at, Some(1_705_314_600_123));
        assert_eq!(pino.attributes["reqId"], "abc");

        let pri = parse_text_line(
            "<134>Jan 15 10:30:00 myhost sshd[1234]: Accepted publickey for alice",
        );
        assert_eq!(pri.severity, Some(9));
        assert_eq!(pri.message, "Accepted publickey for alice");
        assert_eq!(pri.attributes["process.pid"], 1234);
        assert_eq!(parse_text_line("<11>app: disk failed").severity, Some(17));

        let glog = parse_text_line(
            "E0115 10:30:00.123456   4242 server.go:42] connection reset by peer",
        );
        assert_eq!(glog.severity, Some(17));
        assert_eq!(glog.message, "connection reset by peer");
        assert_eq!(glog.attributes["code.location"], "server.go:42");

        let offset = parse_text_line("2024-01-15 10:30:00.123 +05:30 job finished");
        assert_eq!(offset.at, Some(1_705_294_800_123));
        assert_eq!(offset.message, "job finished");

        let windows = parse_text_line(
            "2016-09-28 04:30:31, Error                 CSI    00000001 WcpInitialize called",
        );
        assert_eq!(windows.severity, Some(17));
        assert!(windows.message.starts_with("CSI"));

        let access = parse_text_line(
            r#"10.0.0.1 - - [15/Jan/2024:10:30:00 +0000] "GET /index.html HTTP/1.1" 200 1234 "-" "curl/8.4.0""#,
        );
        assert_eq!(access.attributes["url.path"], "/index.html");
        assert_eq!(access.attributes["http.status"], 200);

        let with_query = parse_text_line(
            r#"10.0.0.1 - - [15/Jan/2024:10:30:00 +0000] "GET /roadmap?tab=open&page=2 HTTP/1.1" 200 1234 "-" "curl/8.4.0""#,
        );
        assert_eq!(with_query.attributes["url.path"], "/roadmap");
        assert_eq!(with_query.attributes["url.query"], "tab=open&page=2");

        let plain_dash = parse_text_line("deploy - started by alice");
        assert_eq!(plain_dash.message, "deploy - started by alice");
    }
}
