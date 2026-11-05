use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::de::{self, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub api: Api,
    #[serde(default)]
    pub server: Server,
    #[serde(default)]
    pub processes: Vec<ProcessCheck>,
    #[serde(default)]
    pub ports: Vec<PortCheck>,
    #[serde(default)]
    pub files: Vec<FileLog>,
    #[serde(default)]
    pub host: HostConfig,
    #[serde(default)]
    pub agent: Agent,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Api {
    #[serde(default = "default_api_url")]
    pub url: String,
    pub token: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub name: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Systemd(String),
    Docker(DockerTarget),
    Process(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerTarget {
    Name(String),
    Labels(BTreeMap<String, String>),
}

pub const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
pub const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Systemd(value) | Self::Process(value) => formatter.write_str(value),
            Self::Docker(target) => target.fmt(formatter),
        }
    }
}

impl fmt::Display for DockerTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => formatter.write_str(name),
            Self::Labels(labels) => {
                let pairs: Vec<String> =
                    labels.iter().map(|(key, value)| format!("{key}={value}")).collect();
                formatter.write_str(&pairs.join(","))
            }
        }
    }
}

impl TryFrom<RawDocker> for DockerTarget {
    type Error = String;

    fn try_from(raw: RawDocker) -> Result<Self, Self::Error> {
        let selector = match raw {
            RawDocker::Name(name) => return Ok(Self::Name(name)),
            RawDocker::Selector(selector) => selector,
        };

        let mut labels = selector.labels.unwrap_or_default();
        if let Some((key, _)) = labels.iter().find(|(key, _)| !is_label_key(key)) {
            return Err(format!(
                "docker label {key:?} is not a valid key. A key cannot be empty or contain = or a comma"
            ));
        }

        if let Some(compose) = selector.compose {
            if labels.contains_key(COMPOSE_PROJECT_LABEL)
                || labels.contains_key(COMPOSE_SERVICE_LABEL)
            {
                return Err(format!(
                    "docker sets compose: {compose} and a compose label in labels. Use only compose"
                ));
            }

            let (project, service) = parse_compose(&compose)?;
            labels.insert(COMPOSE_PROJECT_LABEL.to_string(), project.to_string());
            labels.insert(COMPOSE_SERVICE_LABEL.to_string(), service.to_string());
        }

        if labels.is_empty() {
            return Err("docker needs a container name, compose or labels".to_string());
        }

        Ok(Self::Labels(labels))
    }
}

fn is_label_key(key: &str) -> bool {
    !key.trim().is_empty() && !key.contains('=') && !key.contains(',')
}

fn parse_compose(compose: &str) -> Result<(&str, &str), String> {
    let invalid = || {
        format!(
            "compose: {compose} is not valid. Write it as project/service, for example shop/web"
        )
    };

    let (project, service) = compose.split_once('/').ok_or_else(invalid)?;
    let is_part = |part: &str| !part.is_empty() && !part.contains(['/', '=', ',']);
    if !is_part(project) || !is_part(service) {
        return Err(invalid());
    }

    Ok((project, service))
}

