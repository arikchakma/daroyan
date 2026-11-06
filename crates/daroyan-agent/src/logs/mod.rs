mod docker;
mod file;
mod follow;
mod journal;
mod parse;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use daroyan_protocol::log::{LogBatch, LogFormat, LogLine};
use log::warn;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::check::docker_events::DockerEvents;
use crate::config::{Config, DockerTarget, Source};
use crate::logs::docker::{DockerLogs, DockerSource};
use crate::logs::file::FilePosition;
use crate::logs::follow::{FollowedLine, LineQueue, Position};
use crate::logs::journal::{JournalSource, follow_units, start_position};
use crate::logs::parse::{RawLine, to_log_line};

pub use crate::logs::follow::LogFollowers;

const TICK: Duration = Duration::from_millis(500);
const POLL_EVERY: Duration = Duration::from_secs(2);
const IDLE_FLUSH: Duration = Duration::from_secs(1);
const SAVE_EVERY: Duration = Duration::from_secs(5);
const BUDGET_WINDOW: Duration = Duration::from_secs(60);
const LINE_OVERHEAD: u64 = 64;

struct LogSource {
    name: String,
    format: LogFormat,
    kind: SourceKind,
}

enum SourceKind {
    Docker { target: DockerTarget },
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
    used: Vec<u64>,
    window_start: Instant,
}

impl Budget {
    fn new(limit: u64, sources: usize) -> Self {
        Self {
            limit,
            used: vec![0; sources.max(1)],
            window_start: Instant::now(),
        }
    }

    fn allow(&mut self, source: usize, line: &RawLine) -> bool {
        if self.window_start.elapsed() >= BUDGET_WINDOW {
            self.window_start = Instant::now();
            self.used.fill(0);
        }

        let share = self.limit / self.used.len() as u64;
        let size = line.text.len() as u64 + LINE_OVERHEAD;
        let Some(used) = self.used.get_mut(source) else {
            return false;
        };
        if *used + size > share {
            return false;
        }

        *used += size;
        true
    }

    fn admit(&mut self, lines: Vec<(usize, RawLine)>) -> (Vec<(usize, RawLine)>, u64) {
        let (kept, dropped): (Vec<_>, Vec<_>) = lines
            .into_iter()
            .partition(|(source, line)| self.allow(*source, line));

        (kept, dropped.len() as u64)
    }
}

#[derive(Default)]
struct Held {
    line: Option<RawLine>,
    read_at: Option<Instant>,
}

