use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use daroyan_protocol::log::LogStream;
use log::{info, warn};

use crate::check::docker_events::DockerEvents;
use crate::config::DockerTarget;
use crate::docker_read::{self, DockerRead, LogsRead};
use crate::logs::follow::{Feed, Follower, Launch, LogFollowers, Position, Sink, Watch};
use crate::logs::parse::RawLine;
use crate::service::is_container_running;

const RESYNC_EVERY: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(10);
const MAX_LINE_BYTES: usize = 64 * 1024;
const PARTIAL_BYTES: usize = 16 * 1024;
const MAX_STAMP_BYTES: usize = 40;
const DISCOVERY_TICK: Duration = Duration::from_millis(500);
const RESUME_TAIL: u32 = 10_000;
const WATCH_EVERY: Duration = Duration::from_secs(60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const CHECK_BYTES: u64 = 64 * 1024;

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn parse_timestamp(text: &str) -> Option<i64> {
    let (date, clock) = text.split_once('T')?;
    let clock = clock.strip_suffix('Z')?;
    let mut date_parts = date.split('-').map(str::parse::<i64>);
    let (year, month, day) =
        (date_parts.next()?.ok()?, date_parts.next()?.ok()?, date_parts.next()?.ok()?);
    let (time, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut time_parts = time.split(':').map(str::parse::<i64>);
    let (hour, minute, second) =
        (time_parts.next()?.ok()?, time_parts.next()?.ok()?, time_parts.next()?.ok()?);
    let digits = fraction.get(..fraction.len().min(9))?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let nanos = if digits.is_empty() {
        0
    } else {
        digits.parse::<i64>().ok()? * 10_i64.pow(9 - digits.len() as u32)
    };
    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(seconds * 1_000_000_000 + nanos)
}

fn format_since(nanos: i64) -> String {
    format!("{}.{:09}", nanos.div_euclid(1_000_000_000), nanos.rem_euclid(1_000_000_000))
}

fn split_timestamp(line: &[u8]) -> Option<(i64, &[u8])> {
    let space = line.iter().take(MAX_STAMP_BYTES).position(|byte| *byte == b' ');
    let (stamp, message) = match space {
        Some(index) => (&line[..index], &line[index + 1..]),
        None => (line, &[][..]),
    };

    Some((parse_timestamp(std::str::from_utf8(stamp).ok()?)?, message))
}

fn join_partials(message: &[u8]) -> String {
    if message.len() <= PARTIAL_BYTES {
        return String::from_utf8_lossy(message).into_owned();
    }

    let mut joined = Vec::with_capacity(message.len());
    let mut rest = message;
    while rest.len() > PARTIAL_BYTES {
        let (chunk, tail) = rest.split_at(PARTIAL_BYTES);
        let Some((_, after)) = split_timestamp(tail) else {
            break;
        };

        joined.extend_from_slice(chunk);
        rest = after;
    }
    joined.extend_from_slice(rest);

    String::from_utf8_lossy(&joined).into_owned()
}

fn short_id(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

struct ContainerFeed {
    container: Arc<str>,
    is_tailing: AtomicBool,
    wants_full_scan: AtomicBool,
}

impl ContainerFeed {
    fn request(&self, from: Option<&Position>) -> LogsRead {
        let since = from.and_then(Position::nanos).filter(|nanos| *nanos > 0);
        let is_tailing =
            since.is_some() && !self.wants_full_scan.swap(false, Ordering::AcqRel);
        self.is_tailing.store(is_tailing, Ordering::Release);

        LogsRead {
            container: self.container.to_string(),
            follow: true,
            tail: is_tailing.then_some(RESUME_TAIL),
            since: since.filter(|_| !is_tailing).map(format_since),
        }
    }

    fn new(container: Arc<str>) -> Self {
        Self {
            container,
            is_tailing: AtomicBool::new(false),
            wants_full_scan: AtomicBool::new(false),
        }
    }
}

impl Feed for ContainerFeed {
    fn name(&self) -> String {
        format!("docker logs {}", short_id(&self.container))
    }

    fn launch(&self, from: Option<&Position>, sudo: bool) -> Launch {
        Launch::Docker { read: DockerRead::Logs(self.request(from)), sudo }
    }

    fn read(&self, line: &[u8], stream: LogStream) -> Option<RawLine> {
        let (nanos, message) = split_timestamp(line)?;

        Some(RawLine {
            at: u64::try_from(nanos / 1_000_000).unwrap_or_default(),
            stream,
            text: join_partials(message),
            severity: None,
            position: Some(Position::Time(nanos)),
        })
    }

    fn max_line_bytes(&self) -> usize {
        MAX_LINE_BYTES
    }

    fn reads_stderr(&self) -> bool {
        true
    }

    fn can_use_sudo(&self) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        is_container_running(&self.container)
    }

    fn starts_after_gap(&self, first: &Position, from: Option<&Position>) -> bool {
        self.is_tailing.load(Ordering::Acquire)
            && first.nanos() > from.and_then(Position::nanos)
    }

    fn rescan(&self) {
        warn!(
            "{} wrote more than {RESUME_TAIL} lines since the saved time, reading from that time",
            self.name()
        );
        self.wants_full_scan.store(true, Ordering::Release);
    }
}

fn read_newest(container: &str, sudo: bool) -> Option<i64> {
    let newest = LogsRead {
        container: container.to_string(),
        tail: Some(1),
        ..LogsRead::default()
    };
    let mut stream = docker_read::open(&DockerRead::Logs(newest), sudo);
    let handle = stream.handle();
    let pipes = [stream.take_stdout(), stream.take_stderr()];
    let (output, is_done) = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut output = String::new();
            for mut pipe in pipes.into_iter().flatten() {
                let _ = pipe.by_ref().take(CHECK_BYTES).read_to_string(&mut output);
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            }
            output
        });
        let is_done = handle.wait_for(CHECK_TIMEOUT).is_some();
        if !is_done {
            warn!("docker logs --tail 1 for {} timed out", short_id(container));
            handle.close();
        }
        (reader.join().unwrap_or_default(), is_done)
    });
    if !is_done {
        return None;
    }

    output
        .lines()
        .filter_map(|line| split_timestamp(line.as_bytes()))
        .map(|(nanos, _)| nanos)
        .max()
}

