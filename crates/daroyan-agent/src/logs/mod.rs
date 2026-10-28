mod docker;
mod file;
mod journal;
mod parse;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use daroyan_protocol::log::{LogBatch, LogFormat, LogLine};
use log::warn;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::config::{Config, Source};
use crate::logs::file::FilePosition;
use crate::logs::parse::{RawLine, to_log_line};

const INTERVAL: Duration = Duration::from_secs(2);
const BUDGET_WINDOW: Duration = Duration::from_secs(60);
const LINE_OVERHEAD: u64 = 64;

struct LogSource {
    name: String,
    format: LogFormat,
    kind: SourceKind,
}

enum SourceKind {
    Docker { pattern: String },
    Journal { unit: String },
    File { path: PathBuf },
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

fn is_continuation(text: &str, format: LogFormat) -> bool {
    let is_more = text
        .strip_prefix("... ")
        .and_then(|rest| rest.strip_suffix(" more"))
        .is_some_and(|count| count.bytes().all(|byte| byte.is_ascii_digit()));
    let reads_php =
        matches!(format, LogFormat::Auto | LogFormat::Php | LogFormat::Laravel);
    let is_php_frame = reads_php
        && text
            .strip_prefix('#')
            .and_then(|rest| rest.split_once(' '))
            .is_some_and(|(number, frame)| {
                !number.is_empty()
                    && number.bytes().all(|byte| byte.is_ascii_digit())
                    && (frame.starts_with('/')
                        || frame.starts_with("{main}")
                        || frame.starts_with("[internal function]")
                        || frame.contains(".php("))
            });
    text.starts_with(' ')
        || text.starts_with('\t')
        || text.starts_with("Caused by: ")
        || is_more
        || is_php_frame
        || (reads_php
            && matches!(text.trim_end(), "[stacktrace]" | "Stack trace:" | "\"}"))
}

const TRACEBACK: &str = "Traceback (most recent call last):";
const CHAINED_EXCEPTIONS: [&str; 2] = [
    "During handling of the above exception, another exception occurred:",
    "The above exception was the direct cause of the following exception:",
];

fn continues_block(previous: &str, text: &str, format: LogFormat) -> bool {
    let first = previous.lines().next().unwrap_or_default();
    let last = previous.lines().last().unwrap_or_default();
    let reads_mysql = matches!(format, LogFormat::Auto | LogFormat::Mysql);
    let reads_python = matches!(format, LogFormat::Auto | LogFormat::Python);
    if reads_mysql && starts_slow_query(first) {
        let is_done = last.ends_with(';')
            && !last.starts_with("SET timestamp=")
            && !last.starts_with("use ");
        return !is_done && !text.starts_with("# Time: ");
    }
    if !reads_python || !previous.contains(TRACEBACK) {
        return false;
    }
    let text = text.trim_end();
    let last_written = previous
        .lines()
        .rev()
        .map(str::trim_end)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let after_chain = CHAINED_EXCEPTIONS.contains(&last_written);
    if text.is_empty() {
        return after_chain || is_exception_line(last_written);
    }
    let after_frame = last.starts_with(' ') || last.starts_with('\t');
    CHAINED_EXCEPTIONS.contains(&text)
        || (after_chain && text == TRACEBACK)
        || (after_frame && is_exception_line(text))
}

fn starts_slow_query(line: &str) -> bool {
    let is_mysql_time = line.strip_prefix("# Time: ").is_some_and(|time| {
        time.len() >= 4 && time.bytes().take(4).all(|byte| byte.is_ascii_digit())
    });
    is_mysql_time || line.starts_with("# User@Host: ")
}

fn is_exception_line(text: &str) -> bool {
    let name = text.split(':').next().unwrap_or_default();
    let is_name = !name.is_empty()
        && name.starts_with(|character: char| character.is_ascii_alphabetic())
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_')
        });
    let is_class = name.rsplit('.').next().is_some_and(|class| {
        class.starts_with(|character: char| character.is_ascii_uppercase())
    });
    is_name && (text.contains(": ") || is_class)
}

