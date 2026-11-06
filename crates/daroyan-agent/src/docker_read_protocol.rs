use std::io::{self, Read, Write};

pub const EVENT_ACTIONS: [&str; 5] = ["start", "die", "kill", "stop", "oom"];
pub const ALLOW_FILE: &str = "/etc/daroyan/docker-read.allow";
pub const HELPER_NAME: &str = "daroyan-docker-read";
pub const TARGETS_ENV: &str = "DAROYAN_DOCKER_TARGETS";

pub const MAX_FRAME_BYTES: usize = 16 * 1024;
pub const MAX_REQUEST_BYTES: usize = 4 * 1024;
pub const MAX_CONTAINERS: usize = 256;
const MAX_TAIL: u32 = 100_000;
const MAX_REF_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListTarget {
    pub key: String,
    pub filters: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockerRead {
    List(ListTarget),
    Inspect(Vec<String>),
    Logs(LogsRead),
    Events,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogsRead {
    pub container: String,
    pub follow: bool,
    pub tail: Option<u32>,
    pub since: Option<String>,
}

impl DockerRead {
    pub fn helper_args(&self) -> Vec<String> {
        match self {
            Self::List(target) => vec!["list".to_string(), target.key.clone()],
            Self::Inspect(containers) => ["inspect".to_string()]
                .into_iter()
                .chain(containers.iter().cloned())
                .collect(),
            Self::Logs(logs) => ["logs".to_string(), logs.container.clone()]
                .into_iter()
                .chain(logs.options())
                .collect(),
            Self::Events => vec!["events".to_string()],
        }
    }

    pub fn parse(args: &[String], targets: &[ListTarget]) -> Result<Self, String> {
        let Some((operation, rest)) = args.split_first() else {
            return Err("name an operation: list, inspect, logs or events".to_string());
        };

        match operation.as_str() {
            "list" => {
                let [key] = rest else {
                    return Err("list takes one configured target".to_string());
                };
                targets
                    .iter()
                    .find(|target| target.key == *key)
                    .map(|target| Self::List(target.clone()))
                    .ok_or_else(|| format!("{key} is not a docker target in the config"))
            }
            "inspect" => {
                if rest.is_empty() || rest.len() > MAX_CONTAINERS {
                    return Err(format!(
                        "inspect takes 1 to {MAX_CONTAINERS} containers"
                    ));
                }
                for container in rest {
                    check_reference(container)?;
                }

                Ok(Self::Inspect(rest.to_vec()))
            }
            "logs" => LogsRead::parse(rest).map(Self::Logs),
            "events" if rest.is_empty() => Ok(Self::Events),
            "events" => Err("events takes no arguments".to_string()),
            other => Err(format!("unknown operation {other}")),
        }
    }

    pub fn containers(&self) -> &[String] {
        match self {
            Self::Inspect(containers) => containers,
            Self::Logs(logs) => std::slice::from_ref(&logs.container),
            Self::List(_) | Self::Events => &[],
        }
    }
}

impl LogsRead {
    fn options(&self) -> Vec<String> {
        let mut options = Vec::new();
        if self.follow {
            options.push("--follow".to_string());
        }
        if let Some(tail) = self.tail {
            options.extend(["--tail".to_string(), tail.to_string()]);
        }
        if let Some(since) = &self.since {
            options.extend(["--since".to_string(), since.clone()]);
        }

        options
    }

    fn parse(args: &[String]) -> Result<Self, String> {
        let Some((container, mut rest)) = args.split_first() else {
            return Err("logs takes a container".to_string());
        };
        check_reference(container)?;
        let mut logs = Self { container: container.clone(), ..Self::default() };

        while let Some((option, after)) = rest.split_first() {
            rest = after;
            match option.as_str() {
                "--follow" if !logs.follow => logs.follow = true,
                "--tail" if logs.tail.is_none() => {
                    let (value, after) =
                        rest.split_first().ok_or("--tail needs a number")?;
                    rest = after;
                    logs.tail = Some(parse_tail(value)?);
                }
                "--since" if logs.since.is_none() => {
                    let (value, after) =
                        rest.split_first().ok_or("--since needs a time")?;
                    rest = after;
                    check_since(value)?;
                    logs.since = Some(value.clone());
                }
                other => return Err(format!("logs does not take {other} here")),
            }
        }

        Ok(logs)
    }
}

fn check_reference(reference: &str) -> Result<(), String> {
    let mut bytes = reference.bytes();
    let starts_well = bytes.next().is_some_and(|byte| byte.is_ascii_alphanumeric());
    let is_valid = starts_well
        && reference.len() <= MAX_REF_BYTES
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
        });
    if !is_valid {
        return Err(format!("{reference:?} is not a container name or ID"));
    }

    Ok(())
}