pub struct DockerSource {
    pub source: usize,
    pub target: DockerTarget,
}

struct Snapshot {
    source: usize,
    key: String,
    states: Vec<(String, bool)>,
}

type Watches = Arc<Mutex<HashMap<Arc<str>, Watch>>>;

struct Check {
    delivered: Option<i64>,
    newest: Option<i64>,
}

impl Check {
    fn is_stuck(&self, delivered: Option<i64>) -> bool {
        delivered == self.delivered && self.newest > delivered
    }
}

struct Discovery {
    sources: Vec<DockerSource>,
    events: Option<DockerEvents>,
    snapshots: SyncSender<Snapshot>,
    group: LogFollowers,
    watches: Watches,
    sudo: Arc<AtomicBool>,
}

impl Discovery {
    fn run(self) {
        let mut seen_changes = None;
        let mut synced_at: Option<Instant> = None;
        let mut watched_at = Instant::now();
        let mut delivered = HashMap::new();

        while !self.group.is_stopped() {
            if watched_at.elapsed() >= WATCH_EVERY {
                watched_at = Instant::now();
                self.restart_stalled(&mut delivered);
            }

            let changes = self.events.as_ref().map_or(0, DockerEvents::changes);
            let is_due = synced_at.is_none_or(|at| at.elapsed() >= RESYNC_EVERY);
            if is_due || seen_changes != Some(changes) {
                seen_changes = Some(changes);
                synced_at = Some(Instant::now());
                if !self.list_all() {
                    return;
                }
            }

            thread::sleep(DISCOVERY_TICK);
        }
    }

    fn restart_stalled(&self, previous: &mut HashMap<Arc<str>, Check>) {
        let watches: Vec<(Arc<str>, Watch)> = lock(&self.watches)
            .iter()
            .map(|(id, watch)| (Arc::clone(id), watch.clone()))
            .collect();

        let mut current = HashMap::new();
        for (id, watch) in watches {
            let delivered = watch.delivered().as_ref().and_then(Position::nanos);
            let is_stuck =
                previous.get(&id).is_some_and(|before| before.is_stuck(delivered));
            if is_stuck {
                warn!(
                    "docker logs for {} stopped sending new lines, following again",
                    short_id(&id)
                );
                watch.restart();
            }

            let newest = read_newest(&id, self.sudo.load(Ordering::Acquire));
            current.insert(id, Check { delivered, newest });
        }

        *previous = current;
    }

