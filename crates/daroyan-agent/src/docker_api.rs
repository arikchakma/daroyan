use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Read};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};

use crate::docker_file;
use crate::docker_read_protocol::{
    DockerRead, EVENT_ACTIONS, ListTarget, LogsRead, MAX_FRAME_BYTES,
};

const SOCKET: &str = "/var/run/docker.sock";
const MAX_HEAD_BYTES: usize = 32 * 1024;
const MAX_LINE_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 1024 * 1024;
const READ_BUFFER: usize = 16 * 1024;
const LABELS: [(&str, &str); 3] = [
    ("project", "com.docker.compose.project"),
    ("service", "com.docker.compose.service"),
    ("config_hash", "com.docker.compose.config-hash"),
];

pub trait Sink {
    fn out(&mut self, data: &[u8]) -> io::Result<()>;

    fn err(&mut self, data: &[u8]) -> io::Result<()>;
}

#[derive(Default)]
pub struct Cancel {
    cancelled: AtomicBool,
    socket: Mutex<Option<UnixStream>>,
}

impl Cancel {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(socket) = lock(&self.socket).take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn connect(&self) -> Result<UnixStream, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("the read was cancelled".to_string());
        }

        let socket = UnixStream::connect(SOCKET).map_err(|error| match error.kind() {
            io::ErrorKind::PermissionDenied => format!(
                "permission denied while trying to connect to the docker API at unix://{SOCKET}"
            ),
            _ => format!(
                "Cannot connect to the Docker daemon at unix://{SOCKET}. Is the docker daemon running? ({error})"
            ),
        })?;
        *lock(&self.socket) = socket.try_clone().ok();
        if self.cancelled.load(Ordering::Acquire) {
            let _ = socket.shutdown(Shutdown::Both);
        }

        Ok(socket)
    }
}

pub fn run(
    read: &DockerRead,
    targets: &[ListTarget],
    cancel: &Cancel,
    sink: &mut impl Sink,
) -> i32 {
    let result =
        check_configured(targets, read.containers(), cancel).and_then(|()| match read {
            DockerRead::List(target) => list(target, cancel, sink),
            DockerRead::Inspect(containers) => inspect(containers, cancel, sink),
            DockerRead::Logs(logs) => follow_logs(logs, cancel, sink),
            DockerRead::Events => follow_events(cancel, sink),
        });

    match result {
        Ok(()) => 0,
        Err(error) => {
            let _ = sink.err(format!("{error}\n").as_bytes());
            1
        }
    }
}

fn check_configured(
    targets: &[ListTarget],
    containers: &[String],
    cancel: &Cancel,
) -> Result<(), String> {
    if containers.is_empty() {
        return Ok(());
    }

    let mut allowed = HashSet::new();
    for target in targets {
        for container in list_containers(target, cancel)? {
            allowed.insert(container.id);
            allowed.extend(container.names);
        }
    }

    match containers.iter().find(|container| !allowed.contains(*container)) {
        Some(container) => Err(format!("{container} is not a container in the config")),
        None => Ok(()),
    }
}

struct Listed {
    id: String,
    names: Vec<String>,
    state: String,
}

fn list_containers(target: &ListTarget, cancel: &Cancel) -> Result<Vec<Listed>, String> {
    let path = format!("/containers/json?all=1&filters={}", encode(&filters(target)));
    let listing = get_json(cancel, &path)?;

    Ok(listing
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|container| Listed {
            id: text(&container["Id"]),
            names: container["Names"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|name| text(name).trim_start_matches('/').to_string())
                .collect(),
            state: text(&container["State"]),
        })
        .collect())
}

fn filters(target: &ListTarget) -> String {
    let mut filters = serde_json::Map::new();
    for filter in &target.filters {
        let (key, value) = filter.split_once('=').unwrap_or((filter, ""));
        let values = filters.entry(key).or_insert_with(|| json!([]));
        if let Some(values) = values.as_array_mut() {
            values.push(json!(value));
        }
    }

    Value::Object(filters).to_string()
}

fn list(
    target: &ListTarget,
    cancel: &Cancel,
    sink: &mut impl Sink,
) -> Result<(), String> {
    for container in list_containers(target, cancel)? {
        let line = format!(
            "{} {} {}\n",
            container.id,
            container.names.join(","),
            container.state
        );
        sink.out(line.as_bytes()).map_err(write_error)?;
    }

    Ok(())
}

