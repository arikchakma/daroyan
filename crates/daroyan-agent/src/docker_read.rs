use std::collections::HashMap;
use std::env;
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use log::warn;

pub use crate::docker_read_protocol::{
    ALLOW_FILE, DockerRead, EVENT_ACTIONS, ListTarget, LogsRead, render_targets,
};
use crate::docker_read_protocol::{
    FrameKind, HELPER_NAME, Request, STREAM_WINDOW, TARGETS_ENV, read_frame,
};
use crate::service::is_permission_error;

const MAX_ERROR_BYTES: usize = 64 * 1024;
const STOPPED: &[u8] = b"the docker read helper stopped\n";

static USES_SUDO: AtomicBool = AtomicBool::new(false);
static TARGETS: OnceLock<String> = OnceLock::new();
static HELPER: Mutex<Option<Arc<Connection>>> = Mutex::new(None);

pub fn helper_path() -> PathBuf {
    env::args_os()
        .next()
        .map(PathBuf::from)
        .filter(|program| program.is_absolute())
        .or_else(|| env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin/daroyan"))
        .with_file_name(HELPER_NAME)
}

pub fn uses_sudo() -> bool {
    USES_SUDO.load(Ordering::Acquire)
}

pub fn require_sudo() {
    USES_SUDO.store(true, Ordering::Release);
}

pub fn set_targets(targets: &[ListTarget]) {
    match render_targets(targets) {
        Ok(text) => {
            let _ = TARGETS.set(text);
        }
        Err(error) => {
            warn!("could not pass the docker targets to the read helper: {error}");
        }
    }
}

pub fn run(read: &DockerRead) -> Result<String> {
    let mut sudo = uses_sudo();

    loop {
        let mut stream = open(read, sudo);
        let stdout = stream.take_stdout();
        let stderr = stream.take_stderr();
        let (output, error) = thread::scope(|scope| {
            let errors = scope.spawn(|| {
                let mut error = Vec::new();
                if let Some(mut stderr) = stderr {
                    let _ = stderr
                        .by_ref()
                        .take(MAX_ERROR_BYTES as u64)
                        .read_to_end(&mut error);
                    let _ = io::copy(&mut stderr, &mut io::sink());
                }
                error
            });
            let mut output = Vec::new();
            if let Some(mut stdout) = stdout {
                let _ = stdout.read_to_end(&mut output);
            }
            (output, errors.join().unwrap_or_default())
        });

        if stream.handle().wait() == 0 {
            return Ok(String::from_utf8_lossy(&output).into_owned());
        }

        let error = String::from_utf8_lossy(&error);
        if !sudo && is_permission_error(&error) {
            require_sudo();
            sudo = true;
            continue;
        }

        bail!("{}", error.trim());
    }
}

pub fn open(read: &DockerRead, sudo: bool) -> DockerStream {
    let connection = connect(sudo);
    let id = connection
        .as_ref()
        .map_or(0, |connection| connection.next.fetch_add(1, Ordering::AcqRel));
    let (stream, route) = new_stream(id, connection.clone());

    let request = Request::Open { stream: id, args: read.helper_args() };
    match (connection, request.render()) {
        (Some(connection), Ok(line)) => {
            lock(&connection.routes).insert(id, route);
            if !connection.send(&line) {
                connection.fail(id, STOPPED);
            }
        }
        (Some(_), Err(error)) => route.fail(error.as_bytes()),
        (None, _) => route.fail(STOPPED),
    }

    stream
}

fn new_stream(id: u32, connection: Option<Arc<Connection>>) -> (DockerStream, Route) {
    let (stdout_tx, stdout_rx) = sync_channel(STREAM_WINDOW);
    let (stderr_tx, stderr_rx) = sync_channel(STREAM_WINDOW);
    let exit = Arc::new(Exit::default());
    let route = Route {
        stdout: stdout_tx,
        stderr: stderr_tx,
        exit: Arc::clone(&exit),
    };
    let acker = connection.clone().map(|connection| Acker { id, connection });
    let stream = DockerStream {
        stdout: Some(ChunkReader::new(stdout_rx, acker.clone())),
        stderr: Some(ChunkReader::new(stderr_rx, acker)),
        handle: StreamHandle { id, exit, connection },
    };

    (stream, route)
}

pub struct DockerStream {
    stdout: Option<ChunkReader>,
    stderr: Option<ChunkReader>,
    handle: StreamHandle,
}

impl DockerStream {
    pub fn take_stdout(&mut self) -> Option<ChunkReader> {
        self.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChunkReader> {
        self.stderr.take()
    }

    pub fn handle(&self) -> StreamHandle {
        self.handle.clone()
    }
}

#[derive(Clone)]
pub struct StreamHandle {
    id: u32,
    exit: Arc<Exit>,
    connection: Option<Arc<Connection>>,
}

impl StreamHandle {
    pub fn close(&self) {
        let Some(connection) = &self.connection else { return };
        if self.exit.code().is_some() {
            return;
        }

        if let Ok(line) = (Request::Close { stream: self.id }).render() {
            connection.send(&line);
        }
        connection.fail(self.id, b"");
    }

    pub fn wait(&self) -> i32 {
        self.exit.wait(None).unwrap_or(1)
    }

    pub fn wait_for(&self, timeout: Duration) -> Option<i32> {
        self.exit.wait(Some(Instant::now() + timeout))
    }
}

#[derive(Clone)]
struct Acker {
    id: u32,
    connection: Arc<Connection>,
}

impl Acker {
    fn ack(&self) {
        if let Ok(line) = (Request::Ack { stream: self.id }).render() {
            self.connection.send(&line);
        }
    }
}

pub struct ChunkReader {
    chunks: Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    at: usize,
    acker: Option<Acker>,
}

impl ChunkReader {
    fn new(chunks: Receiver<Vec<u8>>, acker: Option<Acker>) -> Self {
        Self { chunks, chunk: Vec::new(), at: 0, acker }
    }
}

impl Read for ChunkReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        while self.at >= self.chunk.len() {
            match self.chunks.recv() {
                Ok(chunk) => {
                    if let Some(acker) = &self.acker {
                        acker.ack();
                    }
                    self.chunk = chunk;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }

        let read = buffer.len().min(self.chunk.len() - self.at);
        buffer[..read].copy_from_slice(&self.chunk[self.at..self.at + read]);
        self.at += read;
        Ok(read)
    }
}

#[derive(Default)]
struct Exit {
    code: Mutex<Option<i32>>,
    done: Condvar,
}

impl Exit {
    fn code(&self) -> Option<i32> {
        *lock(&self.code)
    }

    fn set(&self, code: i32) {
        lock(&self.code).get_or_insert(code);
        self.done.notify_all();
    }

    fn wait(&self, until: Option<Instant>) -> Option<i32> {
        let mut code = lock(&self.code);
        while code.is_none() {
            let left = match until {
                Some(until) => until.saturating_duration_since(Instant::now()),
                None => Duration::from_secs(60),
            };
            if left.is_zero() {
                return None;
            }

            code = self
                .done
                .wait_timeout(code, left)
                .map_or_else(|error| error.into_inner().0, |(code, _)| code);
        }

        *code
    }
}

struct Route {
    stdout: SyncSender<Vec<u8>>,
    stderr: SyncSender<Vec<u8>>,
    exit: Arc<Exit>,
}

impl Route {
    fn fail(&self, message: &[u8]) {
        if !message.is_empty() {
            let _ = self.stderr.try_send(message.to_vec());
        }
        self.exit.set(1);
    }
}

struct Connection {
    sudo: bool,
    stdin: Mutex<Option<ChildStdin>>,
    routes: Mutex<HashMap<u32, Route>>,
    next: AtomicU32,
    alive: AtomicBool,
}

impl Connection {
    fn send(&self, line: &str) -> bool {
        let mut stdin = lock(&self.stdin);
        let Some(pipe) = stdin.as_mut() else { return false };
        if pipe.write_all(line.as_bytes()).and_then(|()| pipe.flush()).is_ok() {
            return true;
        }

        *stdin = None;
        false
    }

    fn fail(&self, id: u32, message: &[u8]) {
        if let Some(route) = lock(&self.routes).remove(&id) {
            route.fail(message);
        }
    }

    fn deliver(&self, id: u32, kind: FrameKind, data: Vec<u8>) {
        let sender = {
            let routes = lock(&self.routes);
            let Some(route) = routes.get(&id) else { return };
            match kind {
                FrameKind::Stdout => route.stdout.clone(),
                FrameKind::Stderr => route.stderr.clone(),
                FrameKind::Exit => return,
            }
        };

        if sender.try_send(data).is_err() {
            warn!("{HELPER_NAME} sent more than the stream window for read {id}");
        }
    }

    fn finish(&self, id: u32, code: i32) {
        if let Some(route) = lock(&self.routes).remove(&id) {
            route.exit.set(code);
        }
    }

    fn stop(&self) {
        self.alive.store(false, Ordering::Release);
        *lock(&self.stdin) = None;
        let routes: Vec<Route> =
            lock(&self.routes).drain().map(|(_, route)| route).collect();
        for route in routes {
            route.fail(STOPPED);
        }
    }
}

fn connect(sudo: bool) -> Option<Arc<Connection>> {
    let mut helper = lock(&HELPER);
    if let Some(connection) = helper.as_ref()
        && connection.alive.load(Ordering::Acquire)
        && (connection.sudo || !sudo)
    {
        return Some(Arc::clone(connection));
    }

    if let Some(old) = helper.take() {
        *lock(&old.stdin) = None;
    }

    let connection = spawn(sudo)
        .inspect_err(|error| warn!("could not start {HELPER_NAME}: {error}"))
        .ok()?;
    *helper = Some(Arc::clone(&connection));
    Some(connection)
}

fn spawn(sudo: bool) -> io::Result<Arc<Connection>> {
    let mut command = if sudo {
        let mut command = Command::new("sudo");
        command.arg("-n").arg(helper_path());
        command
    } else {
        let mut command = Command::new(helper_path());
        if let Some(targets) = TARGETS.get() {
            command.env(TARGETS_ENV, targets);
        }
        command
    };
    command
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command.spawn()?;
    let stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("no helper output"))?;
    let connection = Arc::new(Connection {
        sudo,
        stdin: Mutex::new(stdin),
        routes: Mutex::default(),
        next: AtomicU32::new(1),
        alive: AtomicBool::new(true),
    });

    let reader = Arc::clone(&connection);
    thread::Builder::new()
        .name("docker-read".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            demultiplex(&reader, stdout);
            let _ = child.kill();
            let _ = child.wait();
        })?;

    Ok(connection)
}

