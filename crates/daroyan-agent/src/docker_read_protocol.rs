pub const EVENT_ACTIONS: [&str; 5] = ["start", "die", "kill", "stop", "oom"];
pub const ALLOW_FILE: &str = "/etc/daroyan/docker-read.allow";
pub const HELPER_NAME: &str = "daroyan-docker-read";

const LIST_FORMAT: &str = "{{.ID}} {{.Names}} {{.State}}";
const INSPECT_FORMAT: &str = concat!(
    r#"{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Config.Image}},"#,
    r#""image_id":{{json .Image}},"running":{{json .State.Running}},"#,
    r#""started_at":{{json .State.StartedAt}},"finished_at":{{json .State.FinishedAt}},"#,
    r#""exit_code":{{json .State.ExitCode}},"oom_killed":{{json .State.OOMKilled}},"#,
    r#""restart_count":{{json .RestartCount}},"#,
    r#""project":{{json (index .Config.Labels "com.docker.compose.project")}},"#,
    r#""service":{{json (index .Config.Labels "com.docker.compose.service")}},"#,
    r#""config_hash":{{json (index .Config.Labels "com.docker.compose.config-hash")}}}"#,
);
const EVENTS_FORMAT: &str = concat!(
    r#"{"Action":{{json .Action}},"Actor":{"ID":{{json .Actor.ID}},"Attributes":{"#,
    r#""exitCode":{{json (index .Actor.Attributes "exitCode")}},"#,
    r#""signal":{{json (index .Actor.Attributes "signal")}}}}}"#,
);
const MAX_TAIL: u32 = 100_000;
const MAX_CONTAINERS: usize = 256;
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
    pub fn docker_args(&self) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        match self {
            Self::List(target) => {
                args.extend(
                    ["ps", "--all", "--no-trunc", "--format", LIST_FORMAT]
                        .map(String::from),
                );
                for filter in &target.filters {
                    args.extend(["--filter".to_string(), filter.clone()]);
                }
            }
            Self::Inspect(containers) => {
                args.extend(["inspect", "--format", INSPECT_FORMAT].map(String::from));
                args.extend(containers.iter().cloned());
            }
            Self::Logs(logs) => {
                args.extend(["logs", "--timestamps"].map(String::from));
                args.extend(logs.options());
                args.push(logs.container.clone());
            }
            Self::Events => {
                args.extend(["events", "--filter", "type=container"].map(String::from));
                for action in EVENT_ACTIONS {
                    args.extend(["--filter".to_string(), format!("event={action}")]);
                }
                args.extend(["--format".to_string(), EVENTS_FORMAT.to_string()]);
            }
        }

        args
    }

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

#[cfg(test)]
mod tests {
    use super::{
        DockerRead, ListTarget, LogsRead, check_since, parse_targets, render_targets,
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
    fn asks_docker_only_for_allowlisted_fields() {
        let inspect = DockerRead::Inspect(vec!["app-red".to_string()])
            .docker_args()
            .join(" ");
        for field in ["Env", "Cmd", "Args", "{{json .}}"] {
            assert!(!inspect.contains(field), "{field}");
        }
        assert!(!DockerRead::Events.docker_args().join(" ").contains("{{json .}}"));
        assert!(
            !DockerRead::List(targets().remove(0))
                .docker_args()
                .join(" ")
                .contains("Command")
        );
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