fn inspect(
    containers: &[String],
    cancel: &Cancel,
    sink: &mut impl Sink,
) -> Result<(), String> {
    let mut missing = Vec::new();
    for container in containers {
        match get_json(cancel, &format!("/containers/{container}/json")) {
            Ok(details) => {
                let line = facts(&details).to_string() + "\n";
                sink.out(line.as_bytes()).map_err(write_error)?;
            }
            Err(error) if error.contains("No such container") => missing.push(container),
            Err(error) => return Err(error),
        }
    }

    match missing.first() {
        Some(container) => Err(format!("Error: No such object: {container}")),
        None => Ok(()),
    }
}

fn facts(details: &Value) -> Value {
    let state = &details["State"];
    let labels = &details["Config"]["Labels"];
    let mut facts = json!({
        "id": details["Id"],
        "name": details["Name"],
        "image": details["Config"]["Image"],
        "image_id": details["Image"],
        "running": state["Running"],
        "started_at": state["StartedAt"],
        "finished_at": state["FinishedAt"],
        "exit_code": state["ExitCode"],
        "oom_killed": state["OOMKilled"],
        "restart_count": details["RestartCount"],
    });
    for (field, label) in LABELS {
        facts[field] = json!(labels[label].as_str().unwrap_or_default());
    }

    facts
}

fn follow_events(cancel: &Cancel, sink: &mut impl Sink) -> Result<(), String> {
    let filters = json!({ "type": ["container"], "event": EVENT_ACTIONS }).to_string();
    let response = request(cancel, &format!("/events?filters={}", encode(&filters)))?;
    let mut body = BufReader::with_capacity(READ_BUFFER, response.body);
    let mut line = Vec::new();

    loop {
        line.clear();
        let read = (&mut body)
            .take(MAX_EVENT_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(read_error)?;
        if read == 0 {
            return Ok(());
        }
        if line.len() > MAX_EVENT_BYTES {
            return Err("docker sent an event over 1 MB".to_string());
        }

        let Ok(event) = serde_json::from_slice::<Value>(&line) else { continue };
        let attributes = &event["Actor"]["Attributes"];
        let reduced = json!({
            "Action": event["Action"],
            "Actor": {
                "ID": event["Actor"]["ID"],
                "Attributes": {
                    "exitCode": attributes["exitCode"].as_str().unwrap_or_default(),
                    "signal": attributes["signal"].as_str().unwrap_or_default(),
                },
            },
        });
        sink.out((reduced.to_string() + "\n").as_bytes())
            .map_err(write_error)?;
    }
}

fn follow_logs(
    logs: &LogsRead,
    cancel: &Cancel,
    sink: &mut impl Sink,
) -> Result<(), String> {
    let details = get_json(cancel, &format!("/containers/{}/json", logs.container))?;
    let has_tty = details["Config"]["Tty"].as_bool().unwrap_or(false);

    if let Some(files) = docker_file::find(&details) {
        let mut is_running = || is_running(cancel, &logs.container);
        if docker_file::follow(
            &files,
            logs,
            &|| cancel.is_cancelled(),
            &mut is_running,
            sink,
        )? {
            return Ok(());
        }
    }

    let mut path =
        format!("/containers/{}/logs?stdout=1&stderr=1&timestamps=1", logs.container);
    if logs.follow {
        path.push_str("&follow=1");
    }
    if let Some(tail) = logs.tail {
        path.push_str(&format!("&tail={tail}"));
    }
    if let Some(since) = &logs.since {
        path.push_str(&format!("&since={since}"));
    }

    let response = request(cancel, &path)?;
    let mut body = BufReader::with_capacity(READ_BUFFER, response.body);
    if has_tty {
        return copy_raw(&mut body, sink);
    }

    copy_multiplexed(&mut body, sink)
}

fn is_running(cancel: &Cancel, container: &str) -> bool {
    get_json(cancel, &format!("/containers/{container}/json"))
        .is_ok_and(|details| details["State"]["Running"].as_bool().unwrap_or(false))
}

fn copy_raw(body: &mut impl Read, sink: &mut impl Sink) -> Result<(), String> {
    let mut buffer = vec![0; MAX_FRAME_BYTES];
    loop {
        let read = body.read(&mut buffer).map_err(read_error)?;
        if read == 0 {
            return Ok(());
        }

        sink.out(&buffer[..read]).map_err(write_error)?;
    }
}

fn copy_multiplexed(body: &mut impl Read, sink: &mut impl Sink) -> Result<(), String> {
    let mut header = [0; 8];
    let mut buffer = vec![0; MAX_FRAME_BYTES];

    loop {
        if !read_header(body, &mut header)? {
            return Ok(());
        }

        let mut left =
            u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
        while left > 0 {
            let chunk = &mut buffer[..left.min(MAX_FRAME_BYTES)];
            body.read_exact(chunk).map_err(read_error)?;
            left -= chunk.len();
            match header[0] {
                1 => sink.out(chunk),
                2 => sink.err(chunk),
                _ => Ok(()),
            }
            .map_err(write_error)?;
        }
    }
}

fn read_header(body: &mut impl Read, header: &mut [u8; 8]) -> Result<bool, String> {
    let mut filled = 0;
    while filled < header.len() {
        match body.read(&mut header[filled..]).map_err(read_error)? {
            0 if filled == 0 => return Ok(false),
            0 => return Err("docker ended a log frame early".to_string()),
            read => filled += read,
        }
    }

    Ok(true)
}

struct Response {
    body: Body,
}

enum Framing {
    Length(u64),
    Chunked { left: u64, done: bool },
    Close,
}

struct Body {
    reader: BufReader<UnixStream>,
    framing: Framing,
}

impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        match &mut self.framing {
            Framing::Close => self.reader.read(buffer),
            Framing::Length(left) => {
                if *left == 0 {
                    return Ok(0);
                }

                let most = buffer.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let read = self.reader.read(&mut buffer[..most])?;
                if read == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                *left -= read as u64;
                Ok(read)
            }
            Framing::Chunked { left, done } => {
                if *done {
                    return Ok(0);
                }
                if *left == 0 {
                    let size = read_chunk_size(&mut self.reader)?;
                    if size == 0 {
                        skip_trailers(&mut self.reader)?;
                        *done = true;
                        return Ok(0);
                    }
                    *left = size;
                }

                let most = buffer.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let read = self.reader.read(&mut buffer[..most])?;
                if read == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                *left -= read as u64;
                if *left == 0 {
                    expect_line_end(&mut self.reader)?;
                }
                Ok(read)
            }
        }
    }
}