#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "RawProcessCheck")]
pub struct ProcessCheck {
    pub name: String,
    pub source: Source,
    pub user: Option<String>,
    pub restart: Option<Restart>,
    pub logs: Option<LogSettings>,
    pub min_running: Option<u32>,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LogSettings {
    pub format: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawLogs {
    Enabled(bool),
    Settings(LogSettings),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct FileLog {
    pub name: String,
    pub path: PathBuf,
    pub format: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProcessCheck {
    name: Option<String>,
    systemd: Option<String>,
    docker: Option<RawDocker>,
    process: Option<String>,
    user: Option<String>,
    restart: Option<Restart>,
    logs: Option<RawLogs>,
    min_running: Option<u32>,
}

enum RawDocker {
    Name(String),
    Selector(RawSelector),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSelector {
    compose: Option<String>,
    labels: Option<BTreeMap<String, String>>,
}

impl<'de> Deserialize<'de> for RawDocker {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RawDockerVisitor;

        impl<'de> Visitor<'de> for RawDockerVisitor {
            type Value = RawDocker;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a container name, or a map with compose or labels")
            }

            fn visit_str<E: de::Error>(self, name: &str) -> Result<Self::Value, E> {
                Ok(RawDocker::Name(name.to_string()))
            }

            fn visit_map<M: MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                RawSelector::deserialize(MapAccessDeserializer::new(map))
                    .map(RawDocker::Selector)
            }
        }

        deserializer.deserialize_any(RawDockerVisitor)
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct Restart {
    pub attempts: u32,
    #[serde(
        default = "default_restart_window",
        deserialize_with = "deserialize_duration"
    )]
    pub within: Duration,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PortCheck {
    pub name: Option<String>,
    #[serde(default = "default_port_host")]
    pub host: String,
    pub port: u16,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    #[serde(default)]
    pub disks: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    #[serde(default)]
    pub commands: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_path")]
    pub log_path: PathBuf,
    #[serde(default = "default_state_path")]
    pub state_path: PathBuf,
    #[serde(default = "default_log_budget", deserialize_with = "deserialize_size")]
    pub log_budget: u64,
}

impl TryFrom<RawProcessCheck> for ProcessCheck {
    type Error = String;

    fn try_from(raw: RawProcessCheck) -> Result<Self, Self::Error> {
        let source = match (raw.systemd, raw.docker, raw.process) {
            (Some(unit), None, None) => Source::Systemd(unit),
            (None, Some(docker), None) => Source::Docker(DockerTarget::try_from(docker)?),
            (None, None, Some(pattern)) => Source::Process(pattern),
            (None, None, None) => {
                return Err(
                    "a process needs one of systemd, docker or process".to_string()
                );
            }
            _ => {
                return Err(
                    "a process takes only one of systemd, docker or process".to_string()
                );
            }
        };
        if matches!(source, Source::Docker(DockerTarget::Labels(_))) && raw.name.is_none()
        {
            return Err(format!(
                "docker {source} matches containers by label, so it needs a name"
            ));
        }

        let name = raw.name.unwrap_or_else(|| source.to_string());
        if matches!(source, Source::Process(_)) && raw.restart.is_some() {
            return Err(format!(
                "{name} has restart, but a process entry is watch-only. Use systemd or docker to restart it"
            ));
        }
        let logs = match raw.logs {
            None | Some(RawLogs::Enabled(false)) => None,
            Some(RawLogs::Enabled(true)) => Some(LogSettings::default()),
            Some(RawLogs::Settings(settings)) => Some(settings),
        };
        if matches!(source, Source::Process(_)) && logs.is_some() {
            return Err(format!(
                "{name} has logs, but a process entry has no fixed output. Use systemd or docker to read its logs"
            ));
        }
        if raw.restart.is_some_and(|restart| restart.attempts == 0) {
            return Err(format!(
                "{name} has restart.attempts 0. Leave restart out instead"
            ));
        }
        if raw.min_running.is_some() && !matches!(source, Source::Docker(_)) {
            return Err(format!(
                "{name} has min_running, but only a docker entry can run more than one container"
            ));
        }
        if raw.min_running == Some(0) {
            return Err(format!(
                "{name} has min_running 0. Use 1 or more, or leave min_running out"
            ));
        }
        Ok(Self {
            name,
            source,
            user: raw.user,
            restart: raw.restart,
            logs,
            min_running: raw.min_running,
        })
    }
}

impl Default for Api {
    fn default() -> Self {
        Self { url: default_api_url(), token: None }
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            commands: false,
            log_level: default_log_level(),
            log_path: default_log_path(),
            state_path: default_state_path(),
            log_budget: default_log_budget(),
        }
    }
}

fn default_api_url() -> String {
    "https://daroyan.com".to_string()
}

fn default_port_host() -> String {
    "localhost".to_string()
}

fn default_restart_window() -> Duration {
    Duration::from_secs(600)
}

fn default_log_budget() -> u64 {
    1024 * 1024
}

fn deserialize_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_size(&text).map_err(de::Error::custom)
}

fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let invalid =
        || format!("{text} is not a size. Use a number with KB or MB, for example 2MB");
    let upper = text.to_ascii_uppercase();
    let (number, unit) = match upper.strip_suffix("MB") {
        Some(number) => (number, 1024 * 1024),
        None => match upper.strip_suffix("KB") {
            Some(number) => (number, 1024),
            None => (upper.as_str(), 1),
        },
    };
    let value: u64 = number.trim().parse().map_err(|_| invalid())?;
    if value == 0 {
        return Err(invalid());
    }
    value.checked_mul(unit).ok_or_else(invalid)
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_path() -> PathBuf {
    PathBuf::from("/var/log/daroyan")
}

fn default_state_path() -> PathBuf {
    PathBuf::from("/var/lib/daroyan")
}

fn deserialize_duration<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Duration, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_duration(&text).map_err(de::Error::custom)
}

