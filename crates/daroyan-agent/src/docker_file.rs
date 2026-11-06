use std::borrow::Cow;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;

use crate::docker_api::Sink;
use crate::docker_read_protocol::LogsRead;

const MAX_RECORD_BYTES: usize = 1024 * 1024;
const READ_BUFFER: usize = 16 * 1024;
const POLL_EVERY: Duration = Duration::from_millis(250);
const CHECK_EVERY: Duration = Duration::from_secs(2);
const MAX_ROTATED: usize = 64;

#[derive(Deserialize)]
struct Record<'a> {
    #[serde(borrow)]
    log: Cow<'a, str>,
    stream: &'a str,
    time: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LogFiles {
    current: PathBuf,
    rotated: Vec<PathBuf>,
}

pub fn find(details: &Value) -> Option<LogFiles> {
    if details["HostConfig"]["LogConfig"]["Type"].as_str() != Some("json-file") {
        return None;
    }

    let id = details["Id"].as_str()?;
    let is_id = id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit());
    let current = PathBuf::from(details["LogPath"].as_str()?);
    let name = |path: Option<&Path>| path?.file_name()?.to_str().map(str::to_string);
    let is_own_file = name(Some(&current)) == Some(format!("{id}-json.log"))
        && name(current.parent()) == Some(id.to_string())
        && name(current.parent().and_then(Path::parent))
            == Some("containers".to_string());
    if !is_id || !current.is_absolute() || !is_own_file {
        return None;
    }

    let mut rotated = Vec::new();
    for number in 1..=MAX_ROTATED {
        let path = PathBuf::from(format!("{}.{number}", current.display()));
        if Path::new(&format!("{}.gz", path.display())).exists() {
            return None;
        }
        if !path.exists() {
            break;
        }
        rotated.push(path);
    }
    rotated.reverse();

    Some(LogFiles { current, rotated })
}

pub fn follow(
    files: &LogFiles,
    logs: &LogsRead,
    is_cancelled: &dyn Fn() -> bool,
    is_running: &mut dyn FnMut() -> bool,
    sink: &mut impl Sink,
) -> Result<bool, String> {
    let Ok(current) = open(&files.current) else { return Ok(false) };
    let mut readers: Vec<BufReader<File>> = files
        .rotated
        .iter()
        .filter_map(|path| open(path).ok())
        .map(|file| BufReader::with_capacity(READ_BUFFER, file))
        .collect();
    readers.push(BufReader::with_capacity(READ_BUFFER, current));

    if let Some(tail) = logs.tail {
        let (first, offset) = find_tail(&mut readers, u64::from(tail), is_cancelled)?;
        readers.drain(..first);
        for (index, reader) in readers.iter_mut().enumerate() {
            let start = if index == 0 { offset } else { 0 };
            reader.seek(SeekFrom::Start(start)).map_err(read_error)?;
        }
    }

    let mut lines = Lines {
        line: Vec::new(),
        is_too_long: false,
        since: logs.since.as_deref().and_then(parse_since),
        is_cancelled,
        sink,
    };
    let mut current = readers.pop().ok_or("no log file")?;
    for mut reader in readers {
        if !lines.drain(&mut reader)? {
            return Ok(true);
        }
        lines.line.clear();
    }

    let mut identity = identify(current.get_ref()).map_err(read_error)?;
    let mut checked = Instant::now();
    loop {
        if !lines.drain(&mut current)? || !logs.follow {
            return Ok(true);
        }

        if let Ok(metadata) = fs::symlink_metadata(&files.current)
            && (metadata.dev(), metadata.ino()) != identity
        {
            if !lines.drain(&mut current)? {
                return Ok(true);
            }
            let Ok(next) = open(&files.current) else {
                thread::sleep(POLL_EVERY);
                continue;
            };
            identity = identify(&next).map_err(read_error)?;
            current = BufReader::with_capacity(READ_BUFFER, next);
            lines.line.clear();
            continue;
        }
        if current.get_ref().metadata().map_err(read_error)?.len()
            < current.stream_position().map_err(read_error)?
        {
            current.seek(SeekFrom::Start(0)).map_err(read_error)?;
            lines.line.clear();
        }

        if checked.elapsed() >= CHECK_EVERY {
            checked = Instant::now();
            if !is_running() {
                lines.drain(&mut current)?;
                return Ok(true);
            }
        }

        thread::sleep(POLL_EVERY);
    }
}

