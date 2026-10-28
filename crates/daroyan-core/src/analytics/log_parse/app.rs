use serde_json::Value;

use super::http::{raise_for_status, read_duration_ms, read_quoted_request};
use super::{METHODS, ParsedLine, SEVERITY_ERROR, is_digits, read_level_word};

const PYTHON_LEVELS: [&str; 5] = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"];

fn insert_logger(parsed: &mut ParsedLine, logger: &str) {
    let logger = logger.trim();
    if !logger.is_empty() {
        parsed.attributes.insert("logger".into(), Value::from(logger));
    }
}

pub(super) fn read_rails(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (letter, rest) = text.split_once(", [")?;
    if letter.len() != 1 || !"DIWEFAU".contains(letter) {
        return None;
    }
    let (header, rest) = rest.split_once(']')?;
    let (_, pid) = header.rsplit_once(" #")?;
    let (level, rest) = rest.trim_start().split_once(" -- ")?;
    let (progname, message) = rest.split_once(": ")?;
    parsed.severity = Some(read_level_word(level.trim())?);
    if let Ok(pid) = pid.parse::<u64>() {
        parsed.attributes.insert("process.pid".into(), Value::from(pid));
    }
    insert_logger(parsed, progname);
    let message = match message.strip_prefix('[').and_then(|after| after.split_once("] "))
    {
        Some((request_id, after)) if !request_id.contains(' ') => {
            parsed
                .attributes
                .insert("http.request_id".into(), Value::from(request_id));
            after
        }
        _ => message,
    };
    read_rails_request(message, parsed);
    Some(message.to_string())
}

fn read_rails_request(message: &str, parsed: &mut ParsedLine) {
    if let Some(rest) = message.strip_prefix("Started ") {
        let mut tokens = rest.split_whitespace();
        let (Some(method), Some(path)) = (tokens.next(), tokens.next()) else {
            return;
        };
        if !METHODS.contains(&method) {
            return;
        }
        let path = path.trim_matches('"');
        parsed.attributes.insert("http.method".into(), Value::from(method));
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        parsed.attributes.insert("url.path".into(), Value::from(path));
        if !query.is_empty() {
            parsed.attributes.insert("url.query".into(), Value::from(query));
        }
        if let Some(client) = rest
            .split_once(" for ")
            .and_then(|(_, after)| after.split_whitespace().next())
        {
            parsed.attributes.insert("client.ip".into(), Value::from(client));
        }
    } else if let Some(rest) = message.strip_prefix("Completed ") {
        let Some(status) = rest
            .split_whitespace()
            .next()
            .and_then(|status| status.parse::<u16>().ok())
        else {
            return;
        };
        parsed.attributes.insert("http.status".into(), Value::from(status));
        raise_for_status(&mut parsed.severity, status);
        if let Some(duration) = rest
            .split_once(" in ")
            .and_then(|(_, after)| after.split_whitespace().next())
            .and_then(read_duration_ms)
        {
            parsed.attributes.insert("duration_ms".into(), Value::from(duration));
        }
    }
}

pub(super) fn read_python(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (level, rest) = text.split_once(':')?;
    if !PYTHON_LEVELS.contains(&level) {
        return None;
    }
    parsed.severity = read_level_word(level);
    if rest.starts_with(char::is_whitespace) {
        let rest = rest.trim();
        if let Some((client, request)) = rest.split_once(" - ")
            && let Some(message) = read_quoted_request(request, parsed)
        {
            let client = client.rsplit_once(':').map_or(client, |(ip, _)| ip);
            parsed.attributes.insert("client.ip".into(), Value::from(client));
            return Some(message);
        }
        return Some(rest.to_string());
    }
    let (logger, message) = rest.split_once(':')?;
    if logger.contains(char::is_whitespace) {
        return None;
    }
    insert_logger(parsed, logger);
    Some(message.to_string())
}

pub(super) fn read_python_dashes(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let mut parts = text.strip_prefix("- ")?.splitn(3, " - ");
    let (logger, level, message) = (parts.next()?, parts.next()?, parts.next()?);
    if !PYTHON_LEVELS.contains(&level) {
        return None;
    }
    parsed.severity = read_level_word(level);
    insert_logger(parsed, logger);
    Some(message.to_string())
}

pub(super) fn read_flask(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (level, rest) = text.split_once(" in ")?;
    if !PYTHON_LEVELS.contains(&level) {
        return None;
    }
    let (module, message) = rest.split_once(": ")?;
    parsed.severity = read_level_word(level);
    insert_logger(parsed, module);
    Some(message.to_string())
}

