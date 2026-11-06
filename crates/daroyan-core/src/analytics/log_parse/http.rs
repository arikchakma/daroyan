use serde_json::Value;

use super::{
    METHODS, ParsedLine, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_FATAL, SEVERITY_INFO,
    SEVERITY_WARN,
};

pub(super) fn read_duration_ms(token: &str) -> Option<f64> {
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

pub(super) fn insert_http(
    parsed: &mut ParsedLine,
    method: &str,
    path: &str,
    status: u16,
) {
    parsed.attributes.insert("http.method".into(), Value::from(method));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    parsed.attributes.insert("url.path".into(), Value::from(path));
    if !query.is_empty() {
        parsed.attributes.insert("url.query".into(), Value::from(query));
    }
    parsed.attributes.insert("http.status".into(), Value::from(status));
    raise_for_status(&mut parsed.severity, status);
}

pub(super) fn read_nginx_access(text: &str, parsed: &mut ParsedLine) -> Option<String> {
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

    let tail = &rest[request_end + 1..];
    let timings = [
        ("duration_ms", ["rt", "request_time"]),
        ("upstream.duration_ms", ["urt", "upstream_response_time"]),
    ];
    for (target, keys) in timings {
        if let Some(seconds) = keys.iter().find_map(|key| read_nginx_seconds(tail, key)) {
            parsed.attributes.insert(
                target.into(),
                Value::from((seconds * 1_000_000.0).round() / 1000.0),
            );
        }
    }

    Some(request_message(method, path, status))
}

fn read_nginx_seconds(text: &str, key: &str) -> Option<f64> {
    let needle = format!("{key}=");
    let start = text.rmatch_indices(&needle).find_map(|(index, _)| {
        let is_token_start =
            text[..index].chars().next_back().is_none_or(char::is_whitespace);
        is_token_start.then_some(index + needle.len())
    })?;

    let value = &text[start..];
    let value = match value.strip_prefix('"') {
        Some(quoted) => &quoted[..quoted.find('"')?],
        None => value.split_whitespace().next().unwrap_or_default(),
    };

    let mut total = None;
    for part in value.split([',', ':']).map(str::trim) {
        if part == "-" {
            continue;
        }
        let seconds = part
            .parse::<f64>()
            .ok()
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)?;
        total = Some(total.unwrap_or(0.0) + seconds);
    }
    total
}

pub(super) fn read_request(text: &str, parsed: &mut ParsedLine) -> Option<String> {
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

pub(super) fn raise_for_status(severity: &mut Option<u8>, status: u16) {
    let floor = match status {
        500.. => Some(SEVERITY_ERROR),
        400..500 => Some(SEVERITY_WARN),
        _ => None,
    };
    if let Some(floor) = floor {
        *severity = Some(severity.unwrap_or(SEVERITY_INFO).max(floor));
    }
}

fn request_message(method: &str, path: &str, status: u16) -> String {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    format!("{method} {path} {status}")
}

fn split_quoted(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            tokens.push(&quoted[..end]);
            rest = quoted.get(end + 1..).unwrap_or_default().trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            tokens.push(&rest[..end]);
            rest = rest[end..].trim_start();
        }
    }
    tokens
}

fn insert_text(parsed: &mut ParsedLine, key: &str, value: &str) {
    if !value.is_empty() && value != "-" {
        parsed.attributes.insert(key.into(), Value::from(value));
    }
}

pub(super) fn read_quoted_request(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    if !text.starts_with('"') {
        return None;
    }
    let tokens = split_quoted(text);
    let mut request = tokens.first()?.split_whitespace();
    let (method, path) = (request.next()?, request.next()?);
    let status: u16 = tokens.get(1)?.parse().ok()?;
    if !METHODS.contains(&method) || !(100..600).contains(&status) {
        return None;
    }
    insert_http(parsed, method, path, status);
    let is_envoy = tokens.len() >= 7
        && !tokens[2].bytes().all(|byte| byte.is_ascii_digit())
        && tokens[3..6]
            .iter()
            .all(|token| token.bytes().all(|byte| byte.is_ascii_digit()));
    if is_envoy {
        if let Ok(duration) = tokens[5].parse::<f64>() {
            parsed.attributes.insert("duration_ms".into(), Value::from(duration));
        }
        insert_text(parsed, "envoy.response_flags", tokens[2]);
        if let Some(client) =
            tokens.get(7).and_then(|forwarded| forwarded.split(',').next())
        {
            insert_text(parsed, "client.ip", client.trim());
        }
        insert_text(
            parsed,
            "user_agent.original",
            tokens.get(8).copied().unwrap_or_default(),
        );
        insert_text(
            parsed,
            "http.request_id",
            tokens.get(9).copied().unwrap_or_default(),
        );
        insert_text(parsed, "url.domain", tokens.get(10).copied().unwrap_or_default());
        insert_text(
            parsed,
            "envoy.upstream_host",
            tokens.get(11).copied().unwrap_or_default(),
        );
    }
    Some(request_message(method, path, status))
}