    fn list_all(&self) -> bool {
        for source in &self.sources {
            let key = source.target.to_string();
            let states = match source.target.list_states() {
                Ok(states) => states,
                Err(error) => {
                    warn!("could not list containers for {key}: {error}");
                    continue;
                }
            };

            let snapshot = Snapshot { source: source.source, key, states };
            if self.snapshots.send(snapshot).is_err() {
                return false;
            }
        }

        true
    }
}

struct Followed {
    follower: Follower,
    gone_since: Option<Instant>,
}

pub struct DockerLogs {
    followers: HashMap<(usize, Arc<str>), Followed>,
    snapshots: Option<Receiver<Snapshot>>,
    watches: Watches,
    sink: Sink,
    sudo: Arc<AtomicBool>,
    group: LogFollowers,
}

impl DockerLogs {
    pub fn new(
        sources: Vec<DockerSource>,
        sink: Sink,
        group: LogFollowers,
        events: Option<DockerEvents>,
    ) -> Self {
        let watches = Watches::default();
        let sudo = Arc::<AtomicBool>::default();
        let snapshots = (!sources.is_empty()).then(|| {
            let (snapshots, receiver) = sync_channel(sources.len());
            let discovery = Discovery {
                sources,
                events,
                snapshots,
                group: group.clone(),
                watches: Arc::clone(&watches),
                sudo: Arc::clone(&sudo),
            };
            let spawned = thread::Builder::new()
                .name("logs-discovery".to_string())
                .spawn(move || discovery.run());
            if let Err(error) = spawned {
                warn!("could not start docker container discovery: {error}");
            }

            receiver
        });

        Self {
            followers: HashMap::new(),
            snapshots,
            watches,
            sink,
            sudo,
            group,
        }
    }

    pub fn sync(
        &mut self,
        positions: &mut HashMap<String, HashMap<String, i64>>,
        now_nanos: i64,
    ) {
        let snapshots: Vec<Snapshot> = self
            .snapshots
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();

        for Snapshot { source, key, states } in snapshots {
            let known = positions.entry(key).or_insert_with(|| {
                states.iter().map(|(id, _)| (id.clone(), now_nanos)).collect()
            });
            known.retain(|id, _| states.iter().any(|(listed, _)| listed == id));

            self.follow_source(source, &states, known);
        }
    }

