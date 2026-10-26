use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};

use daroyan_protocol::report::PortState;

use crate::config::PortCheck;

const TIMEOUT: Duration = Duration::from_secs(3);

pub fn check_ports(checks: &[PortCheck]) -> Vec<PortState> {
    thread::scope(|scope| {
        let handles: Vec<_> =
            checks.iter().map(|check| scope.spawn(|| check_port(check))).collect();
        handles.into_iter().filter_map(|handle| handle.join().ok()).collect()
    })
}

fn check_port(check: &PortCheck) -> PortState {
    let started = Instant::now();
    let open = (check.host.as_str(), check.port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, TIMEOUT).is_ok());

    PortState {
        name: check.name.clone(),
        host: check.host.clone(),
        port: check.port,
        open,
        latency_ms: open.then(|| started.elapsed().as_millis() as u32),
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn reports_open_and_closed_ports() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let open_port = listener.local_addr().unwrap().port();
        let closed_port =
            TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

        let samples = check_ports(&[
            PortCheck {
                name: None,
                host: "127.0.0.1".to_string(),
                port: open_port,
            },
            PortCheck {
                name: None,
                host: "127.0.0.1".to_string(),
                port: closed_port,
            },
        ]);

        assert!(samples[0].open);
        assert!(samples[0].latency_ms.is_some());
        assert!(!samples[1].open);
        assert!(samples[1].latency_ms.is_none());
    }
}