pub(super) fn read_spring(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (pid, rest) = text.split_once(" --- ")?;
    if !is_digits(pid) {
        return None;
    }
    let mut groups = Vec::new();
    let mut rest = rest;
    while let Some((group, after)) =
        rest.strip_prefix('[').and_then(|after| after.split_once(']'))
    {
        groups.push(group.trim());
        rest = after.trim_start();
    }
    let (logger, message) = rest.split_once(" : ")?;
    parsed
        .attributes
        .insert("process.pid".into(), Value::from(pid.parse::<u64>().ok()?));
    match groups.as_slice() {
        [application, thread] => {
            parsed
                .attributes
                .insert("service.name".into(), Value::from(*application));
            parsed.attributes.insert("thread".into(), Value::from(*thread));
        }
        [thread] => {
            parsed.attributes.insert("thread".into(), Value::from(*thread));
        }
        _ => {}
    }
    insert_logger(parsed, logger);
    Some(message.trim().to_string())
}

pub(super) fn read_level_and_logger(
    text: &str,
    parsed: &mut ParsedLine,
) -> Option<String> {
    let (level, message) = text.split_once(' ')?;
    if level.bytes().any(|byte| byte.is_ascii_lowercase()) {
        return None;
    }
    parsed.severity = Some(read_level_word(level)?);
    let message = message.trim();
    if let Some((before, logger)) =
        message.strip_suffix(')').and_then(|rest| rest.rsplit_once(" ("))
        && logger.contains('.')
        && !logger.contains(char::is_whitespace)
    {
        insert_logger(parsed, logger);
        return Some(before.to_string());
    }
    Some(message.to_string())
}

pub(super) fn read_elasticsearch(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (_, rest) = text.strip_prefix('[')?.split_once("][")?;
    let (level, rest) = rest.split_once("][")?;
    let (logger, rest) = rest.split_once("] ")?;
    parsed.severity = Some(read_level_word(level.trim())?);
    insert_logger(parsed, logger);
    let message = match rest.strip_prefix('[').and_then(|after| after.split_once("] ")) {
        Some((node, message)) => {
            parsed
                .attributes
                .insert("elasticsearch.node".into(), Value::from(node));
            message
        }
        None => rest,
    };
    Some(message.to_string())
}

pub(super) fn read_python_traceback(
    text: &str,
    parsed: &mut ParsedLine,
) -> Option<String> {
    if text.lines().next()? != "Traceback (most recent call last):" {
        return None;
    }
    let last = text.lines().last()?.trim();
    let kind = last.split(':').next().unwrap_or(last);
    parsed.severity = Some(SEVERITY_ERROR);
    parsed.attributes.insert("exception.type".into(), Value::from(kind));
    parsed
        .attributes
        .insert("stack.lines".into(), Value::from(text.lines().count()));
    Some(format!("{last}\n{text}"))
}

#[cfg(test)]
mod tests {
    use crate::analytics::log_parse::tests::parse;

    #[test]
    fn reads_python_tracebacks() {
        let traceback = parse(
            "Traceback (most recent call last):\n  File \"/app/worker.py\", line 12, in run\n    connect()\nConnectionRefusedError: [Errno 111] Connection refused",
        );
        assert_eq!(traceback.severity, Some(17));
        assert!(traceback.message.starts_with(
            "ConnectionRefusedError: [Errno 111] Connection refused\nTraceback"
        ));
        assert_eq!(traceback.attributes["exception.type"], "ConnectionRefusedError");
    }

    #[test]
    fn reads_rails_logs() {
        let completed = parse(
            "I, [2026-10-28T01:50:02.123456 #1234]  INFO -- : [8f3a1c] Completed 500 Internal Server Error in 12ms (Views: 1.2ms | ActiveRecord: 3.4ms)",
        );
        assert_eq!(completed.severity, Some(17));
        assert!(completed.message.starts_with("Completed 500"));
        assert_eq!(completed.attributes["http.request_id"], "8f3a1c");
        assert_eq!(completed.attributes["http.status"], 500);
        assert_eq!(completed.attributes["duration_ms"], 12.0);
        assert_eq!(completed.attributes["process.pid"], 1234);

        let started = parse(
            "I, [2026-10-28T01:50:02.123456 #1234]  INFO -- : Started GET \"/users?page=2\" for 10.0.0.1 at 2026-10-28 01:50:02 +0000",
        );
        assert_eq!(started.attributes["http.method"], "GET");
        assert_eq!(started.attributes["url.path"], "/users");
        assert_eq!(started.attributes["client.ip"], "10.0.0.1");

        let sidekiq = parse(
            "E, [2026-10-28T01:50:02.123456 #1234] ERROR -- sidekiq: Job raised exception",
        );
        assert_eq!(sidekiq.severity, Some(17));
        assert_eq!(sidekiq.attributes["logger"], "sidekiq");
    }

