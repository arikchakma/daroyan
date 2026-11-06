use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use daroyan_protocol::log::LogStream;
use log::warn;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::docker_read::{self, DockerRead, DockerStream, StreamHandle};
use crate::logs::parse::RawLine;
use crate::service::is_permission_error;

const QUEUE_BYTES: usize = 8 * 1024 * 1024;
const TICK_BYTES: usize = 2 * 1024 * 1024;
const LINE_OVERHEAD: usize = 64;
const ROOM_WAIT: Duration = Duration::from_millis(200);
const OVERLAP_WAIT: Duration = Duration::from_millis(250);
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(30);
const STEADY_AFTER: Duration = Duration::from_secs(60);
const READ_BUFFER: usize = 64 * 1024;
const THREAD_STACK: usize = 256 * 1024;

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    Time(i64),
    Cursor(Arc<str>),
}

impl Position {
    pub fn nanos(&self) -> Option<i64> {
        match self {
            Self::Time(nanos) => Some(*nanos),
            Self::Cursor(_) => None,
        }
    }

    fn is_after(&self, from: Option<&Self>) -> bool {
        match (self, from) {
            (Self::Time(nanos), Some(Self::Time(from))) => nanos > from,
            _ => true,
        }
    }
}

pub struct FollowedLine {
    pub source: usize,
    pub key: Arc<str>,
    pub raw: RawLine,
}

pub trait Feed: Send + Sync + 'static {
    fn name(&self) -> String;

    fn launch(&self, from: Option<&Position>, sudo: bool) -> Launch;

    fn read(&self, line: &[u8], stream: LogStream) -> Option<RawLine>;

    fn max_line_bytes(&self) -> usize;

    fn reads_stderr(&self) -> bool;

    fn can_use_sudo(&self) -> bool;

    fn is_live(&self) -> bool;

    fn starts_after_gap(&self, _first: &Position, _from: Option<&Position>) -> bool {
        false
    }

    fn rescan(&self) {}

    fn recover_from(&self, _error: &str) -> Option<Position> {
        None
    }

    fn restarts_at_once(&self) -> bool {
        false
    }
}

pub enum Launch {
    Command(Command),
    Docker { read: DockerRead, sudo: bool },
}

#[derive(Clone)]
enum Stopper {
    Group(i32),
    Stream(StreamHandle),
}

impl Stopper {
    fn terminate(&self) {
        match self {
            Self::Group(pid) => signal_group(*pid, Signal::SIGTERM),
            Self::Stream(stream) => stream.close(),
        }
    }

    fn kill(&self) {
        match self {
            Self::Group(pid) => signal_group(*pid, Signal::SIGKILL),
            Self::Stream(stream) => stream.close(),
        }
    }
}

enum Process {
    Child(Child),
    Stream(DockerStream),
}

struct Running {
    run: u64,
    stopper: Stopper,
    process: Process,
}

impl Running {
    fn start(launch: Launch) -> std::io::Result<Self> {
        let run = NEXT_RUN.fetch_add(1, Ordering::AcqRel);
        match launch {
            Launch::Command(mut command) => {
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .process_group(0);
                let child = command.spawn()?;
                let stopper = Stopper::Group(child.id().cast_signed());
                Ok(Self { run, stopper, process: Process::Child(child) })
            }
            Launch::Docker { read, sudo } => {
                let stream = docker_read::open(&read, sudo);
                let stopper = Stopper::Stream(stream.handle());
                Ok(Self { run, stopper, process: Process::Stream(stream) })
            }
        }
    }

    fn take_pipes(
        &mut self,
    ) -> (Option<Box<dyn Read + Send>>, Option<Box<dyn Read + Send>>) {
        match &mut self.process {
            Process::Child(child) => (
                child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
                child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
            ),
            Process::Stream(stream) => (
                stream
                    .take_stdout()
                    .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
                stream
                    .take_stderr()
                    .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
            ),
        }
    }

    fn finish(self) -> (bool, Option<String>) {
        match self.process {
            Process::Child(mut child) => {
                wait_for_exit(child.id().cast_signed());
                let status = child.wait().ok();
                (
                    status.is_some_and(|status| status.success()),
                    status.map(|status| status.to_string()),
                )
            }
            Process::Stream(stream) => {
                let code = stream.handle().wait();
                (code == 0, Some(format!("exit status: {code}")))
            }
        }
    }
}

#[derive(Default)]
struct Group {
    stopped: bool,
    running: HashMap<u64, Stopper>,
}

#[derive(Clone, Default)]
pub struct LogFollowers {
    group: Arc<Mutex<Group>>,
}

