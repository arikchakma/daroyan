use std::env;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use anyhow::Result;
use ipnet::IpNet;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub duckdb_path: PathBuf,
    pub retention_days: u32,
    pub log_retention_days: u32,
    pub otlp_log_budget: u64,
    pub metrics: MetricSettings,
    pub backup_dir: Option<PathBuf>,
    pub duckdb_max_bytes: Option<u64>,
    pub duckdb_memory_bytes: u64,
    pub archive: Option<ArchiveConfig>,
    pub trusted_proxies: Vec<IpNet>,
    pub client_ip_header: String,
    pub app_url: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub allowed_emails: Vec<String>,
    pub slack_api_url: String,
    pub slack_client_id: Option<String>,
    pub slack_client_secret: Option<String>,
    pub discord_client_id: Option<String>,
    pub discord_client_secret: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub allow_private_targets: bool,
}

#[derive(Clone, Debug)]
pub struct ArchiveConfig {
    pub bucket: String,
    pub prefix: String,
    pub endpoint: Option<String>,
    pub region: String,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub retention_days: u32,
    pub cache_dir: PathBuf,
    pub cache_bytes: u64,
    pub range_reads: bool,
}

impl ArchiveConfig {
    pub fn from_env() -> Result<Option<Self>> {
        let Some(url) = read_optional("ARCHIVE_URL") else { return Ok(None) };
        let (bucket, prefix) = parse_archive_url(&url)?;

        Ok(Some(Self {
            bucket,
            prefix,
            endpoint: read_optional("ARCHIVE_ENDPOINT"),
            region: read_optional("ARCHIVE_REGION").unwrap_or_else(|| "auto".to_string()),
            access_key_id: read_optional("ARCHIVE_ACCESS_KEY_ID"),
            secret_access_key: read_optional("ARCHIVE_SECRET_ACCESS_KEY"),
            retention_days: read_number("ARCHIVE_RETENTION_DAYS", 90)?,
            cache_dir: read_optional("ARCHIVE_CACHE_DIR")
                .map_or_else(|| PathBuf::from("data/archive-cache"), PathBuf::from),
            cache_bytes: read_number::<u64>("ARCHIVE_CACHE_GB", 2)? * 1024 * 1024 * 1024,
            range_reads: read_optional("ARCHIVE_RANGE_READS")
                .is_some_and(|value| value == "true"),
        }))
    }
}

