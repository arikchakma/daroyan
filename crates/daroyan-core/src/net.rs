use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tokio::net::lookup_host;

static ALLOW_PRIVATE: AtomicBool = AtomicBool::new(false);

pub fn allow_private_targets(allow: bool) {
    ALLOW_PRIVATE.store(allow, Ordering::Relaxed);
}

pub fn is_reachable(ip: IpAddr) -> bool {
    ALLOW_PRIVATE.load(Ordering::Relaxed) || is_public(ip)
}

pub fn is_public(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (18..20).contains(&b)))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let [0x64, 0xff9b, 0, 0, 0, 0, hi, lo] = segments {
        return is_public_v4(Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo)));
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || matches!(segments, [0x2001, 0xdb8, ..]))
}

pub fn blocked_error(host: &str) -> String {
    format!(
        "{host} points to a private address. Set ALLOW_PRIVATE_TARGETS=true to reach it"
    )
}

pub async fn resolve_reachable(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    let addresses: Vec<SocketAddr> = lookup_host((host, port)).await?.collect();
    let reachable: Vec<SocketAddr> = addresses
        .iter()
        .copied()
        .filter(|address| is_reachable(address.ip()))
        .collect();
    if reachable.is_empty() && !addresses.is_empty() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, blocked_error(host)));
    }
    Ok(reachable)
}

pub struct ReachableResolver;

impl Resolve for ReachableResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses = resolve_reachable(name.as_str(), 0).await?;
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses_are_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111", "64:ff9b::808:808"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }
}
