use std::collections::HashMap;

use daroyan_protocol::host::Host;

use crate::db::entities::servers::Model as Server;

#[derive(Default)]
pub struct HostIndex {
    servers: HashMap<String, String>,
}

impl HostIndex {
    pub fn find(&self, host_name: &str) -> Option<&str> {
        let name = host_name.trim().to_ascii_lowercase();
        self.servers
            .get(&name)
            .or_else(|| self.servers.get(get_short_name(&name)))
            .map(String::as_str)
    }
}

impl From<&[Server]> for HostIndex {
    fn from(servers: &[Server]) -> Self {
        let mut index = HashMap::new();
        for server in servers {
            let Ok(host) = serde_json::from_value::<Host>(server.host.clone()) else {
                continue;
            };
            let name = host.hostname.trim().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }

            // A server reports `web-1` and an app may report `web-1.internal`,
            // or the other way round, so both forms point at the server.
            index
                .entry(get_short_name(&name).to_string())
                .or_insert(server.id.clone());
            index.entry(name).or_insert(server.id.clone());
        }

        Self { servers: index }
    }
}

fn get_short_name(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::HostIndex;
    use crate::db::entities::servers::Model as Server;

    fn server(id: &str, hostname: &str) -> Server {
        Server {
            id: id.to_string(),
            name: id.to_string(),
            labels: json!([]),
            host: json!({
                "name": id, "labels": [], "hostname": hostname, "platform": "linux",
                "platform_version": "", "kernel": "", "arch": "x86_64", "cpu": "",
                "cpu_cores": 2, "mem_total": 0, "swap_total": 0, "private_ip": null,
                "boot_time": 0, "version": "0.0.5",
            }),
            token_hash: String::new(),
            last_report: None,
            last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn matches_host_names_with_or_without_a_domain() {
        let servers = [server("srv_1", "Inboxjot"), server("srv_2", "web-1.internal")];
        let index = HostIndex::from(servers.as_slice());

        assert_eq!(index.find("inboxjot"), Some("srv_1"));
        assert_eq!(index.find("inboxjot.example.com"), Some("srv_1"));
        assert_eq!(index.find("web-1"), Some("srv_2"));
        assert_eq!(index.find("web-1.internal"), Some("srv_2"));
        assert_eq!(index.find("4f1c2a9b7e3d"), None);
    }
}