impl LogFollowers {
    pub fn stop(&self) {
        let mut group = lock(&self.group);
        group.stopped = true;
        for stopper in group.running.values() {
            stopper.terminate();
        }
    }

    pub fn kill_left(&self) {
        for stopper in lock(&self.group).running.values() {
            stopper.kill();
        }
    }

    pub fn is_stopped(&self) -> bool {
        lock(&self.group).stopped
    }

    pub fn has_children(&self) -> bool {
        !lock(&self.group).running.is_empty()
    }

    fn register(&self, run: u64, stopper: Stopper) {
        let mut group = lock(&self.group);
        if group.stopped {
            stopper.terminate();
        }

        group.running.insert(run, stopper);
    }

    fn release(&self, run: u64) {
        lock(&self.group).running.remove(&run);
    }
}

pub fn signal_group(pid: i32, signal: Signal) {
    let _ = killpg(Pid::from_raw(pid), signal);
}

#[derive(Default)]
struct Gate {
    used: Mutex<usize>,
    freed: Condvar,
}

impl Gate {
    fn enter(&self, size: usize, is_stopped: impl Fn() -> bool) -> bool {
        let mut used = lock(&self.used);
        while *used > 0 && *used + size > QUEUE_BYTES {
            if is_stopped() {
                return false;
            }

            used = self
                .freed
                .wait_timeout(used, ROOM_WAIT)
                .map_or_else(|error| error.into_inner().0, |(used, _)| used);
        }

        *used += size;
        true
    }

    fn leave(&self, size: usize) {
        let mut used = lock(&self.used);
        *used = used.saturating_sub(size);
        self.freed.notify_all();
    }
}

fn measure(line: &FollowedLine) -> usize {
    line.raw.text.len() + LINE_OVERHEAD
}

pub struct LineQueue {
    sender: Sender<FollowedLine>,
    receiver: Receiver<FollowedLine>,
    gate: Arc<Gate>,
}

impl Default for LineQueue {
    fn default() -> Self {
        let (sender, receiver) = channel();

        Self { sender, receiver, gate: Arc::default() }
    }
}

impl LineQueue {
    pub fn sink(&self) -> Sink {
        Sink {
            lines: self.sender.clone(),
            gate: Arc::clone(&self.gate),
        }
    }

    pub fn receive(&self, until: Instant) -> Vec<FollowedLine> {
        let mut lines = Vec::new();
        let mut bytes = 0;

        while bytes < TICK_BYTES {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }

            let Ok(line) = self.receiver.recv_timeout(left) else {
                break;
            };
            let size = measure(&line);
            self.gate.leave(size);
            bytes += size;
            lines.push(line);
        }

        lines
    }
}

#[derive(Clone)]
pub struct Sink {
    lines: Sender<FollowedLine>,
    gate: Arc<Gate>,
}

impl Sink {
    fn send(&self, line: FollowedLine, is_stopped: impl Fn() -> bool) -> bool {
        let size = measure(&line);
        if !self.gate.enter(size, is_stopped) {
            return false;
        }
        if self.lines.send(line).is_err() {
            self.gate.leave(size);
            return false;
        }

        true
    }
}

#[derive(Default)]
struct Slot {
    stopped: bool,
    running: Option<Stopper>,
}

impl Slot {
    fn stop(&mut self) {
        self.stopped = true;
        self.interrupt();
    }

    fn interrupt(&self) {
        if let Some(stopper) = &self.running {
            stopper.terminate();
        }
    }
}

#[derive(Clone)]
pub struct Watch {
    slot: Arc<Mutex<Slot>>,
    last: Arc<Mutex<Option<Position>>>,
    from: Option<Position>,
}

impl Watch {
    pub fn delivered(&self) -> Option<Position> {
        lock(&self.last).clone().or_else(|| self.from.clone())
    }

    pub fn restart(&self) {
        lock(&self.slot).interrupt();
    }
}

pub struct Follower {
    watch: Watch,
    handle: JoinHandle<()>,
}

