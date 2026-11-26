use std::path::Path;

use serde_json::Value;

pub const ACCESS_KEY: &str = "daroyan/access";
const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";
const COMMANDS: [&str; 15] = [
    "sshd",
    "sshd-session",
    "sshd-auth",
    "sudo",
    "su",
    "login",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "groupmod",
    "gpasswd",
    "passwd",
    "chpasswd",
];
const NOISE: [&str; 17] = [
    "Received disconnect from ",
    "Disconnected from user ",
    "Connection reset by ",
    "Timeout before authentication",
    "Unable to negotiate with ",
    "error: kex_exchange_identification: ",
    "banner exchange: ",
    "pam_unix(sudo:session): ",
    "pam_unix(login:auth): authentication failure",
    "PAM unable to dlopen(",
    "PAM adding faulty module: ",
    "pam_unix(su:session): ",
    "pam_unix(su-l:session): ",
    "pam_unix(sshd:session): session opened ",
    "pam_unix(sshd:auth): check pass; user unknown",
    "pam_unix(sshd:auth): authentication failure",
    "PAM service(sshd) ignoring max retries",
];
const SETUID_COMMANDS: [&str; 4] = ["sudo", "su", "passwd", "gpasswd"];
const SYSTEM_DIRS: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
const UNNAMED_CLOSES: [&str; 2] = ["Connection closed by ", "Disconnected from "];
const NAMED_CLOSES: [&str; 2] = ["authenticating user ", "invalid user "];

pub fn has_journal() -> bool {
    Path::new(JOURNAL_SOCKET).exists()
}

pub fn journal_matches() -> impl Iterator<Item = String> {
    COMMANDS.iter().map(|command| format!("_COMM={command}"))
}

fn field<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

fn is_system_binary(entry: &Value, command: &str) -> bool {
    field(entry, "_EXE").is_some_and(|exe| {
        SYSTEM_DIRS.iter().any(|dir| {
            exe.strip_prefix(dir).and_then(|rest| rest.strip_prefix('/')) == Some(command)
        })
    })
}

fn is_privileged(entry: &Value, command: &str) -> bool {
    if field(entry, "_UID") == Some("0") {
        return true;
    }
    if !SETUID_COMMANDS.contains(&command) {
        return false;
    }

    let has_capabilities = field(entry, "_CAP_EFFECTIVE")
        .and_then(|caps| u64::from_str_radix(caps, 16).ok())
        .is_some_and(|caps| caps != 0);
    has_capabilities && is_system_binary(entry, command)
}

fn is_noise(message: &str) -> bool {
    let message = message.trim_start();
    if NOISE.iter().any(|noise| message.starts_with(noise)) {
        return true;
    }

    UNNAMED_CLOSES
        .iter()
        .filter_map(|close| message.strip_prefix(close))
        .any(|rest| !NAMED_CLOSES.iter().any(|named| rest.starts_with(named)))
}

pub fn tag(entry: &Value, message: &str) -> Option<String> {
    let command = field(entry, "_COMM").filter(|command| COMMANDS.contains(command))?;
    let pid = field(entry, "_PID").and_then(|pid| pid.parse::<u32>().ok())?;
    if !is_privileged(entry, command) || is_noise(message) {
        return None;
    }

    Some(format!("{command}[{pid}]: {message}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{is_noise, journal_matches, tag};

    #[test]
    fn tags_privileged_entries_with_the_trusted_command() {
        let sshd = json!({ "_COMM": "sshd", "_PID": "812", "_UID": "0" });
        assert_eq!(
            tag(&sshd, "Accepted publickey for arik from 203.0.113.7 port 52144 ssh2")
                .as_deref(),
            Some(
                "sshd[812]: Accepted publickey for arik from 203.0.113.7 port 52144 ssh2"
            )
        );

        let sudo = json!({
            "_COMM": "sudo",
            "_PID": "512712",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "1ffffffffff",
            "_EXE": "/usr/bin/sudo",
        });
        assert!(tag(&sudo, "  tester : USER=root ; COMMAND=/usr/bin/true").is_some());
    }

    #[test]
    fn drops_entries_an_unprivileged_user_could_write() {
        let logger = json!({
            "_COMM": "logger",
            "SYSLOG_IDENTIFIER": "sshd",
            "_PID": "9",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "0",
        });
        assert!(
            tag(&logger, "Accepted password for root from 6.6.6.6 port 1 ssh2").is_none()
        );

        let renamed = json!({
            "_COMM": "sshd",
            "_PID": "9",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "0",
        });
        assert!(
            tag(&renamed, "Accepted password for root from 6.6.6.6 port 1 ssh2")
                .is_none()
        );

        let namespaced = json!({
            "_COMM": "sshd",
            "_PID": "9",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "1ffffffffff",
            "_EXE": "/usr/bin/python3.12",
        });
        assert!(
            tag(&namespaced, "Accepted password for root from 9.9.9.9 port 1 ssh2")
                .is_none()
        );

        let bind_mounted = json!({
            "_COMM": "sudo",
            "_PID": "9",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "1ffffffffff",
        });
        assert!(tag(&bind_mounted, "  eve : USER=root ; COMMAND=/bin/forged").is_none());

        let elsewhere = json!({
            "_COMM": "sudo",
            "_PID": "9",
            "_UID": "1000",
            "_CAP_EFFECTIVE": "1ffffffffff",
            "_EXE": "/home/eve/sudo",
        });
        assert!(tag(&elsewhere, "  eve : USER=root ; COMMAND=/bin/forged").is_none());

        let unknown = json!({ "_COMM": "sshd", "_PID": "9" });
        assert!(tag(&unknown, "Accepted password for root").is_none());

        let no_pid = json!({ "_COMM": "sshd", "_UID": "0" });
        assert!(tag(&no_pid, "Accepted password for root").is_none());
    }

    #[test]
    fn drops_noise_but_keeps_named_failures() {
        assert!(is_noise("Connection closed by 45.155.205.233 port 41022 [preauth]"));
        assert!(is_noise("Received disconnect from 45.155.205.233 port 41022:11: Bye"));
        assert!(is_noise(
            "pam_unix(sudo:session): session opened for user root(uid=0) by (uid=1000)"
        ));
        assert!(is_noise(
            "pam_unix(su-l:session): session opened for user tester(uid=1000) by (uid=0)"
        ));
        assert!(!is_noise("pam_unix(sshd:session): session closed for user arik"));
        assert!(is_noise("PAM adding faulty module: pam_lastlog.so"));
        assert!(!is_noise(
            "pam_unix(login:session): session opened for user tester(uid=1000) by tester(uid=0)"
        ));
        assert!(!is_noise(
            "Connection closed by authenticating user root 45.155.205.233 port 41022 [preauth]"
        ));
        assert!(!is_noise(
            "Connection closed by invalid user ubnt 45.155.205.233 port 1"
        ));
        assert!(!is_noise("Invalid user oracle from 45.155.205.233 port 41022"));
        assert!(!is_noise("Server listening on 0.0.0.0 port 22."));
    }

    #[test]
    fn matches_each_command_by_its_trusted_name() {
        let matches: Vec<String> = journal_matches().collect();
        assert!(matches.contains(&"_COMM=sshd-session".to_string()));
        assert!(matches.iter().all(|matched| matched.starts_with("_COMM=")));
    }
}
