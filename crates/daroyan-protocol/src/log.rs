use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SEVERITY_DEBUG: u8 = 5;
pub const SEVERITY_INFO: u8 = 9;
pub const SEVERITY_WARN: u8 = 13;
pub const SEVERITY_ERROR: u8 = 17;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogBatch {
    pub lines: Vec<LogLine>,
    #[serde(default)]
    pub dropped: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogLine {
    pub at: u64,
    pub service: String,
    pub stream: LogStream,
    pub severity: u8,
    pub message: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attributes: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(default, skip_serializing_if = "LogFormat::is_auto")]
    pub format: LogFormat,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LogFormat {
    #[default]
    Auto,
    Plain,
    Json,
    Logfmt,
    Nginx,
    Apache,
    Caddy,
    Haproxy,
    Envoy,
    Traefik,
    Laravel,
    Php,
    PhpFpm,
    Rails,
    Python,
    Java,
    Mysql,
    Postgresql,
    Mongodb,
    Redis,
    Kafka,
    Rabbitmq,
    Elasticsearch,
    System,
}

impl LogFormat {
    pub const ALL: [Self; 24] = [
        Self::Auto,
        Self::Plain,
        Self::Json,
        Self::Logfmt,
        Self::Nginx,
        Self::Apache,
        Self::Caddy,
        Self::Haproxy,
        Self::Envoy,
        Self::Traefik,
        Self::Laravel,
        Self::Php,
        Self::PhpFpm,
        Self::Rails,
        Self::Python,
        Self::Java,
        Self::Mysql,
        Self::Postgresql,
        Self::Mongodb,
        Self::Redis,
        Self::Kafka,
        Self::Rabbitmq,
        Self::Elasticsearch,
        Self::System,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Plain => "plain",
            Self::Json => "json",
            Self::Logfmt => "logfmt",
            Self::Nginx => "nginx",
            Self::Apache => "apache",
            Self::Caddy => "caddy",
            Self::Haproxy => "haproxy",
            Self::Envoy => "envoy",
            Self::Traefik => "traefik",
            Self::Laravel => "laravel",
            Self::Php => "php",
            Self::PhpFpm => "php-fpm",
            Self::Rails => "rails",
            Self::Python => "python",
            Self::Java => "java",
            Self::Mysql => "mysql",
            Self::Postgresql => "postgresql",
            Self::Mongodb => "mongodb",
            Self::Redis => "redis",
            Self::Kafka => "kafka",
            Self::Rabbitmq => "rabbitmq",
            Self::Elasticsearch => "elasticsearch",
            Self::System => "system",
        }
    }

    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

impl<'de> Deserialize<'de> for LogFormat {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(name.parse().unwrap_or_default())
    }
}

impl FromStr for LogFormat {
    type Err = UnknownLogFormat;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|format| format.as_str() == name)
            .ok_or_else(|| UnknownLogFormat(name.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLogFormat(pub String);

impl fmt::Display for UnknownLogFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let known: Vec<&str> =
            LogFormat::ALL.iter().map(|format| format.as_str()).collect();
        write!(
            formatter,
            "unknown log format {:?}, use one of: {}",
            self.0,
            known.join(", ")
        )
    }
}

impl std::error::Error for UnknownLogFormat {}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
    Journal,
    File,
}

impl LogStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Journal => "journal",
            Self::File => "file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogFormat, UnknownLogFormat};

    #[test]
    fn names_round_trip_and_unknown_reads_as_auto() {
        for format in LogFormat::ALL {
            let json = serde_json::to_string(&format).unwrap();
            assert_eq!(json, format!("\"{}\"", format.as_str()));
            assert_eq!(serde_json::from_str::<LogFormat>(&json).unwrap(), format);
            assert_eq!(format.as_str().parse(), Ok(format));
        }
        assert_eq!(
            serde_json::from_str::<LogFormat>("\"syslog-ng\"").unwrap(),
            LogFormat::Auto
        );
        assert_eq!(
            "ngnix".parse::<LogFormat>(),
            Err(UnknownLogFormat("ngnix".to_string()))
        );
    }
}
