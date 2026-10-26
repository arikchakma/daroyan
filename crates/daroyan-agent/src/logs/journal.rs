use std::process::Command;

use daroyan_protocol::log::{
    LogStream, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN,
};
use serde_json::Value;

use crate::logs::parse::RawLine;

const SINCE_PREFIX: &str = "since:";

fn read_message(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(bytes)) => {
            let bytes: Vec<u8> = bytes
                .iter()
                .filter_map(|byte| u8::try_from(byte.as_u64()?).ok())
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
        _ => String::new(),
    }
}

fn read_priority(value: Option<&Value>) -> Option<u8> {
    match value?.as_str()?.parse::<u8>().ok()? {
        0..=2 => Some(21),
        3 => Some(SEVERITY_ERROR),
        4 => Some(SEVERITY_WARN),
        5 | 6 => Some(SEVERITY_INFO),
        _ => Some(SEVERITY_DEBUG),
    }
}

fn run_journalctl(unit: &str, position: &str) -> Option<String> {
    let mut command = Command::new("journalctl");
    command.args(["--unit", unit, "--output", "json", "--no-pager", "--quiet"]);
    match position.strip_prefix(SINCE_PREFIX) {
        Some(seconds) => command.args(["--since", &format!("@{seconds}")]),
        None => command.args(["--after-cursor", position]),
    };
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn read_journal(
    unit: &str,
    position: &mut Option<String>,
    now_secs: u64,
) -> Vec<RawLine> {
    let Some(current) = position.clone() else {
        *position = Some(format!("{SINCE_PREFIX}{now_secs}"));
        return Vec::new();
    };
    let Some(text) = run_journalctl(unit, &current) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for entry in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        if let Some(cursor) = entry.get("__CURSOR").and_then(Value::as_str) {
            *position = Some(cursor.to_string());
        }
        let micros = entry
            .get("__REALTIME_TIMESTAMP")
            .and_then(Value::as_str)
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or_default();
        lines.push(RawLine {
            at: micros / 1_000,
            stream: LogStream::Journal,
            text: read_message(entry.get("MESSAGE")),
            severity: read_priority(entry.get("PRIORITY")),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{read_message, read_priority};

    #[test]
    fn reads_journal_fields() {
        assert_eq!(read_message(Some(&json!("started"))), "started");
        assert_eq!(read_message(Some(&json!([104, 105]))), "hi");
        assert_eq!(read_priority(Some(&json!("3"))), Some(17));
        assert_eq!(read_priority(Some(&json!("6"))), Some(9));
    }
}