impl Held {
    fn take_ready(
        &mut self,
        read: Vec<RawLine>,
        format: LogFormat,
        now: Instant,
    ) -> Vec<RawLine> {
        if !read.is_empty() {
            self.read_at = Some(now);
            return take_ready_lines(&mut self.line, read, format);
        }

        let is_idle = self
            .read_at
            .is_some_and(|read_at| now.duration_since(read_at) >= IDLE_FLUSH);
        if is_idle {
            return self.line.take().into_iter().collect();
        }

        Vec::new()
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
        let is_header = text.starts_with("# ") && !text.starts_with("# Time: ");
        let has_query_time =
            previous.lines().any(|line| line.starts_with("# Query_time: "));

        return !is_done && !starts_with_timestamp(text) && (is_header || has_query_time);
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

fn starts_with_timestamp(text: &str) -> bool {
    let text = text.strip_prefix('[').unwrap_or(text);

    text.starts_with(|character: char| character.is_ascii_digit())
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
            previous.severity = previous.severity.max(line.severity);
            previous.text.push('\n');
            previous.text.push_str(line.text.trim_end());
            if line.position.is_some() {
                previous.position = line.position;
            }
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
            Source::Docker(target) => SourceKind::Docker { target: target.clone() },
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
        SourceKind::Docker { .. } | SourceKind::Journal { .. } => Vec::new(),
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

pub fn collect_logs(
    config: &Config,
    batches: &Sender<LogBatch>,
    followers: &LogFollowers,
    events: Option<DockerEvents>,
) {
    let sources = list_sources(config);
    let positions_path = config.agent.state_path.join("log_positions.json");
    let mut positions = Positions::load(&positions_path);
    let mut saved = positions.clone();
    let mut sent = positions.clone();
    let mut saved_at = Instant::now();

    let docker_sources = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| match &source.kind {
            SourceKind::Docker { target } => {
                Some(DockerSource { source: index, target: target.clone() })
            }
            _ => None,
        })
        .collect();
    let queue = LineQueue::default();
    let mut docker =
        DockerLogs::new(docker_sources, queue.sink(), followers.clone(), events);
    let journal_sources = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| match &source.kind {
            SourceKind::Journal { unit } => Some(JournalSource {
                source: index,
                unit: unit.clone(),
                from: positions
                    .journal
                    .entry(unit.clone())
                    .or_insert_with(|| start_position(unix_now().as_secs()))
                    .clone(),
            }),
            _ => None,
        })
        .collect();
    let _journal = follow_units(journal_sources, &queue.sink(), followers);
    let mut streamed: HashMap<(usize, Arc<str>), Held> = HashMap::new();
    let mut polled: Vec<Option<RawLine>> = sources.iter().map(|_| None).collect();
    let mut polled_at = Instant::now();
    let mut budget = Budget::new(config.agent.log_budget, sources.len());

    loop {
        let is_stopping = followers.is_stopped();
        if !is_stopping {
            docker.sync(&mut positions.docker, to_nanos(unix_now()));
        }
        let followed = queue.receive(Instant::now() + TICK);
        let now = Instant::now();

        let mut ready =
            take_streamed(&mut streamed, followed, &mut positions, &sources, now);
        if now.duration_since(polled_at) >= POLL_EVERY {
            polled_at = now;
            let unix = unix_now();
            for (index, source) in sources.iter().enumerate() {
                let read = read_source(source, &mut positions, unix);
                ready.extend(
                    take_ready_lines(&mut polled[index], read, source.format)
                        .into_iter()
                        .map(|line| (index, line)),
                );
            }
        }
        if is_stopping {
            ready.extend(flush_held(
                &mut streamed,
                &mut polled,
                &mut positions,
                &sources,
            ));
        }

        let (ready, dropped) = budget.admit(ready);
        let lines: Vec<LogLine> = ready
            .into_iter()
            .map(|(index, raw)| {
                to_log_line(&sources[index].name, sources[index].format, raw)
            })
            .collect();
        if (!lines.is_empty() || dropped > 0)
            && batches.blocking_send(LogBatch { lines, dropped }).is_err()
        {
            if sent != saved {
                sent.save(&positions_path);
            }
            return;
        }
        sent.clone_from(&positions);
        if is_stopping {
            break;
        }

        if positions != saved && saved_at.elapsed() >= SAVE_EVERY {
            positions.save(&positions_path);
            saved = positions.clone();
            saved_at = Instant::now();
        }
    }

    if positions != saved {
        positions.save(&positions_path);
    }
}

fn take_streamed(
    streamed: &mut HashMap<(usize, Arc<str>), Held>,
    followed: Vec<FollowedLine>,
    positions: &mut Positions,
    sources: &[LogSource],
    now: Instant,
) -> Vec<(usize, RawLine)> {
    let mut reads: HashMap<(usize, Arc<str>), Vec<RawLine>> = HashMap::new();
    for line in followed {
        reads.entry((line.source, line.key)).or_default().push(line.raw);
    }
    for key in reads.keys() {
        streamed.entry(key.clone()).or_default();
    }

    let mut ready = Vec::new();
    for (key, held) in streamed.iter_mut() {
        let mut read = reads.remove(key).unwrap_or_default();
        read.sort_by_key(|line| line.position.as_ref().and_then(Position::nanos));

        let emitted = held.take_ready(read, sources[key.0].format, now);
        commit_position(positions, &sources[key.0], &key.1, &emitted);
        ready.extend(emitted.into_iter().map(|line| (key.0, line)));
    }
    streamed.retain(|_, held| held.line.is_some());
    ready.sort_by_key(|(_, line)| line.at);

    ready
}

fn flush_held(
    streamed: &mut HashMap<(usize, Arc<str>), Held>,
    polled: &mut [Option<RawLine>],
    positions: &mut Positions,
    sources: &[LogSource],
) -> Vec<(usize, RawLine)> {
    let mut ready = Vec::new();
    for ((source, container), held) in streamed.drain() {
        let emitted: Vec<RawLine> = held.line.into_iter().collect();
        commit_position(positions, &sources[source], &container, &emitted);
        ready.extend(emitted.into_iter().map(|line| (source, line)));
    }
    for (index, held) in polled.iter_mut().enumerate() {
        ready.extend(held.take().map(|line| (index, line)));
    }

    ready
}

fn commit_position(
    positions: &mut Positions,
    source: &LogSource,
    key: &str,
    emitted: &[RawLine],
) {
    match &source.kind {
        SourceKind::Docker { target } => {
            let Some(newest) = emitted
                .iter()
                .filter_map(|line| line.position.as_ref()?.nanos())
                .max()
            else {
                return;
            };
            let position = positions
                .docker
                .entry(target.to_string())
                .or_default()
                .entry(key.to_string())
                .or_default();
            *position = (*position).max(newest);
        }
        SourceKind::Journal { unit } => {
            let cursor = emitted.iter().rev().find_map(|line| match &line.position {
                Some(Position::Cursor(cursor)) => Some(cursor),
                _ => None,
            });
            if let Some(cursor) = cursor {
                positions.journal.insert(unit.clone(), cursor.to_string());
            }
        }
        SourceKind::File { .. } => {}
    }
}

fn unix_now() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}