    fn follow_source(
        &mut self,
        source: usize,
        states: &[(String, bool)],
        known: &HashMap<String, i64>,
    ) {
        let mut ended = HashMap::new();
        self.followers.retain(|(follower_source, id), followed| {
            if *follower_source != source || !followed.follower.is_finished() {
                return true;
            }

            let last = followed.follower.last().as_ref().and_then(Position::nanos);
            ended.insert(Arc::clone(id), last.unwrap_or_default());
            lock(&self.watches).remove(id);
            false
        });

        let running: HashSet<&str> = states
            .iter()
            .filter(|(_, is_running)| *is_running)
            .map(|(id, _)| id.as_str())
            .collect();

        self.followers.retain(|(follower_source, id), followed| {
            if *follower_source != source || running.contains(&**id) {
                followed.gone_since = None;
                return true;
            }

            let gone_since = *followed.gone_since.get_or_insert_with(Instant::now);
            let keeps = gone_since.elapsed() < STOP_GRACE;
            if !keeps {
                lock(&self.watches).remove(id);
            }

            keeps
        });

        for id in running {
            let key = (source, Arc::<str>::from(id));
            if self.followers.contains_key(&key) {
                continue;
            }

            let since = known
                .get(id)
                .copied()
                .unwrap_or_default()
                .max(ended.get(id).copied().unwrap_or_default());
            let feed = ContainerFeed::new(Arc::clone(&key.1));
            let started = Follower::start(
                feed,
                source,
                Arc::clone(&key.1),
                (since > 0).then_some(Position::Time(since)),
                self.sink.clone(),
                self.group.clone(),
                Arc::clone(&self.sudo),
            );

            match started {
                Ok(follower) => {
                    info!("following docker logs of {}", short_id(id));
                    lock(&self.watches).insert(Arc::clone(&key.1), follower.watch());
                    self.followers.insert(key, Followed { follower, gone_since: None });
                }
                Err(error) => {
                    warn!("could not follow docker logs of {}: {error}", short_id(id));
                }
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use daroyan_protocol::log::{LogFormat, LogStream};

    use super::{
        Check, ContainerFeed, LogsRead, PARTIAL_BYTES, join_partials, parse_timestamp,
    };
    use crate::logs::follow::{Feed, Position};
    use crate::logs::parse::to_log_line;

    #[test]
    fn parses_docker_timestamps() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:01Z"), Some(1_000_000_000));
        assert_eq!(
            parse_timestamp("2026-10-16T01:02:03.5Z"),
            Some(1_792_112_523_500_000_000)
        );
        assert_eq!(
            parse_timestamp("2026-10-16T01:02:03.123456789Z").map(|n| n % 1_000_000_000),
            Some(123_456_789)
        );
        assert_eq!(parse_timestamp("not a time"), None);
        assert_eq!(parse_timestamp("2026-10-16T01:02:03.1é3Z"), None);
        assert_eq!(parse_timestamp("2026-10-16T01:02:03.+5Z"), None);
    }

    #[test]
    fn joins_messages_docker_split_into_partials() {
        let stamp = "2026-11-06T06:02:53.157925476Z ";
        let first = "a".repeat(PARTIAL_BYTES);
        let second = "b".repeat(PARTIAL_BYTES);
        let message = format!("{first}{stamp}{second}{stamp}tail");

        assert_eq!(join_partials(message.as_bytes()), format!("{first}{second}tail"));

        let plain = format!("{first} not a stamp");
        assert_eq!(join_partials(plain.as_bytes()), plain);
    }

    #[test]
    fn reads_container_lines_and_resumes_after_the_last_one() {
        let feed = ContainerFeed::new(Arc::from("c1"));

        let line = feed
            .read(b"2026-11-06T10:00:02Z disk almost full", LogStream::Stderr)
            .unwrap();
        assert_eq!(line.text, "disk almost full");
        assert_eq!(
            line.position,
            Some(Position::Time(parse_timestamp("2026-11-06T10:00:02Z").unwrap()))
        );
        assert_eq!(to_log_line("api", LogFormat::Auto, line).severity, 13);
        assert!(feed.read(b"permission denied", LogStream::Stderr).is_none());

        let since = Position::Time(parse_timestamp("2026-11-06T10:00:01.5Z").unwrap());
        let resume = |tail: Option<u32>, since: Option<&str>| LogsRead {
            container: "c1".to_string(),
            follow: true,
            tail,
            since: since.map(str::to_string),
        };
        assert_eq!(feed.request(Some(&since)), resume(Some(10_000), None));

        let older = Position::Time(parse_timestamp("2026-11-06T10:00:01Z").unwrap());
        assert!(!feed.starts_after_gap(&older, Some(&since)));
        let newer = Position::Time(parse_timestamp("2026-11-06T10:00:02Z").unwrap());
        assert!(feed.starts_after_gap(&newer, Some(&since)));
        feed.rescan();
        assert_eq!(
            feed.request(Some(&since)),
            resume(None, Some("1793959201.500000000"))
        );
        assert!(!feed.starts_after_gap(&newer, Some(&since)));
        assert_eq!(feed.request(Some(&since)), resume(Some(10_000), None));
        assert_eq!(feed.request(None), resume(None, None));
    }

    #[test]
    fn restarts_only_a_follower_that_made_no_progress_while_behind() {
        let check = Check { delivered: Some(10), newest: Some(20) };

        assert!(check.is_stuck(Some(10)));
        assert!(!check.is_stuck(Some(15)));

        let quiet = Check { delivered: Some(20), newest: Some(20) };
        assert!(!quiet.is_stuck(Some(20)));

        let unreadable = Check { delivered: Some(10), newest: None };
        assert!(!unreadable.is_stuck(Some(10)));
    }
}