impl Follower {
    pub fn start<F: Feed>(
        feed: F,
        source: usize,
        key: Arc<str>,
        from: Option<Position>,
        sink: Sink,
        group: LogFollowers,
        sudo: Arc<AtomicBool>,
    ) -> std::io::Result<Self> {
        let watch = Watch {
            slot: Arc::default(),
            last: Arc::default(),
            from: from.clone(),
        };
        let task = FollowTask {
            name: feed.name(),
            feed,
            source,
            key,
            from,
            sink,
            slot: Arc::clone(&watch.slot),
            last: Arc::clone(&watch.last),
            group,
            sudo,
        };
        let handle = thread::Builder::new()
            .name(format!("logs-{}", task.name))
            .stack_size(THREAD_STACK)
            .spawn(move || task.run())?;

        Ok(Self { watch, handle })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub fn last(&self) -> Option<Position> {
        lock(&self.watch.last).clone()
    }

    pub fn watch(&self) -> Watch {
        self.watch.clone()
    }

    pub fn stop(&self) {
        lock(&self.watch.slot).stop();
        self.handle.thread().unpark();
    }
}

impl Drop for Follower {
    fn drop(&mut self) {
        self.stop();
    }
}

struct FollowTask<F> {
    feed: F,
    name: String,
    source: usize,
    key: Arc<str>,
    from: Option<Position>,
    sink: Sink,
    slot: Arc<Mutex<Slot>>,
    last: Arc<Mutex<Option<Position>>>,
    group: LogFollowers,
    sudo: Arc<AtomicBool>,
}

#[derive(Default)]
struct RunEnd {
    used_sudo: bool,
    is_clean: bool,
    status: Option<String>,
    has_gap: AtomicBool,
    has_overlap: AtomicBool,
    has_started: AtomicBool,
    denied: AtomicBool,
    error: Mutex<Option<String>>,
}

impl<F: Feed> FollowTask<F> {
    fn is_stopped(&self) -> bool {
        lock(&self.slot).stopped || self.group.is_stopped()
    }

    fn run(self) {
        let mut retry = FIRST_RETRY;

        while !self.is_stopped() {
            let from = lock(&self.last).clone().or_else(|| self.from.clone());
            let started = Instant::now();
            let end = self.follow_once(from.as_ref());

            if self.is_stopped() {
                return;
            }
            if end.has_gap.load(Ordering::Acquire) {
                continue;
            }
            let error = lock(&end.error).take();
            if let Some(position) =
                error.as_deref().and_then(|error| self.feed.recover_from(error))
            {
                warn!(
                    "{} could not resume ({}), starting again from now",
                    self.name,
                    error.unwrap_or_default()
                );
                *lock(&self.last) = Some(position);
                continue;
            }
            if end.denied.load(Ordering::Acquire)
                && !end.used_sudo
                && self.feed.can_use_sudo()
            {
                self.sudo.store(true, Ordering::Release);
                continue;
            }
            if !self.feed.is_live() {
                return;
            }

            if end.is_clean
                && (started.elapsed() >= FIRST_RETRY || self.feed.restarts_at_once())
            {
                retry = FIRST_RETRY;
                continue;
            }
            if started.elapsed() >= STEADY_AFTER {
                retry = FIRST_RETRY;
            }
            if !end.is_clean {
                let error = error.unwrap_or_else(|| {
                    end.status.unwrap_or_else(|| "not started".to_string())
                });
                warn!(
                    "{} failed ({error}), following again in {}s",
                    self.name,
                    retry.as_secs()
                );
            }

            self.sleep(retry);
            retry = (retry * 2).min(LAST_RETRY);
        }
    }

    fn sleep(&self, duration: Duration) {
        let until = Instant::now() + duration;

        while !self.is_stopped() {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }

            thread::park_timeout(left);
        }
    }

    fn follow_once(&self, from: Option<&Position>) -> RunEnd {
        let end = RunEnd {
            used_sudo: self.feed.can_use_sudo() && self.sudo.load(Ordering::Acquire),
            ..RunEnd::default()
        };
        let Some(mut running) = self.spawn(from, end.used_sudo) else {
            return end;
        };

        let (stdout, stderr) = running.take_pipes();
        thread::scope(|scope| {
            if let Some(stderr) = stderr {
                let reader = thread::Builder::new()
                    .name(format!("logs-err-{}", self.name))
                    .stack_size(THREAD_STACK)
                    .spawn_scoped(scope, || {
                        self.read_stream(stderr, LogStream::Stderr, from, &end);
                    });
                if let Err(error) = reader {
                    warn!("could not read the errors of {}: {error}", self.name);
                }
            }
            if let Some(stdout) = stdout {
                self.read_stream(stdout, LogStream::Stdout, from, &end);
            }
        });

        let run = running.run;
        let (is_clean, status) = running.finish();
        lock(&self.slot).running = None;
        self.group.release(run);

        RunEnd { is_clean, status, ..end }
    }

    fn spawn(&self, from: Option<&Position>, sudo: bool) -> Option<Running> {
        let launch = self.feed.launch(from, sudo);

        let mut slot = lock(&self.slot);
        if slot.stopped {
            return None;
        }

        let running = Running::start(launch)
            .inspect_err(|error| warn!("could not run {}: {error}", self.name))
            .ok()?;
        slot.running = Some(running.stopper.clone());
        drop(slot);

        self.group.register(running.run, running.stopper.clone());

        Some(running)
    }