fn to_nanos(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use daroyan_protocol::log::{LogFormat, LogStream};

    use std::collections::HashMap;
    use std::sync::Arc;

    use super::{
        Budget, FollowedLine, Held, IDLE_FLUSH, LogSource, Position, Positions, RawLine,
        SourceKind, flush_held, join_continuations, resolve_format, take_ready_lines,
        take_streamed,
    };
    use crate::config::DockerTarget;

    fn raw(stream: LogStream, text: &str) -> RawLine {
        RawLine {
            at: 1,
            stream,
            text: text.to_string(),
            severity: None,
            position: None,
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
    fn keeps_records_after_a_lone_slow_query_header_apart() {
        let lines = [
            "2026-11-05 10:00:00 INFO login failed for alice",
            "# User@Host: x [x] @ x []",
            "2026-11-05 10:00:01 ERROR db down: connection refused",
            "2026-11-05 10:00:02 ERROR payment worker crashed",
        ];
        let joined = join_continuations(
            lines.iter().map(|text| raw(LogStream::File, text)).collect(),
            LogFormat::Auto,
        );
        assert_eq!(joined.len(), 4);

        let journal =
            [("# User@Host: x [x] @ x []", 9), ("db down", 17), ("worker crashed", 17)]
                .into_iter()
                .map(|(text, severity)| RawLine {
                    severity: Some(severity),
                    ..raw(LogStream::Journal, text)
                })
                .collect();
        let joined = join_continuations(journal, LogFormat::Auto);
        assert_eq!(joined.len(), 3);
        assert_eq!(joined[1].severity, Some(17));
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

    #[test]
    fn flushes_a_held_entry_once_its_stream_goes_quiet() {
        let start = Instant::now();
        let mut held = Held::default();

        let first = held.take_ready(
            vec![
                raw(LogStream::Stderr, "Error: read ECONNRESET"),
                raw(LogStream::Stderr, "    at TLSSocket.onClose"),
            ],
            LogFormat::Auto,
            start,
        );
        assert!(first.is_empty());

        let early = start + IDLE_FLUSH - Duration::from_millis(1);
        assert!(held.take_ready(Vec::new(), LogFormat::Auto, early).is_empty());

        let late = held.take_ready(
            vec![raw(LogStream::Stderr, "    at node:net:337:12")],
            LogFormat::Auto,
            early,
        );
        assert!(late.is_empty());

        let flushed = held.take_ready(Vec::new(), LogFormat::Auto, early + IDLE_FLUSH);
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].text.lines().count(), 3);
        assert!(held.line.is_none());
    }

    #[test]
    fn counts_lines_over_the_budget() {
        let mut budget = Budget::new(200, 1);
        let lines = (0..5)
            .map(|index| {
                (
                    0,
                    raw(
                        LogStream::Stdout,
                        &format!("GET /health 200 {index}ms {}", "x".repeat(10)),
                    ),
                )
            })
            .collect();

        let (kept, dropped) = budget.admit(lines);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped, 3);
    }

    #[test]
    fn keeps_a_fair_share_for_quiet_sources() {
        let mut budget = Budget::new(1000, 2);
        let line = |text: &str| raw(LogStream::Stdout, &text.repeat(36));

        let flood = (0..20).map(|_| (0, line("x"))).collect();
        let (kept, dropped) = budget.admit(flood);
        assert_eq!((kept.len(), dropped), (5, 15));

        let quiet = (0..5).map(|_| (1, line("y"))).collect();
        let (kept, dropped) = budget.admit(quiet);
        assert_eq!((kept.len(), dropped), (5, 0));
        assert!(!budget.allow(1, &line("y")));
    }

    #[test]
    fn saves_a_position_only_once_its_line_is_sent() {
        let sources = [
            LogSource {
                name: "api".to_string(),
                format: LogFormat::Auto,
                kind: SourceKind::Docker {
                    target: DockerTarget::Name("api".to_string()),
                },
            },
            LogSource {
                name: "nginx".to_string(),
                format: LogFormat::Auto,
                kind: SourceKind::Journal { unit: "nginx.service".to_string() },
            },
        ];
        let lines =
            [("Error: boom", 1), ("    at run (app.js:1:1)", 2), ("INFO next", 3)];
        let followed = lines
            .into_iter()
            .flat_map(|(text, number)| {
                [
                    FollowedLine {
                        source: 0,
                        key: Arc::from("c1"),
                        raw: RawLine {
                            position: Some(Position::Time(number)),
                            ..raw(LogStream::Stderr, text)
                        },
                    },
                    FollowedLine {
                        source: 1,
                        key: Arc::from("nginx.service"),
                        raw: RawLine {
                            position: Some(Position::Cursor(
                                format!("i={number}").into(),
                            )),
                            ..raw(LogStream::Journal, text)
                        },
                    },
                ]
            })
            .collect();
        let mut streamed = HashMap::new();
        let mut positions = Positions::default();

        let ready = take_streamed(
            &mut streamed,
            followed,
            &mut positions,
            &sources,
            Instant::now(),
        );
        assert_eq!(ready.len(), 2);
        assert!(ready.iter().all(|(_, line)| line.text.lines().count() == 2));
        assert_eq!(positions.docker["api"]["c1"], 2);
        assert_eq!(positions.journal["nginx.service"], "i=2");

        let flushed =
            flush_held(&mut streamed, &mut [None, None], &mut positions, &sources);
        assert_eq!(flushed.len(), 2);
        assert!(flushed.iter().all(|(_, line)| line.text == "INFO next"));
        assert_eq!(positions.docker["api"]["c1"], 3);
        assert_eq!(positions.journal["nginx.service"], "i=3");
        assert!(streamed.is_empty());
    }
}