fn request(cancel: &Cancel, path: &str) -> Result<Response, String> {
    let mut socket = cancel.connect()?;
    let head = format!(
        "GET {path} HTTP/1.1\r\nHost: docker\r\nUser-Agent: daroyan-docker-read\r\nConnection: close\r\n\r\n"
    );
    io::Write::write_all(&mut socket, head.as_bytes()).map_err(write_error)?;

    let mut reader = BufReader::with_capacity(READ_BUFFER, socket);
    let mut head_bytes = 0;
    let status_line = read_head_line(&mut reader, &mut head_bytes)?;
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or("docker sent a response without a status")?;

    let mut length = None;
    let mut is_chunked = false;
    loop {
        let line = read_head_line(&mut reader, &mut head_bytes)?;
        if line.is_empty() {
            break;
        }

        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = value.parse::<u64>().ok();
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            is_chunked = value.eq_ignore_ascii_case("chunked");
        }
    }

    let framing = match (is_chunked, length) {
        (true, _) => Framing::Chunked { left: 0, done: false },
        (false, Some(length)) => Framing::Length(length),
        (false, None) => Framing::Close,
    };
    let body = Body { reader, framing };
    if !(200..300).contains(&status) {
        return Err(daemon_error(body));
    }

    Ok(Response { body })
}

fn get_json(cancel: &Cancel, path: &str) -> Result<Value, String> {
    let response = request(cancel, path)?;
    let mut text = Vec::new();
    response
        .body
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut text)
        .map_err(read_error)?;
    if text.len() as u64 > MAX_BODY_BYTES {
        return Err("docker sent an answer over 32 MB".to_string());
    }

    serde_json::from_slice(&text)
        .map_err(|error| format!("docker sent bad JSON: {error}"))
}

fn daemon_error(body: Body) -> String {
    let mut text = Vec::new();
    let _ = body.take(MAX_LINE_BYTES as u64).read_to_end(&mut text);
    let message = serde_json::from_slice::<Value>(&text)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| String::from_utf8_lossy(&text).trim().to_string());

    format!("Error response from daemon: {message}")
}

fn read_head_line(reader: &mut impl BufRead, used: &mut usize) -> Result<String, String> {
    let mut line = Vec::new();
    let read = reader
        .by_ref()
        .take((MAX_LINE_BYTES + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(read_error)?;
    *used += read;
    if read == 0 || line.len() > MAX_LINE_BYTES || *used > MAX_HEAD_BYTES {
        return Err("docker sent a bad response head".to_string());
    }

    Ok(String::from_utf8_lossy(&line).trim_end().to_string())
}

fn read_chunk_size(reader: &mut impl BufRead) -> io::Result<u64> {
    let mut line = Vec::new();
    reader
        .by_ref()
        .take(MAX_LINE_BYTES as u64)
        .read_until(b'\n', &mut line)?;
    let text = std::str::from_utf8(&line).map_err(io::Error::other)?;
    let size = text.split(';').next().unwrap_or_default().trim();

    u64::from_str_radix(size, 16).map_err(io::Error::other)
}

fn skip_trailers(reader: &mut impl BufRead) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(MAX_LINE_BYTES as u64)
            .read_until(b'\n', &mut line)?;
        if read == 0 || line == b"\r\n" || line == b"\n" {
            return Ok(());
        }
    }
}