    fn read_stream(
        &self,
        pipe: impl Read,
        stream: LogStream,
        from: Option<&Position>,
        end: &RunEnd,
    ) {
        let mut reader = BufReader::with_capacity(READ_BUFFER, pipe);
        let mut buffer = Vec::new();
        let is_log = stream != LogStream::Stderr || self.feed.reads_stderr();

        loop {
            let read =
                read_line_capped(&mut reader, &mut buffer, self.feed.max_line_bytes());
            if read.unwrap_or_default() == 0 {
                return;
            }

            let line = trim_line_end(&buffer);
            let Some(raw) = is_log.then(|| self.feed.read(line, stream)).flatten() else {
                let text = String::from_utf8_lossy(line);
                if is_permission_error(&text) {
                    end.denied.store(true, Ordering::Release);
                }
                lock(&end.error).get_or_insert_with(|| text.into_owned());
                continue;
            };
            if raw.position.as_ref().is_some_and(|position| !position.is_after(from)) {
                end.has_overlap.store(true, Ordering::Release);
                continue;
            }
            if let Some(position) = &raw.position
                && !end.has_started.swap(true, Ordering::AcqRel)
                && self.feed.starts_after_gap(position, from)
                && !wait_for_overlap(end)
            {
                self.feed.rescan();
                end.has_gap.store(true, Ordering::Release);
                lock(&self.slot).interrupt();
                return;
            }

            self.remember(raw.position.as_ref());
            let line = FollowedLine {
                source: self.source,
                key: Arc::clone(&self.key),
                raw,
            };

            if !self.sink.send(line, || self.is_stopped()) {
                lock(&self.slot).stop();
                return;
            }
        }
    }

    fn remember(&self, position: Option<&Position>) {
        let Some(position) = position else {
            return;
        };

        let mut last = lock(&self.last);
        let is_newer = match (&*last, position) {
            (Some(Position::Time(old)), Position::Time(new)) => new > old,
            _ => true,
        };
        if is_newer {
            *last = Some(position.clone());
        }
    }
}

fn wait_for_overlap(end: &RunEnd) -> bool {
    let until = Instant::now() + OVERLAP_WAIT;
    while !end.has_overlap.load(Ordering::Acquire) {
        if Instant::now() >= until {
            return false;
        }

        thread::sleep(Duration::from_millis(10));
    }

    true
}

#[cfg(target_os = "linux")]
fn wait_for_exit(pid: i32) {
    use nix::sys::wait::{Id, WaitPidFlag, waitid};

    let flags = WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT;
    let _ = waitid(Id::Pid(Pid::from_raw(pid)), flags);
}

#[cfg(not(target_os = "linux"))]
fn wait_for_exit(_pid: i32) {}

fn read_line_capped(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<usize> {
    line.clear();
    let mut read = 0;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(read);
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let used = newline.map_or(available.len(), |index| index + 1);
        let room = limit.saturating_sub(line.len());
        line.extend_from_slice(&available[..used.min(room)]);
        reader.consume(used);
        read += used;

        if newline.is_some() {
            return Ok(read);
        }
    }
}