fn join_continuations(lines: Vec<RawLine>, format: LogFormat) -> Vec<RawLine> {
    let mut joined: Vec<RawLine> = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some(previous) = joined
            .iter_mut()
            .rev()
            .find(|previous| previous.stream == line.stream)
            && ((!line.text.trim().is_empty() && is_continuation(&line.text, format))
                || continues_block(&previous.text, &line.text, format))
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

fn take_ready_lines(
    held: &mut Option<RawLine>,
    read: Vec<RawLine>,
    format: LogFormat,
) -> Vec<RawLine> {
    if read.is_empty() {
        return held.take().into_iter().collect();
    }
    let mut lines =
        join_continuations(held.take().into_iter().chain(read).collect(), format);
    *held = lines.pop();
    lines
}

fn resolve_format(source: &str, name: Option<&str>) -> LogFormat {
    let Some(name) = name else {
        return LogFormat::Auto;
    };
    name.parse().unwrap_or_else(|error| {
        warn!("{source}: {error}. Its lines are read with auto");
        LogFormat::Auto
    })
}

fn list_sources(config: &Config) -> Vec<LogSource> {
    let processes = config.processes.iter().filter_map(|check| {
        let settings = check.logs.as_ref()?;
        let kind = match &check.source {
            Source::Docker(pattern) => SourceKind::Docker { pattern: pattern.clone() },
            Source::Systemd(unit) => SourceKind::Journal { unit: unit.clone() },
            Source::Process(_) => return None,
        };
        Some(LogSource {
            name: check.name.clone(),
            format: resolve_format(&check.name, settings.format.as_deref()),
            kind,
        })
    });
    let files = config.files.iter().map(|file| LogSource {
        name: file.name.clone(),
        format: resolve_format(&file.name, file.format.as_deref()),
        kind: SourceKind::File { path: file.path.clone() },
    });
    processes.chain(files).collect()
}

fn read_source(
    source: &LogSource,
    positions: &mut Positions,
    now: Duration,
) -> Vec<RawLine> {
    match &source.kind {
        SourceKind::Docker { pattern } => {
            let mut containers = positions.docker.get(pattern).cloned();
            let lines = docker::read_docker(
                pattern,
                &mut containers,
                i64::try_from(now.as_nanos()).unwrap_or(i64::MAX),
            );
            if let Some(containers) = containers {
                positions.docker.insert(pattern.clone(), containers);
            }
            lines
        }
        SourceKind::Journal { unit } => {
            let mut cursor = positions.journal.get(unit).cloned();
            let lines = journal::read_journal(unit, &mut cursor, now.as_secs());
            if let Some(cursor) = cursor {
                positions.journal.insert(unit.clone(), cursor);
            }
            lines
        }
        SourceKind::File { path } => {
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
            lines
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
            let raw_lines = read_source(source, &mut positions, now);
            for raw in take_ready_lines(&mut held[index], raw_lines, source.format) {
                let line = to_log_line(&source.name, source.format, raw);
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
    use daroyan_protocol::log::{LogFormat, LogStream};

    use super::{RawLine, join_continuations, resolve_format, take_ready_lines};

    fn raw(stream: LogStream, text: &str) -> RawLine {
        RawLine {
            at: 1,
            stream,
            text: text.to_string(),
            severity: None,
        }
    }

    #[test]
    fn joins_mysql_slow_query_blocks() {
        let lines = [
            "# Time: 2026-10-28T01:50:02.123456Z",
            "# User@Host: app[app] @ localhost []  Id:     8",
            "# Query_time: 2.502010  Lock_time: 0.000123 Rows_sent: 1  Rows_examined: 500000",
            "SET timestamp=1793152202;",
            "SELECT * FROM links",
            "WHERE owner_id = 42;",
            "# Time: 2026-10-28T01:50:09.000000Z",
            "# User@Host: app[app] @ localhost []  Id:     9",
        ];
        let joined = join_continuations(
            lines.iter().map(|text| raw(LogStream::Stdout, text)).collect(),
            LogFormat::Auto,
        );
        assert_eq!(joined.len(), 2);
        assert!(joined[0].text.ends_with("WHERE owner_id = 42;"));
        assert!(joined[1].text.starts_with("# Time: 2026-10-28T01:50:09"));
    }

    #[test]
    fn joins_python_tracebacks_with_their_exception() {
        let lines = [
            "Traceback (most recent call last):",
            "  File \"/app/worker.py\", line 12, in run",
            "    connect()",
            "ConnectionRefusedError: [Errno 111] Connection refused",
            "",
            "During handling of the above exception, another exception occurred:",
            "",
            "Traceback (most recent call last):",
            "  File \"/app/worker.py\", line 14, in run",
            "    raise JobFailed()",
            "app.errors.JobFailed",
            "INFO:root:next job",
        ];
        let joined = join_continuations(
            lines.iter().map(|text| raw(LogStream::Stderr, text)).collect(),
            LogFormat::Auto,
        );
        assert_eq!(joined.len(), 2);
        assert!(joined[0].text.ends_with("app.errors.JobFailed"));
        assert_eq!(joined[1].text, "INFO:root:next job");
    }

    #[test]
    fn joins_php_stack_traces() {
        let lines = [
            "[2026-10-28 01:50:02] production.ERROR: boom {\"exception\":\"[object] (Exception(code: 0): boom at /app.php:3)",
            "[stacktrace]",
            "#0 /var/www/app.php(12): run()",
            "#1 {main}",
            "\"} ",
            "[2026-10-28 01:50:03] production.INFO: next",
        ];
        let joined = join_continuations(
            lines.iter().map(|text| raw(LogStream::Stdout, text)).collect(),
            LogFormat::Auto,
        );
        assert_eq!(joined.len(), 2);
        assert!(joined[0].text.ends_with("#1 {main}\n\"}"));
    }

    #[test]
    fn leaves_lookalike_lines_alone() {
        let lines = [
            "# Time: 12ms build step",
            "GET /health 200 1ms",
            "#1 [internal] load build definition",
            "#2 [internal] load .dockerignore",
        ];
        let joined = join_continuations(
            lines.iter().map(|text| raw(LogStream::Stdout, text)).collect(),
            LogFormat::Auto,
        );
        assert_eq!(joined.len(), 4);
    }

    #[test]
    fn joins_stack_trace_lines() {
        let joined = join_continuations(
            vec![
                raw(LogStream::Stderr, "Error: read ECONNRESET"),
                raw(LogStream::Stdout, "GET /v1/links 200 3ms"),
                raw(
                    LogStream::Stderr,
                    "    at TLSSocket.onClose (node:_tls_wrap:1097:12)",
                ),
                raw(LogStream::Stderr, "    at node:net:337:12"),
                raw(LogStream::Stdout, "GET /health 200 1ms"),
                raw(LogStream::Stdout, "  indented but first of its stream"),
            ],
            LogFormat::Auto,
        );
        let java = join_continuations(
            vec![
                raw(LogStream::Stderr, "java.lang.IllegalStateException: pool exhausted"),
                raw(LogStream::Stderr, "\tat com.example.Foo.bar(Foo.java:42)"),
                raw(LogStream::Stderr, "Caused by: java.net.SocketException: reset"),
                raw(LogStream::Stderr, "\tat java.net.Socket.read(Socket.java:10)"),
                raw(LogStream::Stderr, "... 12 more"),
                raw(LogStream::Stderr, "2024-01-15 INFO next entry"),
            ],
            LogFormat::Auto,
        );
        assert_eq!(java.len(), 2);
        assert_eq!(java[0].text.lines().count(), 5);
        assert_eq!(joined.len(), 3);
        assert_eq!(joined[0].text.lines().count(), 3);
        assert_eq!(joined[2].text.lines().count(), 2);
    }

    #[test]
    fn joins_only_for_the_configured_format() {
        let lines = || {
            vec![
                raw(LogStream::Stderr, "Traceback (most recent call last):"),
                raw(LogStream::Stderr, "  File \"app.py\", line 3, in <module>"),
                raw(LogStream::Stderr, "ValueError: bad"),
            ]
        };
        assert_eq!(join_continuations(lines(), LogFormat::Python).len(), 1);
        assert_eq!(join_continuations(lines(), LogFormat::Nginx).len(), 2);
        assert_eq!(resolve_format("App", Some("php-fpm")), LogFormat::PhpFpm);
        assert_eq!(resolve_format("App", Some("ngnix")), LogFormat::Auto);
        assert_eq!(resolve_format("App", None), LogFormat::Auto);
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
            LogFormat::Auto,
        );
        assert_eq!(first.len(), 1);
        let second = take_ready_lines(
            &mut held,
            vec![
                raw(LogStream::Stderr, "    at TLSSocket.onClose"),
                raw(LogStream::Stdout, "GET /v1/links 200 3ms"),
            ],
            LogFormat::Auto,
        );
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].text.lines().count(), 2);
        let flushed = take_ready_lines(&mut held, Vec::new(), LogFormat::Auto);
        assert_eq!(flushed[0].text, "GET /v1/links 200 3ms");
        assert!(held.is_none());
    }
}
