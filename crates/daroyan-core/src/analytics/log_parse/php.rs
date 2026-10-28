use serde_json::Value;

use super::{ParsedLine, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN, read_level_word};

pub(super) const LARAVEL_LEVELS: [&str; 8] =
    ["DEBUG", "INFO", "NOTICE", "WARNING", "ERROR", "CRITICAL", "ALERT", "EMERGENCY"];

pub(super) fn read_laravel(rest: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (channel, message) = rest.split_once(": ")?;
    let (environment, level) = channel.split_once('.')?;
    if !LARAVEL_LEVELS.contains(&level) || environment.contains(char::is_whitespace) {
        return None;
    }
    parsed.severity = read_level_word(level);
    parsed
        .attributes
        .insert("deployment.environment".into(), Value::from(environment));
    let message = message.strip_suffix(" []").unwrap_or(message);
    let Some(context_start) = message.find(" {\"") else {
        return Some(message.to_string());
    };
    let (text, context) = message.split_at(context_start);
    let context = context.trim_start();
    if let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(context) {
        for (key, value) in fields {
            parsed.attributes.entry(key).or_insert(value);
        }
    }
    if let Some(kind) = context
        .split_once("\"exception\":\"[object] (")
        .and_then(|(_, after)| after.split_once('(').map(|(kind, _)| kind))
    {
        parsed.attributes.remove("exception");
        parsed.attributes.insert("exception.type".into(), Value::from(kind));
    }
    Some(text.to_string())
}

pub(super) fn read_php_error(rest: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (kind, message) = rest.strip_prefix("PHP ")?.split_once(':')?;
    let severity = match kind {
        "Fatal error"
        | "Parse error"
        | "Recoverable fatal error"
        | "Catchable fatal error" => SEVERITY_ERROR,
        "Warning" => SEVERITY_WARN,
        "Notice" | "Deprecated" | "Strict Standards" => SEVERITY_INFO,
        _ => return None,
    };
    parsed.severity = Some(severity);
    parsed.attributes.insert("error.type".into(), Value::from(kind));
    let message = message.trim();
    let location = message
        .rsplit_once(" in /")
        .map(|(_, location)| format!("/{location}"));
    if let Some(location) = location {
        let (file, line) = match location.split_once(" on line ") {
            Some((file, line)) => (file.to_string(), line.parse::<u64>().ok()),
            None => match location.rsplit_once(':') {
                Some((file, line)) => (file.to_string(), line.parse::<u64>().ok()),
                None => (location.clone(), None),
            },
        };
        parsed.attributes.insert("code.filepath".into(), Value::from(file));
        if let Some(line) = line {
            parsed.attributes.insert("code.lineno".into(), Value::from(line));
        }
    }
    Some(message.to_string())
}

pub(super) fn read_php_fpm(rest: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (level, message) = rest.split_once(": ")?;
    if level.contains(char::is_lowercase) {
        return None;
    }
    parsed.severity = Some(read_level_word(level)?);
    let message = match message
        .strip_prefix("[pool ")
        .and_then(|after| after.split_once("] "))
    {
        Some((pool, message)) => {
            parsed.attributes.insert("php_fpm.pool".into(), Value::from(pool));
            message
        }
        None => message,
    };
    Some(message.to_string())
}