fn parse_duration(text: &str) -> Result<Duration, String> {
    let invalid = || format!("{text:?} is not a duration. Write it like 30s, 10m or 1h");
    let split = text.len().checked_sub(1).ok_or_else(invalid)?;
    let (number, unit) = text.split_at(split);
    let number: u64 = number.parse().map_err(|_| invalid())?;
    let unit_secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return Err(invalid()),
    };
    let secs = number.checked_mul(unit_secs).ok_or_else(invalid)?;
    Ok(Duration::from_secs(secs))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Snippet {
    #[serde(default)]
    processes: Vec<ProcessCheck>,
    #[serde(default)]
    ports: Vec<PortCheck>,
    #[serde(default)]
    files: Vec<FileLog>,
}

fn read_yaml<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read the config file {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(T::default());
    }
    serde_norway::from_str(&text)
        .with_context(|| format!("parse the config file {}", path.display()))
}

fn list_snippets(dir: &Path) -> Result<Vec<PathBuf>> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn load_config(path: &Path) -> Result<Config> {
    let mut config: Config = read_yaml(path)?;
    let dir = path.parent().unwrap_or(Path::new(".")).join("conf.d");
    for snippet_path in list_snippets(&dir)? {
        let snippet: Snippet = read_yaml(&snippet_path)?;
        config.processes.extend(snippet.processes);
        config.ports.extend(snippet.ports);
        config.files.extend(snippet.files);
    }
    check_names(config)
}

#[cfg(test)]
pub fn parse_config(text: &str) -> Result<Config> {
    if text.trim().is_empty() {
        return Ok(Config::default());
    }
    check_names(serde_norway::from_str(text)?)
}

