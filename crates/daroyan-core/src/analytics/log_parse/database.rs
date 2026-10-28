use chrono::DateTime;
use serde_json::Value;

use super::{
    ParsedLine, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN, is_digits, rest_after,
};

pub(super) fn read_mysql(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let mut rest = text;
    let thread = rest.split_whitespace().next().filter(|thread| is_digits(thread))?;
    rest = rest_after(rest, 1);
    let (level, after) = rest.strip_prefix('[')?.split_once("] ")?;
    let severity = match level {
        "ERROR" => SEVERITY_ERROR,
        "Warning" => SEVERITY_WARN,
        "Note" | "System" => SEVERITY_INFO,
        _ => return None,
    };
    rest = after;
    parsed.severity = Some(severity);
    parsed.attributes.insert("db.system".into(), Value::from("mysql"));
    parsed
        .attributes
        .insert("thread.id".into(), Value::from(thread.parse::<u64>().ok()?));
    if let Some((code, after)) =
        rest.strip_prefix("[MY-").and_then(|after| after.split_once("] "))
    {
        parsed
            .attributes
            .insert("mysql.error_code".into(), Value::from(format!("MY-{code}")));
        rest = after;
    }
    if let Some((subsystem, after)) =
        rest.strip_prefix('[').and_then(|after| after.split_once("] "))
    {
        parsed
            .attributes
            .insert("mysql.subsystem".into(), Value::from(subsystem));
        rest = after;
    }
    Some(rest.to_string())
}

pub(super) fn read_postgres_duration(text: &str, parsed: &mut ParsedLine) {
    let Some(after) = text.strip_prefix("duration: ") else {
        return;
    };
    let Some((number, rest)) = after.split_once(" ms") else {
        return;
    };
    let Ok(duration) = number.parse::<f64>() else {
        return;
    };
    parsed.attributes.insert("duration_ms".into(), Value::from(duration));
    let rest = rest.trim_start();
    let statement = rest.strip_prefix("statement: ").or_else(|| {
        rest.split_once(": ")
            .filter(|(head, _)| head.starts_with("execute "))
            .map(|(_, sql)| sql)
    });
    if let Some(statement) = statement {
        parsed
            .attributes
            .insert("db.statement".into(), Value::from(statement));
    }
}

pub(super) fn read_mysql_slow_query(
    text: &str,
    parsed: &mut ParsedLine,
) -> Option<String> {
    let first = text.lines().next()?;
    if !first.starts_with("# Time: ") && !first.starts_with("# User@Host: ") {
        return None;
    }
    let mut statement = Vec::new();
    let mut duration_ms = None;
    for line in text.lines() {
        if let Some(time) = line.strip_prefix("# Time: ") {
            if let Ok(time) = DateTime::parse_from_rfc3339(time.trim()) {
                parsed.at = Some(time.timestamp_millis());
            }
        } else if let Some(rest) = line.strip_prefix("# User@Host: ") {
            if let Some(user) = rest.split('[').next() {
                parsed.attributes.insert("user".into(), Value::from(user.trim()));
            }
            if let Some(host) = rest
                .split_once(" @ ")
                .map(|(_, host)| host.split_whitespace().next().unwrap_or_default())
            {
                parsed.attributes.insert("client.address".into(), Value::from(host));
            }
        } else if line.starts_with("# Query_time: ") {
            let fields: Vec<&str> =
                line.trim_start_matches("# ").split_whitespace().collect();
            for pair in fields.chunks(2) {
                let [key, value] = pair else { continue };
                let Ok(number) = value.parse::<f64>() else { continue };
                match key.trim_end_matches(':') {
                    "Query_time" => {
                        duration_ms = Some((number * 1_000_000.0).round() / 1000.0);
                    }
                    "Lock_time" => {
                        parsed.attributes.insert(
                            "mysql.lock_time_ms".into(),
                            Value::from((number * 1_000_000.0).round() / 1000.0),
                        );
                    }
                    "Rows_sent" => {
                        parsed
                            .attributes
                            .insert("mysql.rows_sent".into(), Value::from(number));
                    }
                    "Rows_examined" => {
                        parsed
                            .attributes
                            .insert("mysql.rows_examined".into(), Value::from(number));
                    }
                    _ => {}
                }
            }
        } else if !line.starts_with('#')
            && !line.starts_with("SET timestamp=")
            && !line.starts_with("use ")
        {
            statement.push(line.trim());
        }
    }
    let statement = statement.join(" ");
    let duration_ms = duration_ms?;
    parsed.severity = Some(SEVERITY_WARN);
    parsed.attributes.insert("db.system".into(), Value::from("mysql"));
    parsed
        .attributes
        .insert("duration_ms".into(), Value::from(duration_ms));
    parsed
        .attributes
        .insert("db.statement".into(), Value::from(statement.as_str()));
    Some(format!("Slow query {duration_ms:.0} ms: {statement}"))
}

#[cfg(test)]
mod tests {
    use crate::analytics::log_parse::tests::parse;

    #[test]
    fn reads_mysql_slow_query_blocks() {
        let block = parse(
            "# Time: 2026-10-28T01:50:02.123456Z\n# User@Host: app[app] @ localhost []  Id:     8\n# Query_time: 2.502010  Lock_time: 0.000123 Rows_sent: 1  Rows_examined: 500000\nSET timestamp=1793152202;\nSELECT * FROM links\nWHERE owner_id = 42;",
        );
        assert_eq!(
            block.message,
            "Slow query 2502 ms: SELECT * FROM links WHERE owner_id = 42;"
        );
        assert_eq!(block.severity, Some(13));
        assert_eq!(block.at, Some(1_793_152_202_123));
        assert_eq!(block.attributes["duration_ms"], 2502.01);
        assert_eq!(block.attributes["mysql.rows_examined"], 500_000.0);
        assert_eq!(block.attributes["user"], "app");
        assert_eq!(block.attributes["client.address"], "localhost");
    }
}
