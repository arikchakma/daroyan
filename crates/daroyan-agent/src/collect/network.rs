use daroyan_protocol::report::NicState;
use sysinfo::Networks;

use crate::collect::per_sec;

const VIRTUAL_PREFIXES: [&str; 8] =
    ["lo", "docker", "veth", "br-", "virbr", "cni", "flannel", "cali"];

pub fn collect_nics(
    networks: &Networks,
    allow: &[String],
    elapsed_secs: f64,
) -> Vec<NicState> {
    networks
        .iter()
        .filter(|(name, _)| is_reported(name, allow))
        .map(|(name, data)| NicState {
            name: name.clone(),
            net_in_speed: per_sec(data.received(), elapsed_secs),
            net_out_speed: per_sec(data.transmitted(), elapsed_secs),
            net_in_transfer: data.total_received(),
            net_out_transfer: data.total_transmitted(),
        })
        .collect()
}

pub fn is_reported(name: &str, allow: &[String]) -> bool {
    if allow.is_empty() {
        !VIRTUAL_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
    } else {
        allow.iter().any(|allowed| allowed == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_virtual_interfaces_unless_allowed() {
        assert!(is_reported("eth0", &[]));
        assert!(is_reported("ens5", &[]));
        assert!(!is_reported("lo", &[]));
        assert!(!is_reported("docker0", &[]));
        assert!(!is_reported("veth12ab", &[]));
        assert!(!is_reported("br-3f2a", &[]));
        assert!(!is_reported("eth1", &["eth0".to_string()]));
        assert!(is_reported("eth0", &["eth0".to_string()]));
    }
}