fn trim_line_end(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);

    line.strip_suffix(b"\r").unwrap_or(line)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use daroyan_protocol::log::LogStream;

    use super::{
        Feed, FollowTask, Launch, LineQueue, LogFollowers, Position, RunEnd, Slot,
        read_line_capped,
    };
    use crate::logs::parse::RawLine;

    struct Numbered;

    impl Feed for Numbered {
        fn name(&self) -> String {
            "numbered".to_string()
        }

        fn launch(&self, _from: Option<&Position>, _sudo: bool) -> Launch {
            Launch::Command(std::process::Command::new("true"))
        }

        fn read(&self, line: &[u8], stream: LogStream) -> Option<RawLine> {
            let text = std::str::from_utf8(line).ok()?;
            let (number, message) = text.split_once(' ')?;

            Some(RawLine {
                at: 0,
                stream,
                text: message.to_string(),
                severity: None,
                position: Some(Position::Time(number.parse().ok()?)),
            })
        }

        fn max_line_bytes(&self) -> usize {
            2 * 1024 * 1024
        }

        fn reads_stderr(&self) -> bool {
            true
        }

        fn can_use_sudo(&self) -> bool {
            true
        }

        fn is_live(&self) -> bool {
            true
        }

        fn starts_after_gap(&self, first: &Position, from: Option<&Position>) -> bool {
            let (Some(first), Some(from)) =
                (first.nanos(), from.and_then(Position::nanos))
            else {
                return false;
            };

            first > from + 1
        }
    }

    fn make_task(queue: &LineQueue) -> FollowTask<Numbered> {
        FollowTask {
            feed: Numbered,
            name: "numbered".to_string(),
            source: 0,
            key: Arc::from("c1"),
            from: None,
            sink: queue.sink(),
            slot: Arc::new(Mutex::new(Slot::default())),
            last: Arc::new(Mutex::new(None)),
            group: LogFollowers::default(),
            sudo: Arc::new(AtomicBool::new(false)),
        }
    }

    fn receive_texts(queue: &LineQueue) -> Vec<String> {
        queue
            .receive(Instant::now() + Duration::from_millis(50))
            .into_iter()
            .map(|line| line.raw.text)
            .collect()
    }

    #[test]
    fn skips_lines_up_to_the_resume_position() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        let output = "1 older\n2 already sent\n3 first new\n4 second new\n";

        task.read_stream(
            Cursor::new(output),
            LogStream::Stdout,
            Some(&Position::Time(2)),
            &RunEnd::default(),
        );

        assert_eq!(receive_texts(&queue), ["first new", "second new"]);
        assert_eq!(*task.last.lock().unwrap(), Some(Position::Time(4)));
    }

    #[test]
    fn restarts_when_the_resume_leaves_a_gap() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        let end = RunEnd::default();

        task.read_stream(
            Cursor::new("9 after a gap\n"),
            LogStream::Stdout,
            Some(&Position::Time(2)),
            &end,
        );
        assert!(end.has_gap.load(Ordering::Acquire));
        assert!(receive_texts(&queue).is_empty());

        let end = RunEnd::default();
        task.read_stream(
            Cursor::new("2 overlap\n9 new\n"),
            LogStream::Stdout,
            Some(&Position::Time(2)),
            &end,
        );
        assert!(!end.has_gap.load(Ordering::Acquire));
        assert_eq!(receive_texts(&queue), ["new"]);
    }

    #[test]
    fn waits_for_the_other_stream_before_calling_it_a_gap() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        let end = RunEnd::default();

        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(50));
                end.has_overlap.store(true, Ordering::Release);
            });
            task.read_stream(
                Cursor::new("9 new on stderr\n"),
                LogStream::Stderr,
                Some(&Position::Time(2)),
                &end,
            );
        });

        assert!(!end.has_gap.load(Ordering::Acquire));
        assert_eq!(receive_texts(&queue), ["new on stderr"]);
    }

    #[test]
    fn spots_permission_errors_between_lines() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        let output =
            "permission denied while trying to connect to the docker API\n1 ok\n";
        let end = RunEnd::default();

        task.read_stream(Cursor::new(output), LogStream::Stderr, None, &end);

        assert_eq!(receive_texts(&queue), ["ok"]);
        assert!(end.denied.load(Ordering::Acquire));
    }

    #[test]
    fn waits_for_room_instead_of_dropping_lines() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        let big = "x".repeat(1024 * 1024);
        let output: String = (1..=12).map(|number| format!("{number} {big}\n")).collect();

        thread::scope(|scope| {
            let reader = scope.spawn(|| {
                task.read_stream(
                    Cursor::new(output),
                    LogStream::Stdout,
                    None,
                    &RunEnd::default(),
                );
            });
            thread::sleep(Duration::from_millis(300));
            assert!(!reader.is_finished());

            let mut received = 0;
            while received < 12 {
                received +=
                    queue.receive(Instant::now() + Duration::from_millis(100)).len();
            }
            assert_eq!(received, 12);
        });
    }

    #[test]
    fn stops_when_nobody_reads_the_lines() {
        let queue = LineQueue::default();
        let task = make_task(&queue);
        drop(queue);

        task.read_stream(
            Cursor::new("1 hi\n"),
            LogStream::Stdout,
            None,
            &RunEnd::default(),
        );

        assert!(task.is_stopped());
    }

    #[test]
    fn caps_long_lines_and_keeps_reading() {
        let input = format!("{}\nnext\n", "x".repeat(100));
        let mut reader = std::io::BufReader::with_capacity(8, Cursor::new(input));
        let mut line = Vec::new();

        assert_eq!(read_line_capped(&mut reader, &mut line, 10).unwrap(), 101);
        assert_eq!(line, b"xxxxxxxxxx");
        assert_eq!(read_line_capped(&mut reader, &mut line, 10).unwrap(), 5);
        assert_eq!(line, b"next\n");
        assert_eq!(read_line_capped(&mut reader, &mut line, 10).unwrap(), 0);
    }
}