fn demultiplex(connection: &Connection, stdout: impl Read) {
    let mut input = BufReader::new(stdout);
    loop {
        match read_frame(&mut input) {
            Ok(Some(frame)) if frame.kind == FrameKind::Exit => {
                connection.finish(frame.stream, frame.exit_code());
            }
            Ok(Some(frame)) => connection.deliver(frame.stream, frame.kind, frame.data),
            Ok(None) => break,
            Err(error) => {
                warn!("{HELPER_NAME} sent a bad frame: {error}");
                break;
            }
        }
    }

    connection.stop();
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{Connection, DockerStream, STOPPED, demultiplex, lock, new_stream};
    use crate::docker_read_protocol::{FrameKind, write_exit, write_frame};

    fn connection() -> Arc<Connection> {
        Arc::new(Connection {
            sudo: false,
            stdin: Mutex::new(None),
            routes: Mutex::default(),
            next: AtomicU32::new(1),
            alive: AtomicBool::new(true),
        })
    }

    fn attach(connection: &Arc<Connection>, id: u32) -> DockerStream {
        let (stream, route) = new_stream(id, Some(Arc::clone(connection)));
        lock(&connection.routes).insert(id, route);
        stream
    }

    fn read_all(reader: Option<impl Read>) -> String {
        let mut text = String::new();
        reader.unwrap().read_to_string(&mut text).unwrap();
        text
    }

    #[test]
    fn routes_frames_to_their_streams() {
        let connection = connection();
        let mut first = attach(&connection, 1);
        let mut second = attach(&connection, 2);

        let mut frames = Vec::new();
        write_frame(&mut frames, 1, FrameKind::Stdout, b"one ").unwrap();
        write_frame(&mut frames, 2, FrameKind::Stderr, b"denied").unwrap();
        write_frame(&mut frames, 9, FrameKind::Stdout, b"closed earlier").unwrap();
        write_frame(&mut frames, 1, FrameKind::Stdout, b"two").unwrap();
        write_exit(&mut frames, 1, 0).unwrap();
        write_exit(&mut frames, 2, 3).unwrap();
        demultiplex(&connection, frames.as_slice());

        assert_eq!(read_all(first.take_stdout()), "one two");
        assert_eq!(read_all(first.take_stderr()), "");
        assert_eq!(first.handle().wait(), 0);
        assert_eq!(read_all(second.take_stderr()), "denied");
        assert_eq!(second.handle().wait(), 3);
    }

    #[test]
    fn fails_open_streams_when_the_helper_stops() {
        let connection = connection();
        let mut stream = attach(&connection, 1);

        let mut frames = Vec::new();
        write_frame(&mut frames, 1, FrameKind::Stdout, b"partial").unwrap();
        frames.extend_from_slice(&[0, 0, 0, 1]);
        demultiplex(&connection, frames.as_slice());

        assert_eq!(read_all(stream.take_stdout()), "partial");
        assert_eq!(read_all(stream.take_stderr()).as_bytes(), STOPPED);
        assert_eq!(stream.handle().wait(), 1);
        assert!(lock(&connection.routes).is_empty());
    }

    #[test]
    fn closing_ends_the_stream_at_once() {
        let connection = connection();
        let mut stream = attach(&connection, 1);
        let handle = stream.handle();
        assert_eq!(handle.wait_for(Duration::from_millis(20)), None);

        handle.close();

        assert_eq!(handle.wait_for(Duration::from_millis(20)), Some(1));
        assert_eq!(read_all(stream.take_stdout()), "");
    }
}
