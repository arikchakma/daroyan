use std::fs;

pub fn count_connections() -> (u32, u32) {
    let tcp = count_lines("/proc/net/tcp") + count_lines("/proc/net/tcp6");
    let udp = count_lines("/proc/net/udp") + count_lines("/proc/net/udp6");
    (tcp, udp)
}

fn count_lines(path: &str) -> u32 {
    fs::read_to_string(path).map_or(0, |text| text.lines().skip(1).count() as u32)
}
