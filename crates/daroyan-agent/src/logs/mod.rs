mod docker;
mod file;
mod journal;
mod parse;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use daroyan_protocol::log::{LogBatch, LogLine};
use log::warn;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::config::{Config, Source};
use crate::logs::file::FilePosition;
use crate::logs::parse::{RawLine, to_log_line};

const INTERVAL: Duration = Duration::from_secs(2);
const BUDGET_WINDOW: Duration = Duration::from_secs(60);
const LINE_OVERHEAD: u64 = 64;

enum LogSource {
    Docker { name: String, pattern: String },
    Journal { name: String, unit: String },
    File { name: String, path: PathBuf },
}

#[derive(Serialize, Deserialize, Default, PartialEq, Clone)]
struct Positions {
    #[serde(default)]
    docker: HashMap<String, HashMap<String, i64>>,
    #[serde(default)]
    journal: HashMap<String, String>,
    #[serde(default)]
    files: HashMap<String, FilePosition>,
}

impl Positions {
    fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) {
        let partial = path.with_extension("json.partial");
        let saved = serde_json::to_vec(self)
            .map_err(std::io::Error::other)
            .and_then(|bytes| fs::write(&partial, bytes))
            .and_then(|()| fs::rename(&partial, path));
        if let Err(error) = saved {
            warn!("could not save the log positions: {error}");
        }
    }
}

struct Budget {
    limit: u64,
    used: u64,
    window_start: Instant,
}

impl Budget {
    fn allow(&mut self, line: &LogLine) -> bool {
        if self.window_start.elapsed() >= BUDGET_WINDOW {
            self.window_start = Instant::now();
            self.used = 0;
        }
        let size = line.message.len() as u64 + LINE_OVERHEAD;
        if self.used + size > self.limit {
            return false;
        }
        self.used += size;
        true
    }
}

const MAX_JOINED_BYTES: usize = 16 * 1024;

fn is_continuation(text: &str) -> bool {
    let is_more = text
        .strip_prefix("... ")
        .and_then(|rest| rest.strip_suffix(" more"))
        .is_some_and(|count| count.bytes().all(|byte| byte.is_ascii_digit()));
    text.starts_with(' ')
        || text.starts_with('\t')
        || text.starts_with("Caused by: ")
        || is_more
}

fn join_continuations(lines: Vec<RawLine>) -> Vec<RawLine> {
    let mut joined: Vec<RawLine> = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some(previous) = joined
            .iter_mut()
            .rev()
            .find(|previous| previous.stream == line.stream)
            && is_continuation(&line.text)
            && !line.text.trim().is_empty()
            && previous.text.len() + line.text.len() < MAX_JOINED_BYTES
        {
            previous.text.push('\n');
            previous.text.push_str(line.text.trim_end());
            continue;
        }
        joined.push(line);
    }
    joined
}

fn take_ready_lines(held: &mut Option<RawLine>, read: Vec<RawLine>) -> Vec<RawLine> {
    if read.is_empty() {
        return held.take().into_iter().collect();
    }
    let mut lines = join_continuations(held.take().into_iter().chain(read).collect());
    *held = lines.pop();
    lines
}

fn list_sources(config: &Config) -> Vec<LogSource> {
    let processes = config.processes.iter().filter(|check| check.logs).filter_map(
        |check| match &check.source {
            Source::Docker(pattern) => Some(LogSource::Docker {
                name: check.name.clone(),
                pattern: pattern.clone(),
            }),
            Source::Systemd(unit) => {
                Some(LogSource::Journal { name: check.name.clone(), unit: unit.clone() })
            }
            Source::Process(_) => None,
        },
    );
    let files = config
        .files
        .iter()
        .map(|file| LogSource::File { name: file.name.clone(), path: file.path.clone() });
    processes.chain(files).collect()
}

