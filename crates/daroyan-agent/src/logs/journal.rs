use std::ffi::OsString;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use daroyan_protocol::log::{
    LogStream, SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN,
};
use log::{info, warn};
use serde_json::Value;

use crate::logs::follow::{Feed, Follower, LogFollowers, Position, Sink};
use crate::logs::parse::RawLine;

const SINCE_PREFIX: &str = "since:";
const MAX_LINE_BYTES: usize = 1024 * 1024;
const BAD_CURSOR: &str = "Failed to seek to cursor";

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

pub fn start_position(now_secs: u64) -> String {
    format!("{SINCE_PREFIX}{now_secs}")
}

struct UnitFeed {
    unit: String,
    is_catching_up: AtomicBool,
    has_caught_up: AtomicBool,
}

impl UnitFeed {
    fn args(&self, from: Option<&Position>) -> Vec<OsString> {
        let mut args: Vec<OsString> =
            ["--unit", &self.unit, "--all", "--output", "json", "--no-pager", "--quiet"]
                .map(OsString::from)
                .into();
        let is_catching_up =
            from.is_some() && self.is_catching_up.swap(false, Ordering::AcqRel);
        if is_catching_up {
            self.has_caught_up.store(true, Ordering::Release);
        } else {
            args.push("--follow".into());
        }

        let cursor = match from {
            Some(Position::Cursor(cursor)) => Some(&**cursor),
            _ => None,
        };
        match cursor.map(|cursor| (cursor, cursor.strip_prefix(SINCE_PREFIX))) {
            Some((_, Some(seconds))) => {
                args.extend([
                    "--no-tail".into(),
                    "--since".into(),
                    format!("@{seconds}").into(),
                ]);
            }
            Some((cursor, None)) => {
                args.extend(["--no-tail".into(), "--after-cursor".into(), cursor.into()]);
            }
            None => args.extend(["--lines".into(), "0".into()]),
        }

        args
    }

    fn new(unit: String) -> Self {
        Self {
            unit,
            is_catching_up: AtomicBool::new(true),
            has_caught_up: AtomicBool::new(false),
        }
    }
}

impl Feed for UnitFeed {
    fn name(&self) -> String {
        format!("journalctl {}", self.unit)
    }

    fn command(&self, from: Option<&Position>, _sudo: bool) -> Command {
        let mut command = Command::new("journalctl");
        command.args(self.args(from));
        command
    }

    fn read(&self, line: &[u8], _stream: LogStream) -> Option<RawLine> {
        let entry: Value = serde_json::from_slice(line).ok()?;
        let cursor = entry.get("__CURSOR")?.as_str()?;
        let micros = entry
            .get("__REALTIME_TIMESTAMP")
            .and_then(Value::as_str)
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or_default();

        Some(RawLine {
            at: micros / 1_000,
            stream: LogStream::Journal,
            text: read_message(entry.get("MESSAGE")),
            severity: read_priority(entry.get("PRIORITY")),
            position: Some(Position::Cursor(Arc::from(cursor))),
        })
    }

    fn max_line_bytes(&self) -> usize {
        MAX_LINE_BYTES
    }

    fn reads_stderr(&self) -> bool {
        false
    }

    fn can_use_sudo(&self) -> bool {
        false
    }

    fn is_live(&self) -> bool {
        true
    }

    fn recover_from(&self, error: &str) -> Option<Position> {
        error
            .contains(BAD_CURSOR)
            .then(|| Position::Cursor(start_position(now_secs()).into()))
    }

    fn restarts_at_once(&self) -> bool {
        self.has_caught_up.swap(false, Ordering::AcqRel)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct JournalSource {
    pub source: usize,
    pub unit: String,
    pub from: String,
}

pub fn follow_units(
    sources: Vec<JournalSource>,
    sink: &Sink,
    group: &LogFollowers,
) -> Vec<Follower> {
    sources
        .into_iter()
        .filter_map(|JournalSource { source, unit, from }| {
            let key = Arc::from(unit.as_str());
            let feed = UnitFeed::new(unit.clone());
            let from = Some(Position::Cursor(Arc::from(from)));

            Follower::start(
                feed,
                source,
                key,
                from,
                sink.clone(),
                group.clone(),
                Arc::default(),
            )
            .inspect(|_| info!("following the journal of {unit}"))
            .inspect_err(|error| warn!("could not follow the journal of {unit}: {error}"))
            .ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::LogStream;
    use serde_json::json;

    use super::{UnitFeed, read_message, read_priority, start_position};
    use crate::logs::follow::{Feed, Position};

    #[test]
    fn reads_journal_fields() {
        assert_eq!(read_message(Some(&json!("started"))), "started");
        assert_eq!(read_message(Some(&json!([104, 105]))), "hi");
        assert_eq!(read_priority(Some(&json!("3"))), Some(17));
        assert_eq!(read_priority(Some(&json!("6"))), Some(9));
    }

    #[test]
    fn reads_entries_with_their_cursor() {
        let feed = UnitFeed::new("nginx.service".to_string());
        let entry = br#"{"__CURSOR":"s=abc;i=2","__REALTIME_TIMESTAMP":"1793959201500000","MESSAGE":[104,105],"PRIORITY":"4"}"#;

        let line = feed.read(entry, LogStream::Stdout).unwrap();
        assert_eq!(line.at, 1_793_959_201_500);
        assert_eq!(line.text, "hi");
        assert_eq!(line.severity, Some(13));
        assert_eq!(line.stream, LogStream::Journal);
        assert_eq!(line.position, Some(Position::Cursor("s=abc;i=2".into())));
        assert!(feed.read(b"-- No entries --", LogStream::Stdout).is_none());
        assert!(feed.read(br#"{"MESSAGE":"no cursor"}"#, LogStream::Stdout).is_none());
    }

    #[test]
    fn catches_up_before_following_and_recovers_from_a_bad_cursor() {
        let feed = UnitFeed::new("nginx.service".to_string());
        let args = |from: Option<Position>| -> Vec<String> {
            feed.args(from.as_ref())
                .into_iter()
                .skip(7)
                .map(|arg| arg.into_string().unwrap())
                .collect()
        };
        let cursor = Some(Position::Cursor("s=abc;i=2".into()));

        assert_eq!(args(cursor.clone()), ["--no-tail", "--after-cursor", "s=abc;i=2"]);
        assert!(feed.restarts_at_once());
        assert!(!feed.restarts_at_once());
        assert_eq!(
            args(cursor),
            ["--follow", "--no-tail", "--after-cursor", "s=abc;i=2"]
        );
        assert_eq!(
            args(Some(Position::Cursor(start_position(1_793_959_201).into()))),
            ["--follow", "--no-tail", "--since", "@1793959201"]
        );
        assert_eq!(args(None), ["--follow", "--lines", "0"]);

        let recovered = feed.recover_from("Failed to seek to cursor: Invalid argument");
        assert!(
            matches!(recovered, Some(Position::Cursor(cursor)) if cursor.starts_with("since:"))
        );
        assert!(feed.recover_from("Failed to open journal").is_none());
    }
}
