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
    pub backup_dir: Option<PathBuf>,
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
            backup_dir: read_optional("DUCKDB_BACKUP_DIR").map(PathBuf::from),
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
}