fn parse_tail(value: &str) -> Result<u32, String> {
    let is_digits = !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    value
        .parse::<u32>()
        .ok()
        .filter(|tail| is_digits && (1..=MAX_TAIL).contains(tail))
        .ok_or_else(|| format!("--tail takes a number from 1 to {MAX_TAIL}"))
}

fn check_since(value: &str) -> Result<(), String> {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, "0"));
    let is_digits = |text: &str, most: usize| {
        !text.is_empty()
            && text.len() <= most
            && text.bytes().all(|byte| byte.is_ascii_digit())
    };
    if !is_digits(seconds, 12) || !is_digits(fraction, 9) {
        return Err("--since takes a Unix time such as 1793952889.868504067".to_string());
    }

    Ok(())
}

pub fn render_targets(targets: &[ListTarget]) -> Result<String, String> {
    let mut text = String::new();
    for target in targets {
        let fields = std::iter::once(&target.key).chain(&target.filters);
        if fields.clone().any(|field| field.contains(['\t', '\n', '\r'])) {
            return Err(format!("{} has a tab or a line break", target.key));
        }

        text.push_str(&fields.map(String::as_str).collect::<Vec<_>>().join("\t"));
        text.push('\n');
    }

    Ok(text)
}

pub fn parse_targets(text: &str) -> Vec<ListTarget> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let key = fields.next().filter(|key| !key.is_empty())?.to_string();

            Some(ListTarget { key, filters: fields.map(str::to_string).collect() })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Stdout,
    Stderr,
    Exit,
}

impl FrameKind {
    fn to_byte(self) -> u8 {
        match self {
            Self::Stdout => 1,
            Self::Stderr => 2,
            Self::Exit => 3,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Stdout),
            2 => Some(Self::Stderr),
            3 => Some(Self::Exit),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub stream: u32,
    pub kind: FrameKind,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn exit_code(&self) -> i32 {
        self.data
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .map_or(1, i32::from_be_bytes)
    }
}

pub fn write_frame(
    out: &mut impl Write,
    stream: u32,
    kind: FrameKind,
    data: &[u8],
) -> io::Result<()> {
    let length = u32::try_from(data.len())
        .ok()
        .filter(|length| *length as usize <= MAX_FRAME_BYTES)
        .ok_or_else(|| io::Error::other("frame is too large"))?;

    let mut header = [0; 9];
    header[..4].copy_from_slice(&stream.to_be_bytes());
    header[4] = kind.to_byte();
    header[5..].copy_from_slice(&length.to_be_bytes());
    out.write_all(&header)?;
    out.write_all(data)?;
    out.flush()
}

pub fn write_exit(out: &mut impl Write, stream: u32, code: i32) -> io::Result<()> {
    write_frame(out, stream, FrameKind::Exit, &code.to_be_bytes())
}

pub fn read_frame(input: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut header = [0; 9];
    let mut filled = 0;
    while filled < header.len() {
        match input.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            read => filled += read,
        }
    }

    let stream = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let kind = FrameKind::from_byte(header[4])
        .ok_or_else(|| io::Error::other("unknown frame kind"))?;
    let length =
        u32::from_be_bytes([header[5], header[6], header[7], header[8]]) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::other("frame is too large"));
    }

    let mut data = vec![0; length];
    input.read_exact(&mut data)?;

    Ok(Some(Frame { stream, kind, data }))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Open { stream: u32, args: Vec<String> },
    Close { stream: u32 },
}