fn check_names(config: Config) -> Result<Config> {
    let mut names = HashSet::new();
    for check in &config.processes {
        if !names.insert(&check.name) {
            bail!("two processes are named {}. Give each a different name", check.name);
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {

    #[test]
    fn reads_log_settings() {
        let config = parse_config(
            "processes:\n  - docker: api-*\n    logs: true\nfiles:\n  - name: Caddy access\n    path: /var/log/caddy/access.log\nagent:\n  log_budget: 2MB\n",
        )
        .unwrap();
        assert_eq!(config.processes[0].logs, Some(LogSettings::default()));
        let formats = parse_config(
            "processes:\n  - systemd: nginx\n    logs:\n      format: nginx\n  - docker: web\n    logs: false\nfiles:\n  - name: App\n    path: /var/log/app.log\n    format: laravel\n",
        )
        .unwrap();
        assert_eq!(
            formats.processes[0].logs.as_ref().unwrap().format.as_deref(),
            Some("nginx")
        );
        assert_eq!(formats.processes[1].logs, None);
        assert_eq!(formats.files[0].format.as_deref(), Some("laravel"));
        assert!(
            parse_config(
                "processes:\n  - systemd: nginx\n    logs:\n      fromat: nginx\n"
            )
            .is_err()
        );
        assert_eq!(config.files[0].name, "Caddy access");
        assert_eq!(config.agent.log_budget, 2 * 1024 * 1024);
        assert_eq!(parse_config("").unwrap().agent.log_budget, 1024 * 1024);
        assert!(parse_config("processes:\n  - process: node\n    logs: true\n").is_err());
        assert_eq!(parse_size("512KB"), Ok(512 * 1024));
        assert!(parse_size("lots").is_err());
        assert!(parse_size("99999999999999MB").is_err());
        assert_eq!(parse_duration("2h"), Ok(Duration::from_secs(7200)));
        assert!(parse_duration("9999999999999999999h").is_err());
    }

    #[test]
    fn merges_snippets_from_conf_d() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-conf-{}", std::process::id()));
        fs::create_dir_all(dir.join("conf.d")).unwrap();
        fs::write(dir.join("agent.yml"), "server:\n  name: web\nports:\n  - port: 443\n")
            .unwrap();
        fs::write(dir.join("conf.d/b.yml"), "processes:\n  - docker: maily-*\n").unwrap();
        fs::write(
            dir.join("conf.d/a.yml"),
            "processes:\n  - docker: linkity-*\nports:\n  - port: 6379\n",
        )
        .unwrap();
        fs::write(dir.join("conf.d/notes.txt"), "ignored").unwrap();
        let config = load_config(&dir.join("agent.yml")).unwrap();
        let names: Vec<_> =
            config.processes.iter().map(|check| check.name.as_str()).collect();
        assert_eq!(names, ["linkity-*", "maily-*"]);
        assert_eq!(config.ports.len(), 2);

        fs::write(dir.join("conf.d/c.yml"), "processes:\n  - docker: maily-*\n").unwrap();
        assert!(load_config(&dir.join("agent.yml")).is_err());
        fs::remove_dir_all(dir).ok();
    }
    use super::*;

    #[test]
    fn parses_the_add_server_example() {
        let config = parse_config(
            "
server:
  name: worker-2
  labels: [production, worker]

processes:
  - name: Inboxing Worker
    systemd: inboxing-worker
    restart:
      attempts: 3
  - name: Redis
    docker: redis-cache
  - process: containerd-shim-runc-v2
    user: root

ports:
  - name: Redis
    host: redis-1
    port: 6379
  - port: 443

host:
  networks: [eth0]
",
        )
        .unwrap();

        assert_eq!(config.server.name.as_deref(), Some("worker-2"));
        assert_eq!(config.server.labels, ["production", "worker"]);
        assert_eq!(
            config.processes[0].source,
            Source::Systemd("inboxing-worker".to_string())
        );
        let restart = config.processes[0].restart.unwrap();
        assert_eq!((restart.attempts, restart.within), (3, Duration::from_secs(600)));
        assert_eq!(
            config.processes[1].source,
            Source::Docker(DockerTarget::Name("redis-cache".to_string()))
        );
        assert_eq!(config.processes[2].name, "containerd-shim-runc-v2");
        assert_eq!(config.processes[2].user.as_deref(), Some("root"));
        assert_eq!(config.ports[0].name.as_deref(), Some("Redis"));
        assert_eq!(config.ports[1].host, "localhost");
        assert_eq!(config.host.networks, ["eth0"]);
        assert!(config.host.disks.is_empty());
        assert!(!config.agent.commands);
        assert_eq!(config.agent.state_path, PathBuf::from("/var/lib/daroyan"));
    }

    #[test]
    fn an_empty_file_is_valid() {
        let config = parse_config("").unwrap();
        assert!(config.processes.is_empty());
        assert_eq!(config.api.url, "https://daroyan.com");
        assert_eq!(config.agent.log_level, "info");
    }

    #[test]
    fn rejects_unclear_process_entries() {
        let error = |text: &str| format!("{:#}", parse_config(text).unwrap_err());

        assert!(error("processes:\n  - name: x\n").contains("needs one of"));
        assert!(
            error("processes:\n  - systemd: a\n    docker: b\n").contains("only one of")
        );
        assert!(
            error("processes:\n  - process: a\n    restart:\n      attempts: 3\n")
                .contains("watch-only")
        );
        assert!(
            error("processes:\n  - systemd: a\n  - docker: a\n")
                .contains("two processes")
        );
        assert!(
            error("processes:\n  - systemd: a\n    unit: b\n").contains("unknown field")
        );
    }

    #[test]
    fn parses_docker_selectors() {
        let config = parse_config(
            "
processes:
  - name: IMG CN Web
    docker:
      compose: img-cn/web
    logs: true
  - name: Shop
    docker:
      labels:
        service: shop
        role: web
  - name: Blue
    docker:
      compose: img-cn/web
      labels:
        color: blue
",
        )
        .unwrap();

        let labels = |pairs: &[(&str, &str)]| {
            Source::Docker(DockerTarget::Labels(
                pairs
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            ))
        };
        assert_eq!(
            config.processes[0].source,
            labels(&[(COMPOSE_PROJECT_LABEL, "img-cn"), (COMPOSE_SERVICE_LABEL, "web")])
        );
        assert_eq!(
            config.processes[1].source,
            labels(&[("role", "web"), ("service", "shop")])
        );
        assert_eq!(
            config.processes[2].source,
            labels(&[
                (COMPOSE_PROJECT_LABEL, "img-cn"),
                (COMPOSE_SERVICE_LABEL, "web"),
                ("color", "blue"),
            ])
        );
        assert_eq!(config.processes[1].source.to_string(), "role=web,service=shop");
    }

    #[test]
    fn rejects_unclear_docker_selectors() {
        let error = |docker: &str| {
            let text = format!("processes:\n  - name: x\n    docker:\n{docker}");
            format!("{:#}", parse_config(&text).unwrap_err())
        };

        assert!(
            format!(
                "{:#}",
                parse_config("processes:\n  - docker:\n      compose: a/b\n")
                    .unwrap_err()
            )
            .contains("needs a name")
        );
        assert!(error("      compose: web\n").contains("project/service"));
        assert!(error("      compose: a/b/c\n").contains("project/service"));
        assert!(error("      compose: /web\n").contains("project/service"));
        assert!(error("      labels: {}\n").contains("needs a container name"));
        assert!(error("      image: nginx\n").contains("unknown field"));
        assert!(error("      labels:\n        a=b: c\n").contains("not a valid key"));
        assert!(
            error(
                "      compose: a/b\n      labels:\n        com.docker.compose.service: c\n"
            )
            .contains("Use only compose")
        );
        assert!(error("      - web\n").contains("a container name, or a map"));
    }

    #[test]
    fn takes_min_running_only_on_docker_entries() {
        let parse =
            |entry: &str| parse_config(&format!("processes:\n  - name: x\n{entry}"));
        let error = |entry: &str| format!("{:#}", parse(entry).unwrap_err());

        let config =
            parse("    docker:\n      compose: shop/worker\n    min_running: 3\n")
                .unwrap();
        assert_eq!(config.processes[0].min_running, Some(3));
        assert_eq!(parse("    docker: web\n").unwrap().processes[0].min_running, None);

        assert!(error("    docker: web\n    min_running: 0\n").contains("min_running 0"));
        assert!(
            error("    systemd: nginx\n    min_running: 2\n")
                .contains("only a docker entry")
        );
        assert!(
            error("    process: node\n    min_running: 2\n")
                .contains("only a docker entry")
        );
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("10m"), Ok(Duration::from_secs(600)));
        assert_eq!(parse_duration("1h"), Ok(Duration::from_secs(3600)));
        assert!(parse_duration("10").is_err());
        assert!(parse_duration("").is_err());
        assert!(parse_duration("m").is_err());
    }
}