struct Lines<'a, S> {
    line: Vec<u8>,
    is_too_long: bool,
    since: Option<i128>,
    is_cancelled: &'a dyn Fn() -> bool,
    sink: &'a mut S,
}

impl<S: Sink> Lines<'_, S> {
    fn drain(&mut self, reader: &mut impl BufRead) -> Result<bool, String> {
        while next_line(reader, &mut self.line, &mut self.is_too_long)
            .map_err(read_error)?
        {
            if (self.is_cancelled)() {
                return Ok(false);
            }
            emit(&self.line, self.since, self.sink)?;
            self.line.clear();
        }

        Ok(!(self.is_cancelled)())
    }
}

fn open(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("the log path is not a file"));
    }

    Ok(file)
}

fn identify(file: &File) -> io::Result<(u64, u64)> {
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

fn find_tail(
    readers: &mut [BufReader<File>],
    tail: u64,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(usize, u64), String> {
    let mut left = tail;
    let mut buffer = vec![0; READ_BUFFER];

    for index in (0..readers.len()).rev() {
        let file = readers[index].get_mut();
        let mut end = file.metadata().map_err(read_error)?.len();
        let mut newlines = 0;
        while end > 0 {
            if is_cancelled() {
                return Err("the read was closed".to_string());
            }

            let size =
                usize::try_from(end).map_or(buffer.len(), |end| end.min(buffer.len()));
            end -= size as u64;
            file.seek(SeekFrom::Start(end)).map_err(read_error)?;
            file.read_exact(&mut buffer[..size]).map_err(read_error)?;
            for (at, byte) in buffer[..size].iter().enumerate().rev() {
                if *byte != b'\n' {
                    continue;
                }
                newlines += 1;
                if newlines > left {
                    return Ok((index, end + at as u64 + 1));
                }
            }
        }

        left -= newlines;
        if left == 0 {
            return Ok((index, 0));
        }
    }

    Ok((0, 0))
}

fn next_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    is_too_long: &mut bool,
) -> io::Result<bool> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let used = newline.map_or(available.len(), |index| index + 1);
        if !*is_too_long && line.len() + used > MAX_RECORD_BYTES {
            *is_too_long = true;
            line.clear();
        }
        if !*is_too_long {
            line.extend_from_slice(&available[..used]);
        }
        reader.consume(used);

        if newline.is_some() {
            if *is_too_long {
                *is_too_long = false;
                continue;
            }
            return Ok(true);
        }
    }
}

fn emit(line: &[u8], since: Option<i128>, sink: &mut impl Sink) -> Result<(), String> {
    let Ok(record) = serde_json::from_slice::<Record>(line) else { return Ok(()) };
    if let Some(since) = since {
        let at = DateTime::parse_from_rfc3339(record.time)
            .ok()
            .and_then(|time| time.timestamp_nanos_opt());
        if at.is_none_or(|at| i128::from(at) < since) {
            return Ok(());
        }
    }

    let mut text = Vec::with_capacity(record.time.len() + 1 + record.log.len());
    text.extend_from_slice(record.time.as_bytes());
    text.push(b' ');
    text.extend_from_slice(record.log.as_bytes());
    match record.stream {
        "stderr" => sink.err(&text),
        _ => sink.out(&text),
    }
    .map_err(|error| format!("write the answer: {error}"))
}