    #[test]
    fn reads_python_logs() {
        let basic = parse("ERROR:root:Database connection lost");
        assert_eq!(
            (basic.severity, basic.message.as_str()),
            (Some(17), "Database connection lost")
        );
        assert_eq!(basic.attributes["logger"], "root");

        let dashes =
            parse("2026-10-28 01:50:02,123 - app.worker - WARNING - retrying job 42");
        assert_eq!(
            (dashes.severity, dashes.message.as_str()),
            (Some(13), "retrying job 42")
        );
        assert_eq!(dashes.attributes["logger"], "app.worker");

        let flask = parse(
            "[2026-10-28 01:50:02,123] ERROR in app: Exception on /checkout [POST]",
        );
        assert_eq!(
            (flask.severity, flask.message.as_str()),
            (Some(17), "Exception on /checkout [POST]")
        );

        let uvicorn = parse(
            "INFO:     10.0.0.1:51234 - \"GET /api/items HTTP/1.1\" 500 Internal Server Error",
        );
        assert_eq!(uvicorn.message, "GET /api/items 500");
        assert_eq!(uvicorn.severity, Some(17));
        assert_eq!(uvicorn.attributes["client.ip"], "10.0.0.1");
        assert_eq!(
            parse("INFO:     Application startup complete.").message,
            "Application startup complete."
        );
    }

    #[test]
    fn reads_java_logs() {
        let spring2 = parse(
            "2026-10-28 01:50:02.123 ERROR 1234 --- [nio-8080-exec-1] c.e.demo.Controller      : Request failed",
        );
        assert_eq!(
            (spring2.severity, spring2.message.as_str()),
            (Some(17), "Request failed")
        );
        assert_eq!(spring2.attributes["thread"], "nio-8080-exec-1");
        assert_eq!(spring2.attributes["logger"], "c.e.demo.Controller");

        let spring3 = parse(
            "2026-10-28T01:50:02.123Z ERROR 1234 --- [demo] [nio-8080-exec-1] c.e.demo.Controller : Request failed",
        );
        assert_eq!(spring3.attributes["service.name"], "demo");
        assert_eq!(spring3.at, Some(1_793_152_202_123));

        let logback = parse("01:50:02.123 [main] ERROR com.example.App - Request failed");
        assert_eq!(
            (logback.severity, logback.message.as_str()),
            (Some(17), "Request failed")
        );
        assert_eq!(logback.attributes["logger"], "com.example.App");
    }

    #[test]
    fn reads_kafka_rabbitmq_and_elasticsearch_logs() {
        let kafka = parse(
            "[2026-10-28 01:50:02,123] WARN [Controller id=1] Processing automatic leader election (kafka.controller.KafkaController)",
        );
        assert_eq!(kafka.severity, Some(13));
        assert_eq!(
            kafka.message,
            "[Controller id=1] Processing automatic leader election"
        );
        assert_eq!(kafka.attributes["logger"], "kafka.controller.KafkaController");

        let rabbitmq = parse(
            "2026-10-28 01:50:02.123456+00:00 [error] <0.123.0> closing AMQP connection <0.456.0>: missed heartbeats",
        );
        assert_eq!(rabbitmq.severity, Some(17));
        assert_eq!(rabbitmq.at, Some(1_793_152_202_123));
        assert_eq!(rabbitmq.attributes["erlang.pid"], "0.123.0");
        assert!(rabbitmq.message.starts_with("closing AMQP connection"));

        let elasticsearch = parse(
            "[2026-10-28T01:50:02,123][WARN ][o.e.c.r.a.DiskThresholdMonitor] [node-1] high disk watermark exceeded",
        );
        assert_eq!(
            (elasticsearch.severity, elasticsearch.message.as_str()),
            (Some(13), "high disk watermark exceeded")
        );
        assert_eq!(elasticsearch.attributes["elasticsearch.node"], "node-1");
    }
}
