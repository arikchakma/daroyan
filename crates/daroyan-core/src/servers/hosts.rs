use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use daroyan_protocol::host::Host;
use sea_orm::DbErr;

use crate::db::Database;

const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct HostIndex {
    full: HashMap<String, String>,
    short: HashMap<String, String>,
}

impl HostIndex {
    pub fn find(&self, host_name: &str) -> Option<&str> {
        let name = normalize(host_name)?;
        self.full
            .get(&name)
            .or_else(|| self.short.get(get_short_name(&name)))
            .map(String::as_str)
    }
}

impl FromIterator<(String, Host)> for HostIndex {
    fn from_iter<T: IntoIterator<Item = (String, Host)>>(servers: T) -> Self {
        let names: Vec<(String, String)> = servers
            .into_iter()
            .filter_map(|(id, host)| Some((id, normalize(&host.hostname)?)))
            .collect();

        let mut full = HashMap::new();
        for (id, name) in &names {
            full.entry(name.clone()).or_insert_with(|| id.clone());
        }

        // A server reports `web-1` and an app may report `web-1.internal`, or
        // the other way round. The short name is only safe to match when one
        // server owns it.
        let mut owners: HashMap<&str, Vec<&str>> = HashMap::new();
        for (id, name) in &names {
            owners.entry(get_short_name(name)).or_default().push(id);
        }
        let short = owners
            .into_iter()
            .filter(|(short, ids)| {
                ids.len() == 1 && full.get(*short).is_none_or(|id| id == ids[0])
            })
            .map(|(short, ids)| (short.to_string(), ids[0].to_string()))
            .collect();

        Self { full, short }
    }
}

fn normalize(host_name: &str) -> Option<String> {
    let name = host_name.trim().trim_end_matches('.').to_ascii_lowercase();
    let is_useless = name.is_empty()
        || name == "localhost"
        || name.starts_with("localhost.")
        || name.parse::<IpAddr>().is_ok();
    (!is_useless).then_some(name)
}

fn get_short_name(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

#[derive(Clone, Default)]
pub struct HostCache {
    cached: Arc<Mutex<Option<(Instant, Arc<HostIndex>)>>>,
}

impl HostCache {
    pub async fn read(&self, database: &Database) -> Result<Arc<HostIndex>, DbErr> {
        if let Some((loaded_at, index)) = &*self.lock()
            && loaded_at.elapsed() < CACHE_TTL
        {
            return Ok(Arc::clone(index));
        }

        let index: Arc<HostIndex> = Arc::new(
            database
                .list_server_hosts()
                .await?
                .into_iter()
                .filter_map(|(id, host)| Some((id, serde_json::from_value(host).ok()?)))
                .collect(),
        );
        *self.lock() = Some((Instant::now(), Arc::clone(&index)));
        Ok(index)
    }

    pub fn forget(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> MutexGuard<'_, Option<(Instant, Arc<HostIndex>)>> {
        self.cached.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::host::Host;

    use super::HostIndex;

    fn host(hostname: &str) -> Host {
        Host {
            name: hostname.to_string(),
            labels: Vec::new(),
            hostname: hostname.to_string(),
            platform: "linux".to_string(),
            platform_version: String::new(),
            kernel: String::new(),
            arch: "x86_64".to_string(),
            cpu: String::new(),
            cpu_cores: 2,
            mem_total: 0,
            swap_total: 0,
            private_ip: None,
            boot_time: 0,
            version: "0.0.5".to_string(),
        }
    }

    fn index(servers: &[(&str, &str)]) -> HostIndex {
        servers
            .iter()
            .map(|(id, hostname)| (id.to_string(), host(hostname)))
            .collect()
    }

    #[test]
    fn matches_host_names_with_or_without_a_domain() {
        let index = index(&[("srv_1", "Inboxjot"), ("srv_2", "web-1.internal")]);

        assert_eq!(index.find("inboxjot"), Some("srv_1"));
        assert_eq!(index.find("inboxjot.example.com."), Some("srv_1"));
        assert_eq!(index.find("web-1"), Some("srv_2"));
        assert_eq!(index.find("WEB-1.internal"), Some("srv_2"));
        assert_eq!(index.find("4f1c2a9b7e3d"), None);
    }

    #[test]
    fn prefers_the_exact_host_name_over_another_server_short_name() {
        let index = index(&[("srv_b", "web-1.internal"), ("srv_a", "web-1")]);

        assert_eq!(index.find("web-1"), Some("srv_a"));
        assert_eq!(index.find("web-1.internal"), Some("srv_b"));
    }

    #[test]
    fn skips_short_names_that_two_servers_share() {
        let index = index(&[("srv_us", "web-1.us"), ("srv_eu", "web-1.eu")]);

        assert_eq!(index.find("web-1.eu"), Some("srv_eu"));
        assert_eq!(index.find("web-1"), None);
        assert_eq!(index.find("web-1.eu.prod"), None);
    }

    #[test]
    fn ignores_addresses_and_localhost() {
        let index = index(&[("srv_1", "10.0.0.5"), ("srv_2", "localhost")]);

        assert_eq!(index.find("10.0.0.5"), None);
        assert_eq!(index.find("10"), None);
        assert_eq!(index.find("localhost"), None);
    }
}