fn parse_since(since: &str) -> Option<i128> {
    let (seconds, fraction) = since.split_once('.').unwrap_or((since, "0"));
    let seconds = seconds.parse::<i128>().ok()?;
    let nanos = format!("{fraction:0<9}").get(..9)?.parse::<i128>().ok()?;

    Some(seconds * 1_000_000_000 + nanos)
}

fn read_error(error: io::Error) -> String {
    format!("read the log file: {error}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    use serde_json::json;

    use super::{LogFiles, find, follow, parse_since};
    use crate::docker_api::Sink;
    use crate::docker_read_protocol::LogsRead;

    const ID: &str = "4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a9c4f2a";

    #[derive(Default)]
    struct Collected {
        out: String,
        err: String,
    }

    impl Sink for Collected {
        fn out(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.out.push_str(&String::from_utf8_lossy(data));
            Ok(())
        }

        fn err(&mut self, data: &[u8]) -> std::io::Result<()> {
            self.err.push_str(&String::from_utf8_lossy(data));
            Ok(())
        }
    }

    fn record(at: u32, text: &str, stream: &str) -> String {
        json!({ "log": text, "stream": stream, "time": format!("2026-11-06T14:00:{at:02}.5Z") })
            .to_string()
            + "\n"
    }

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("daroyan-file-{name}-{}", std::process::id()))
            .join("containers")
            .join(ID);
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn details(log_path: &Path) -> serde_json::Value {
        json!({
            "Id": ID,
            "LogPath": log_path.to_str().unwrap(),
            "HostConfig": { "LogConfig": { "Type": "json-file" } },
        })
    }

    fn read(files: &LogFiles, tail: Option<u32>, since: Option<&str>) -> Collected {
        let logs = LogsRead {
            container: ID.to_string(),
            follow: false,
            tail,
            since: since.map(str::to_string),
        };
        let mut collected = Collected::default();
        let handled =
            follow(files, &logs, &|| false, &mut || true, &mut collected).unwrap();
        assert!(handled);
        collected
    }

    #[test]
    fn reads_rotated_files_in_order_with_tail_and_since() {
        let folder = folder("order");
        let current = folder.join(format!("{ID}-json.log"));
        fs::write(folder.join(format!("{ID}-json.log.2")), record(1, "one\n", "stdout"))
            .unwrap();
        fs::write(folder.join(format!("{ID}-json.log.1")), record(2, "two\n", "stderr"))
            .unwrap();
        let mut text = record(3, "three\n", "stdout");
        text.push_str("{not json}\n");
        text.push_str(&record(4, "four\n", "stdout"));
        text.push_str(&record(5, "partial", "stdout")[..20]);
        fs::write(&current, text).unwrap();

        let files = find(&details(&current)).unwrap();
        let all = read(&files, None, None);
        assert_eq!(
            all.out,
            "2026-11-06T14:00:01.5Z one\n2026-11-06T14:00:03.5Z three\n2026-11-06T14:00:04.5Z four\n"
        );
        assert_eq!(all.err, "2026-11-06T14:00:02.5Z two\n");

        let tail = read(&files, Some(2), None);
        assert_eq!(tail.out, "2026-11-06T14:00:04.5Z four\n");
        assert!(tail.err.is_empty());

        let spanning = read(&files, Some(4), None);
        assert_eq!(
            spanning.out,
            "2026-11-06T14:00:03.5Z three\n2026-11-06T14:00:04.5Z four\n"
        );
        assert_eq!(spanning.err, "2026-11-06T14:00:02.5Z two\n");

        let everything = read(&files, Some(100), None);
        assert_eq!(everything.out, all.out);

        let since = read(&files, None, Some("1793973603.5"));
        assert_eq!(
            since.out,
            "2026-11-06T14:00:03.5Z three\n2026-11-06T14:00:04.5Z four\n"
        );
        assert!(since.err.is_empty());
    }

    #[test]
    fn uses_the_api_for_other_drivers_compressed_files_and_odd_paths() {
        let folder = folder("refuse");
        let current = folder.join(format!("{ID}-json.log"));
        fs::write(&current, "").unwrap();

        let mut local = details(&current);
        local["HostConfig"]["LogConfig"]["Type"] = json!("local");
        assert!(find(&local).is_none());

        let mut elsewhere = details(&current);
        elsewhere["LogPath"] = json!("/etc/shadow");
        assert!(find(&elsewhere).is_none());

        fs::write(folder.join(format!("{ID}-json.log.1.gz")), "").unwrap();
        assert!(find(&details(&current)).is_none());
    }

    #[test]
    fn does_not_follow_links() {
        let folder = folder("link");
        let current = folder.join(format!("{ID}-json.log"));
        let target = folder.join("elsewhere");
        fs::write(&target, record(1, "secret\n", "stdout")).unwrap();
        std::os::unix::fs::symlink(&target, &current).unwrap();

        let files = find(&details(&current)).unwrap();
        let logs = LogsRead { container: ID.to_string(), ..LogsRead::default() };
        let mut collected = Collected::default();
        let handled =
            follow(&files, &logs, &|| false, &mut || true, &mut collected).unwrap();

        assert!(!handled);
        assert!(collected.out.is_empty());
    }

    #[test]
    fn follows_appends_and_rotation_until_the_container_stops() {
        let folder = folder("follow");
        let current = folder.join(format!("{ID}-json.log"));
        fs::write(&current, record(1, "first\n", "stdout")).unwrap();
        let files = find(&details(&current)).unwrap();

        let writer_path = current.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let mut file =
                fs::OpenOptions::new().append(true).open(&writer_path).unwrap();
            file.write_all(record(2, "second\n", "stdout").as_bytes()).unwrap();
            drop(file);
            std::thread::sleep(std::time::Duration::from_millis(300));
            fs::rename(&writer_path, format!("{}.1", writer_path.display())).unwrap();
            fs::write(&writer_path, record(3, "third\n", "stdout")).unwrap();
        });

        let logs = LogsRead {
            container: ID.to_string(),
            follow: true,
            ..LogsRead::default()
        };
        let started = std::time::Instant::now();
        let mut is_running =
            move || started.elapsed() < std::time::Duration::from_millis(2500);
        let mut collected = Collected::default();
        follow(&files, &logs, &|| false, &mut is_running, &mut collected).unwrap();
        writer.join().unwrap();

        assert_eq!(
            collected.out,
            "2026-11-06T14:00:01.5Z first\n2026-11-06T14:00:02.5Z second\n2026-11-06T14:00:03.5Z third\n"
        );
    }

    #[test]
    fn finds_the_tail_without_reading_the_whole_file() {
        let folder = folder("big");
        let current = folder.join(format!("{ID}-json.log"));
        let mut text = String::new();
        for at in 0..40_000 {
            text.push_str(&record(at % 60, "filler\n", "stdout"));
        }
        text.push_str(&record(59, "last\n", "stdout"));
        fs::write(&current, text).unwrap();
        let files = find(&details(&current)).unwrap();

        let newest = read(&files, Some(1), None);
        assert_eq!(newest.out, "2026-11-06T14:00:59.5Z last\n");

        let logs = LogsRead {
            container: ID.to_string(),
            tail: Some(1),
            ..LogsRead::default()
        };
        let cancelled =
            follow(&files, &logs, &|| true, &mut || true, &mut Collected::default());
        assert!(cancelled.is_err());
    }

    #[test]
    fn parses_unix_since_values() {
        assert_eq!(parse_since("12.5"), Some(12_500_000_000));
        assert_eq!(parse_since("12"), Some(12_000_000_000));
        assert_eq!(parse_since("1.000000001"), Some(1_000_000_001));
        assert_eq!(parse_since("x"), None);
    }
}