pub(super) fn read_haproxy(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let tokens = split_quoted(text);
    let client = tokens.first()?.rsplit_once(':')?.0;
    let is_date = tokens
        .get(1)
        .is_some_and(|token| token.starts_with('[') && token.ends_with(']'));
    let (backend, server) = tokens.get(3)?.split_once('/')?;
    let timers: Vec<&str> = tokens.get(4)?.split('/').collect();
    let status: u16 = tokens.get(5)?.parse().ok()?;
    let request = tokens.last()?;
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next()?, parts.next()?);
    if !is_date || timers.len() != 5 || !METHODS.contains(&method) {
        return None;
    }
    insert_http(parsed, method, path, status);
    insert_text(parsed, "client.ip", client);
    insert_text(parsed, "haproxy.frontend", tokens[2].trim_end_matches('~'));
    insert_text(parsed, "haproxy.backend", backend);
    insert_text(parsed, "haproxy.server", server);
    if let Some(total) = timers[4].parse::<f64>().ok().filter(|total| *total >= 0.0) {
        parsed.attributes.insert("duration_ms".into(), Value::from(total));
    }
    Some(request_message(method, path, status))
}

pub(super) fn read_apache_error(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (tag, mut rest) = text.strip_prefix('[')?.split_once("] ")?;
    let (module, level) = match tag.split_once(':') {
        Some((module, level)) => (Some(module), level),
        None => (None, tag),
    };
    parsed.severity = Some(read_apache_level(level)?);
    if let Some(module) = module {
        insert_text(parsed, "apache.module", module);
    }
    while let Some((inner, after)) =
        rest.strip_prefix('[').and_then(|after| after.split_once("] "))
    {
        if let Some(ids) = inner.strip_prefix("pid ") {
            let (pid, thread) = ids.split_once(":tid ").unwrap_or((ids, ""));
            if let Ok(pid) = pid.parse::<u64>() {
                parsed.attributes.insert("process.pid".into(), Value::from(pid));
            }
            if let Ok(thread) = thread.parse::<u64>() {
                parsed.attributes.insert("thread.id".into(), Value::from(thread));
            }
        } else if let Some(client) = inner.strip_prefix("client ") {
            insert_text(
                parsed,
                "client.ip",
                client.rsplit_once(':').map_or(client, |(ip, _)| ip),
            );
        } else {
            break;
        }
        rest = after;
    }
    if let Some(code) = rest.split(": ").find(|part| {
        part.len() == 7
            && part.starts_with("AH")
            && part[2..].bytes().all(|byte| byte.is_ascii_digit())
    }) {
        insert_text(parsed, "apache.error_code", code);
    }
    Some(rest.to_string())
}