impl Request {
    pub fn render(&self) -> Result<String, String> {
        let fields: Vec<String> = match self {
            Self::Open { stream, args } => ["open".to_string(), stream.to_string()]
                .into_iter()
                .chain(args.iter().cloned())
                .collect(),
            Self::Close { stream } => vec!["close".to_string(), stream.to_string()],
        };
        if fields.iter().any(|field| field.contains(['\t', '\n', '\r'])) {
            return Err("a request field has a tab or a line break".to_string());
        }

        let line = fields.join("\t") + "\n";
        if line.len() > MAX_REQUEST_BYTES {
            return Err("the request is too long".to_string());
        }

        Ok(line)
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let mut fields = line.trim_end_matches(['\n', '\r']).split('\t');
        let operation = fields.next().unwrap_or_default();
        let stream = fields
            .next()
            .and_then(|stream| stream.parse::<u32>().ok())
            .ok_or("a request needs a stream number")?;

        match operation {
            "open" => {
                Ok(Self::Open { stream, args: fields.map(str::to_string).collect() })
            }
            "close" if fields.next().is_none() => Ok(Self::Close { stream }),
            other => Err(format!("unknown request {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DockerRead, Frame, FrameKind, ListTarget, LogsRead, MAX_FRAME_BYTES, Request,
        check_since, parse_targets, read_frame, render_targets, write_exit, write_frame,
    };

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    fn targets() -> Vec<ListTarget> {
        vec![
            ListTarget {
                key: "app-*".to_string(),
                filters: vec!["name=^/?app-.*$".to_string()],
            },
            ListTarget {
                key: "com.docker.compose.project=shop".to_string(),
                filters: vec!["label=com.docker.compose.project=shop".to_string()],
            },
        ]
    }

    #[test]
    fn accepts_only_configured_targets_and_safe_arguments() {
        let targets = targets();
        let parse = |args: &[&str]| DockerRead::parse(&strings(args), &targets);

        assert!(parse(&["list", "app-*"]).is_ok());
        assert!(parse(&["list", "*"]).is_err());
        assert!(parse(&["inspect", "4f2a9c", "app-red"]).is_ok());
        assert!(parse(&["inspect", "--format", "{{json .Config.Env}}"]).is_err());
        assert!(parse(&["inspect", "-f", "app-red"]).is_err());
        assert!(parse(&["inspect"]).is_err());
        assert!(parse(&["events"]).is_ok());
        assert!(parse(&["events", "--format", "x"]).is_err());
        assert!(parse(&["exec", "app-red", "sh"]).is_err());

        assert_eq!(
            parse(&["logs", "app-red", "--follow", "--tail", "10000"]).unwrap(),
            DockerRead::Logs(LogsRead {
                container: "app-red".to_string(),
                follow: true,
                tail: Some(10_000),
                since: None,
            })
        );
        assert!(parse(&["logs", "app-red", "--since", "1793952889.868504067"]).is_ok());
        assert!(parse(&["logs", "app-red", "--tail", "0"]).is_err());
        assert!(parse(&["logs", "app-red", "--tail", "+5"]).is_err());
        assert!(parse(&["logs", "app-red", "--follow", "--follow"]).is_err());
        assert!(parse(&["logs", "app-red", "--details"]).is_err());
        assert!(parse(&["logs", "app-red", "--since", "1h"]).is_err());
        assert!(parse(&["logs", "../app-red"]).is_err());
        assert!(check_since("12.").is_err());
    }

    #[test]
    fn round_trips_frames_and_rejects_large_ones() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, 7, FrameKind::Stdout, b"line\n").unwrap();
        write_exit(&mut bytes, 7, 125).unwrap();

        let mut input = bytes.as_slice();
        let first = read_frame(&mut input).unwrap().unwrap();
        assert_eq!(
            first,
            Frame {
                stream: 7,
                kind: FrameKind::Stdout,
                data: b"line\n".to_vec()
            }
        );
        let exit = read_frame(&mut input).unwrap().unwrap();
        assert_eq!((exit.kind, exit.exit_code()), (FrameKind::Exit, 125));
        assert!(read_frame(&mut input).unwrap().is_none());

        let big = vec![0; MAX_FRAME_BYTES + 1];
        assert!(write_frame(&mut Vec::new(), 1, FrameKind::Stdout, &big).is_err());
        let mut header = vec![0, 0, 0, 1, 1];
        header.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_frame(&mut header.as_slice()).is_err());
        assert!(read_frame(&mut [0, 0, 0, 1, 9, 0, 0, 0, 0].as_slice()).is_err());
    }

    #[test]
    fn round_trips_requests_and_refuses_line_breaks() {
        let open = Request::Open {
            stream: 3,
            args: strings(&["logs", "app-red", "--follow"]),
        };
        assert_eq!(Request::parse(&open.render().unwrap()).unwrap(), open);
        let close = Request::Close { stream: 3 };
        assert_eq!(Request::parse(&close.render().unwrap()).unwrap(), close);

        let bad = Request::Open { stream: 1, args: strings(&["logs", "a\nb"]) };
        assert!(bad.render().is_err());
        assert!(Request::parse("open\tx\tlogs").is_err());
        assert!(Request::parse("close\t1\textra").is_err());
        assert!(Request::parse("exec\t1").is_err());
    }

    #[test]
    fn round_trips_helper_arguments_and_the_allow_file() {
        let targets = targets();
        let reads = [
            DockerRead::List(targets[0].clone()),
            DockerRead::Inspect(strings(&["a1", "app-red"])),
            DockerRead::Logs(LogsRead {
                container: "a1".to_string(),
                follow: true,
                tail: None,
                since: Some("1793952889.000000001".to_string()),
            }),
            DockerRead::Events,
        ];
        for read in reads {
            assert_eq!(DockerRead::parse(&read.helper_args(), &targets).unwrap(), read);
        }

        assert_eq!(parse_targets(&render_targets(&targets).unwrap()), targets);
        let bad = ListTarget { key: "a\tb".to_string(), filters: Vec::new() };
        assert!(render_targets(&[bad]).is_err());
    }
}