fn expect_line_end(reader: &mut impl BufRead) -> io::Result<()> {
    let mut line = Vec::new();
    reader.by_ref().take(2).read_until(b'\n', &mut line)?;
    if line != b"\r\n" && line != b"\n" {
        return Err(io::Error::other("docker sent a bad chunk"));
    }

    Ok(())
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn read_error(error: io::Error) -> String {
    format!("read from docker: {error}")
}

fn write_error(error: io::Error) -> String {
    format!("write the answer: {error}")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Read};

    use serde_json::json;

    use super::{Sink, copy_multiplexed, encode, facts, filters, read_chunk_size};
    use crate::docker_read_protocol::ListTarget;

    #[derive(Default)]
    struct Collected {
        out: Vec<u8>,
        err: Vec<u8>,
    }

    impl Sink for Collected {
        fn out(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.out.extend_from_slice(data);
            Ok(())
        }

        fn err(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.err.extend_from_slice(data);
            Ok(())
        }
    }

    fn frame(stream: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![stream, 0, 0, 0];
        bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn splits_multiplexed_logs_into_stdout_and_stderr() {
        let mut body = frame(1, b"2026-11-06T13:00:00.1Z out\n");
        body.extend(frame(2, b"2026-11-06T13:00:00.2Z err\n"));
        body.extend(frame(0, b"ignored"));
        let big = vec![b'x'; 40 * 1024];
        body.extend(frame(1, &big));

        let mut sink = Collected::default();
        copy_multiplexed(&mut body.as_slice(), &mut sink).unwrap();

        assert!(sink.out.starts_with(b"2026-11-06T13:00:00.1Z out\n"));
        assert_eq!(sink.out.len(), 27 + big.len());
        assert_eq!(sink.err, b"2026-11-06T13:00:00.2Z err\n");

        let mut cut = frame(1, b"whole");
        cut.truncate(10);
        assert!(
            copy_multiplexed(&mut cut.as_slice(), &mut Collected::default()).is_err()
        );
    }

    #[test]
    fn builds_list_filters_and_encodes_them() {
        let target = ListTarget {
            key: "shop".to_string(),
            filters: vec![
                "label=com.docker.compose.project=shop".to_string(),
                "name=^/?app-.*$".to_string(),
            ],
        };
        let filters = filters(&target);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&filters).unwrap(),
            json!({ "label": ["com.docker.compose.project=shop"], "name": ["^/?app-.*$"] })
        );
        assert_eq!(encode("a b/{\"x\"}"), "a%20b%2F%7B%22x%22%7D");
    }

    #[test]
    fn keeps_only_the_allowlisted_inspect_fields() {
        let details = json!({
            "Id": "4f2a",
            "Name": "/app-red",
            "Image": "sha256:1",
            "RestartCount": 2,
            "Config": {
                "Image": "alpine:3.20",
                "Env": ["SECRET=1"],
                "Cmd": ["sh"],
                "Labels": { "com.docker.compose.project": "shop" },
            },
            "State": {
                "Running": true,
                "StartedAt": "2026-11-06T13:00:00Z",
                "FinishedAt": "0001-01-01T00:00:00Z",
                "ExitCode": 0,
                "OOMKilled": false,
            },
        });
        let facts = facts(&details);
        let text = facts.to_string();

        assert!(!text.contains("SECRET") && !text.contains("\"sh\""));
        assert_eq!(facts["name"], "/app-red");
        assert_eq!(facts["project"], "shop");
        assert_eq!(facts["service"], "");
        assert_eq!(facts["restart_count"], 2);
    }

    #[test]
    fn reads_chunk_sizes() {
        let mut reader = BufReader::new("1a;name=x\r\n".as_bytes());
        assert_eq!(read_chunk_size(&mut reader).unwrap(), 26);
        let mut bad = BufReader::new("zz\r\n".as_bytes());
        assert!(read_chunk_size(&mut bad).is_err());
        let mut rest = String::new();
        reader.read_to_string(&mut rest).unwrap();
        assert!(rest.is_empty());
    }
}