fn read_apache_level(level: &str) -> Option<u8> {
    match level {
        "emerg" | "alert" | "crit" => Some(SEVERITY_FATAL),
        "error" => Some(SEVERITY_ERROR),
        "warn" => Some(SEVERITY_WARN),
        "notice" | "info" => Some(SEVERITY_INFO),
        _ if level.starts_with("debug") || level.starts_with("trace") => {
            Some(SEVERITY_DEBUG)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::analytics::log_parse::tests::parse;

    #[test]
    fn reads_haproxy_http_logs() {
        let line = parse(
            r#"Oct 28 01:50:02 lb haproxy[1234]: 10.0.0.1:51234 [28/Oct/2026:01:50:02.123] web~ app/srv1 0/0/1/12/13 503 212 - - ---- 1/1/0/0/0 0/0 "GET /health?full=1 HTTP/1.1""#,
        );
        assert_eq!(line.message, "GET /health 503");
        assert_eq!(line.severity, Some(17));
        assert_eq!(line.attributes["client.ip"], "10.0.0.1");
        assert_eq!(line.attributes["haproxy.frontend"], "web");
        assert_eq!(line.attributes["haproxy.backend"], "app");
        assert_eq!(line.attributes["haproxy.server"], "srv1");
        assert_eq!(line.attributes["duration_ms"], 13.0);
        assert_eq!(line.attributes["url.query"], "full=1");
    }

    #[test]
    fn reads_envoy_and_django_access_logs() {
        let envoy = parse(
            r#"[2026-10-28T01:50:02.123Z] "POST /api/orders HTTP/1.1" 201 - 512 18 42 40 "203.0.113.9, 10.0.0.1" "curl/8.0" "5e1c-77" "api.example.com" "10.0.0.2:8080""#,
        );
        assert_eq!(envoy.message, "POST /api/orders 201");
        assert_eq!(envoy.at, Some(1_793_152_202_123));
        assert_eq!(envoy.attributes["duration_ms"], 42.0);
        assert_eq!(envoy.attributes["client.ip"], "203.0.113.9");
        assert_eq!(envoy.attributes["url.domain"], "api.example.com");
        assert_eq!(envoy.attributes["envoy.upstream_host"], "10.0.0.2:8080");
        assert!(!envoy.attributes.contains_key("envoy.response_flags"));

        let django = parse(r#"[28/Oct/2026 01:50:02] "GET /api/users HTTP/1.1" 500 145"#);
        assert_eq!(django.message, "GET /api/users 500");
        assert_eq!(django.severity, Some(17));
    }

    #[test]
    fn reads_nginx_timings() {
        let line = parse(
            r#"93.184.216.34 - - [16/Oct/2026:01:41:57 +0000] "GET /api/orders HTTP/1.1" 200 512 "-" "curl/8.0" rt=0.010 uct="0.001" uht="0.009" urt="0.008""#,
        );
        assert_eq!(line.message, "GET /api/orders 200");
        assert_eq!(line.attributes["duration_ms"], 10.0);
        assert_eq!(line.attributes["upstream.duration_ms"], 8.0);

        let retried = parse(
            r#"10.0.0.1 - - [16/Oct/2026:01:41:57 +0000] "POST /checkout HTTP/1.1" 502 0 "-" "Mozilla/5.0 (rt=9)" request_time=1.250 upstream_response_time="0.400, 0.750 : 0.050""#,
        );
        assert_eq!(retried.attributes["duration_ms"], 1250.0);
        assert_eq!(retried.attributes["upstream.duration_ms"], 1200.0);

        let cached = parse(
            r#"10.0.0.1 - - [16/Oct/2026:01:41:57 +0000] "GET /logo.png HTTP/1.1" 200 1024 "-" "curl/8.0" rt=0.000 urt=-"#,
        );
        assert_eq!(cached.attributes["duration_ms"], 0.0);
        assert!(!cached.attributes.contains_key("upstream.duration_ms"));

        let combined = parse(
            r#"10.0.0.1 - - [16/Oct/2026:01:41:57 +0000] "GET / HTTP/1.1" 200 0 "-" "curl/8.0""#,
        );
        assert!(!combined.attributes.contains_key("duration_ms"));
    }

    #[test]
    fn reads_apache_error_logs() {
        let modern = parse(
            "[Wed Oct 28 01:50:02.123456 2026] [proxy:error] [pid 1234:tid 5678] [client 10.0.0.1:51234] (111)Connection refused: AH00957: failed to make connection to backend",
        );
        assert_eq!(modern.severity, Some(17));
        assert_eq!(modern.attributes["apache.module"], "proxy");
        assert_eq!(modern.attributes["process.pid"], 1234);
        assert_eq!(modern.attributes["thread.id"], 5678);
        assert_eq!(modern.attributes["client.ip"], "10.0.0.1");
        assert_eq!(modern.attributes["apache.error_code"], "AH00957");
        assert!(modern.message.starts_with("(111)Connection refused"));

        let old = parse(
            "[Wed Oct 28 01:50:02 2026] [warn] [client 10.0.0.1] File does not exist: /var/www/favicon.ico",
        );
        assert_eq!(old.severity, Some(13));
        assert_eq!(old.message, "File does not exist: /var/www/favicon.ico");
    }
}