pub fn parse_archive_url(url: &str) -> Result<(String, String)> {
    let rest = url
        .strip_prefix("s3://")
        .ok_or_else(|| anyhow::anyhow!("ARCHIVE_URL must start with s3://"))?;
    let (bucket, prefix) = rest.split_once('/').unwrap_or((rest, ""));
    if bucket.is_empty() {
        anyhow::bail!("ARCHIVE_URL must name a bucket, like s3://daroyan-logs");
    }

    Ok((bucket.to_string(), prefix.trim_matches('/').to_string()))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: read_number("HOST", IpAddr::from(Ipv4Addr::LOCALHOST))?,
            port: read_number("PORT", 8900)?,
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/daroyan.db?mode=rwc".to_string()),
            duckdb_path: env::var("DUCKDB_PATH")
                .map_or_else(|_| PathBuf::from("data/daroyan.duckdb"), PathBuf::from),
            retention_days: read_number("RETENTION_DAYS", 30)?,
            log_retention_days: read_number("LOG_RETENTION_DAYS", 7)?,
            otlp_log_budget: read_number::<u64>("OTLP_LOG_BUDGET_KB", 1024)? * 1024,
            metrics: MetricSettings::from_env()?,
            backup_dir: read_optional("DUCKDB_BACKUP_DIR").map(PathBuf::from),
            duckdb_max_bytes: read_optional("DUCKDB_MAX_SIZE_GB")
                .map(|value| value.parse::<u64>())
                .transpose()
                .map_err(|_| anyhow::anyhow!("DUCKDB_MAX_SIZE_GB must be a number"))?
                .filter(|gigabytes| *gigabytes > 0)
                .map(|gigabytes| gigabytes * 1024 * 1024 * 1024),
            duckdb_memory_bytes: match read_number::<u64>("DUCKDB_MEMORY_MB", 512)? {
                0 => anyhow::bail!("DUCKDB_MEMORY_MB must be more than 0"),
                megabytes => megabytes.saturating_mul(1024 * 1024),
            },
            archive: ArchiveConfig::from_env()?,
            trusted_proxies: parse_trusted_proxies(
                &env::var("TRUSTED_PROXIES")
                    .unwrap_or_else(|_| "127.0.0.1,::1".to_string()),
            )?,
            client_ip_header: env::var("CLIENT_IP_HEADER")
                .unwrap_or_else(|_| "cf-connecting-ip".to_string())
                .to_lowercase(),
            app_url: read_optional("APP_URL"),
            google_client_id: read_optional("GOOGLE_CLIENT_ID"),
            google_client_secret: read_optional("GOOGLE_CLIENT_SECRET"),
            allowed_emails: env::var("ALLOWED_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect(),
            slack_api_url: read_optional("SLACK_API_URL")
                .unwrap_or_else(|| "https://slack.com".to_string()),
            slack_client_id: read_optional("SLACK_CLIENT_ID"),
            slack_client_secret: read_optional("SLACK_CLIENT_SECRET"),
            discord_client_id: read_optional("DISCORD_CLIENT_ID"),
            discord_client_secret: read_optional("DISCORD_CLIENT_SECRET"),
            telegram_bot_token: read_optional("TELEGRAM_BOT_TOKEN"),
            allow_private_targets: read_optional("ALLOW_PRIVATE_TARGETS")
                .is_some_and(|value| value == "true"),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MetricSettings {
    pub retention_days: u32,
    pub rollup_retention_days: u32,
    pub series_limit: usize,
    pub series_per_metric: usize,
    pub point_budget: u64,
}

impl Default for MetricSettings {
    fn default() -> Self {
        Self {
            retention_days: 7,
            rollup_retention_days: 30,
            series_limit: 10_000,
            series_per_metric: 2_000,
            point_budget: 100_000,
        }
    }
}

impl MetricSettings {
    fn from_env() -> Result<Self> {
        let defaults = Self::default();

        Self {
            retention_days: read_number(
                "METRIC_RETENTION_DAYS",
                defaults.retention_days,
            )?,
            rollup_retention_days: read_number(
                "METRIC_ROLLUP_RETENTION_DAYS",
                defaults.rollup_retention_days,
            )?,
            series_limit: read_number("METRIC_SERIES_LIMIT", defaults.series_limit)?,
            series_per_metric: read_number(
                "METRIC_SERIES_PER_METRIC",
                defaults.series_per_metric,
            )?,
            point_budget: read_number("METRIC_POINT_BUDGET", defaults.point_budget)?,
        }
        .validate()
    }

    fn validate(self) -> Result<Self> {
        if self.rollup_retention_days < self.retention_days {
            anyhow::bail!(
                "METRIC_ROLLUP_RETENTION_DAYS ({}) must be at least METRIC_RETENTION_DAYS ({})",
                self.rollup_retention_days,
                self.retention_days
            );
        }

        Ok(self)
    }
}

fn read_optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn read_number<T: std::str::FromStr>(name: &str, default: T) -> Result<T> {
    match env::var(name) {
        Ok(value) => {
            value.parse().map_err(|_| anyhow::anyhow!("{name} must be a number"))
        }
        Err(_) => Ok(default),
    }
}

pub fn parse_trusted_proxies(value: &str) -> Result<Vec<IpNet>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.parse::<IpNet>()
                .or_else(|_| item.parse::<IpAddr>().map(IpNet::from))
                .map_err(|_| {
                    anyhow::anyhow!("TRUSTED_PROXIES has an invalid entry: {item}")
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ips_and_networks() {
        let proxies = parse_trusted_proxies("127.0.0.1, ::1,172.16.0.0/12").unwrap();
        assert!(
            proxies
                .iter()
                .any(|net| net.contains(&"172.18.0.5".parse::<IpAddr>().unwrap()))
        );
        assert!(
            proxies
                .iter()
                .any(|net| net.contains(&"::1".parse::<IpAddr>().unwrap()))
        );
        assert!(parse_trusted_proxies("nope").is_err());
    }

    #[test]
    fn splits_the_archive_url_into_bucket_and_prefix() {
        assert_eq!(
            parse_archive_url("s3://daroyan-logs").unwrap(),
            ("daroyan-logs".to_string(), String::new())
        );
        assert_eq!(
            parse_archive_url("s3://daroyan-logs/prod/eu/").unwrap(),
            ("daroyan-logs".to_string(), "prod/eu".to_string())
        );
        assert!(parse_archive_url("https://daroyan-logs").is_err());
        assert!(parse_archive_url("s3:///prod").is_err());
    }

    #[test]
    fn keeps_metric_rollups_at_least_as_long_as_points() {
        let settings = |retention_days, rollup_retention_days| MetricSettings {
            retention_days,
            rollup_retention_days,
            ..MetricSettings::default()
        };

        assert!(settings(7, 30).validate().is_ok());
        assert!(settings(7, 7).validate().is_ok());
        assert!(settings(30, 7).validate().is_err());
    }
}