fn read_source(
    source: &LogSource,
    positions: &mut Positions,
    now: Duration,
) -> (String, Vec<RawLine>) {
    match source {
        LogSource::Docker { name, pattern } => {
            let mut containers = positions.docker.get(pattern).cloned();
            let lines = docker::read_docker(
                pattern,
                &mut containers,
                i64::try_from(now.as_nanos()).unwrap_or(i64::MAX),
            );
            if let Some(containers) = containers {
                positions.docker.insert(pattern.clone(), containers);
            }
            (name.clone(), lines)
        }
        LogSource::Journal { name, unit } => {
            let mut cursor = positions.journal.get(unit).cloned();
            let lines = journal::read_journal(unit, &mut cursor, now.as_secs());
            if let Some(cursor) = cursor {
                positions.journal.insert(unit.clone(), cursor);
            }
            (name.clone(), lines)
        }
        LogSource::File { name, path } => {
            let key = path.to_string_lossy().into_owned();
            let mut position = positions.files.get(&key).copied();
            let lines = file::read_file(
                path,
                &mut position,
                u64::try_from(now.as_millis()).unwrap_or(u64::MAX),
            );
            if let Some(position) = position {
                positions.files.insert(key, position);
            }
            (name.clone(), lines)
        }
    }
}

pub fn collect_logs(config: &Config, batches: &Sender<LogBatch>) {
    let sources = list_sources(config);
    let positions_path = config.agent.state_path.join("log_positions.json");
    let mut positions = Positions::load(&positions_path);
    let mut held: Vec<Option<RawLine>> = sources.iter().map(|_| None).collect();
    let mut budget = Budget {
        limit: config.agent.log_budget,
        used: 0,
        window_start: Instant::now(),
    };
    loop {
        thread::sleep(INTERVAL);
        if sources.is_empty() {
            continue;
        }
        let before = positions.clone();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let mut lines = Vec::new();
        let mut dropped = 0;
        for (index, source) in sources.iter().enumerate() {
            let (name, raw_lines) = read_source(source, &mut positions, now);
            for raw in take_ready_lines(&mut held[index], raw_lines) {
                let line = to_log_line(&name, raw);
                if budget.allow(&line) {
                    lines.push(line);
                } else {
                    dropped += 1;
                }
            }
        }
        if (!lines.is_empty() || dropped > 0)
            && batches.blocking_send(LogBatch { lines, dropped }).is_err()
        {
            return;
        }
        if positions != before {
            positions.save(&positions_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::LogStream;

    use super::{RawLine, join_continuations, take_ready_lines};

    fn raw(stream: LogStream, text: &str) -> RawLine {
        RawLine {
            at: 1,
            stream,
            text: text.to_string(),
            severity: None,
        }
    }

    #[test]
    fn joins_stack_trace_lines() {
        let joined = join_continuations(vec![
            raw(LogStream::Stderr, "Error: read ECONNRESET"),
            raw(LogStream::Stdout, "GET /v1/links 200 3ms"),
            raw(LogStream::Stderr, "    at TLSSocket.onClose (node:_tls_wrap:1097:12)"),
            raw(LogStream::Stderr, "    at node:net:337:12"),
            raw(LogStream::Stdout, "GET /health 200 1ms"),
            raw(LogStream::Stdout, "  indented but first of its stream"),
        ]);
        let java = join_continuations(vec![
            raw(LogStream::Stderr, "java.lang.IllegalStateException: pool exhausted"),
            raw(LogStream::Stderr, "\tat com.example.Foo.bar(Foo.java:42)"),
            raw(LogStream::Stderr, "Caused by: java.net.SocketException: reset"),
            raw(LogStream::Stderr, "\tat java.net.Socket.read(Socket.java:10)"),
            raw(LogStream::Stderr, "... 12 more"),
            raw(LogStream::Stderr, "2024-01-15 INFO next entry"),
        ]);
        assert_eq!(java.len(), 2);
        assert_eq!(java[0].text.lines().count(), 5);
        assert_eq!(joined.len(), 3);
        assert_eq!(joined[0].text.lines().count(), 3);
        assert_eq!(joined[2].text.lines().count(), 2);
    }

    #[test]
    fn holds_the_last_line_for_late_continuations() {
        let mut held = None;
        let first = take_ready_lines(
            &mut held,
            vec![
                raw(LogStream::Stdout, "GET /health 200 1ms"),
                raw(LogStream::Stderr, "Error: read ECONNRESET"),
            ],
        );
        assert_eq!(first.len(), 1);
        let second = take_ready_lines(
            &mut held,
            vec![
                raw(LogStream::Stderr, "    at TLSSocket.onClose"),
                raw(LogStream::Stdout, "GET /v1/links 200 3ms"),
            ],
        );
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].text.lines().count(), 2);
        let flushed = take_ready_lines(&mut held, Vec::new());
        assert_eq!(flushed[0].text, "GET /v1/links 200 3ms");
        assert!(held.is_none());
    }
}
