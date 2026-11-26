use std::net::IpAddr;

use super::ParsedLine;

const SSH_PROCESSES: [&str; 3] = ["sshd", "sshd-session", "sshd-auth"];
const ACCOUNT_PROCESSES: [&str; 9] = [
    "useradd", "userdel", "usermod", "groupadd", "groupdel", "groupmod", "gpasswd",
    "passwd", "chpasswd",
];
const SSH_FAILURES: [(&str, bool); 6] = [
    ("Invalid user ", true),
    ("Connection closed by invalid user ", true),
    ("Disconnected from invalid user ", true),
    ("Connection closed by authenticating user ", false),
    ("Disconnected from authenticating user ", false),
    ("maximum authentication attempts exceeded for ", false),
];
const SUDO_FAILURES: [&str; 4] = [
    "incorrect password",
    "NOT in sudoers",
    "a password is required",
    "command not allowed",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    SshLogin,
    SshLogout,
    Sudo,
    Su,
    UserAdd,
    UserDelete,
    UserChange,
    GroupAdd,
    GroupDelete,
    PasswordChange,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Self::SshLogin => "ssh_login",
            Self::SshLogout => "ssh_logout",
            Self::Sudo => "sudo",
            Self::Su => "su",
            Self::UserAdd => "user_add",
            Self::UserDelete => "user_delete",
            Self::UserChange => "user_change",
            Self::GroupAdd => "group_add",
            Self::GroupDelete => "group_delete",
            Self::PasswordChange => "password_change",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Success,
    Failure,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

impl ParsedLine {
    fn record(&mut self, action: Action, outcome: Outcome) {
        self.insert("event.action", action.as_str());
        self.insert("event.outcome", outcome.as_str());
        if outcome == Outcome::Failure {
            self.raise_to_warn();
        }
    }
}

struct Remote<'a> {
    user: &'a str,
    ip: IpAddr,
    port: Option<u16>,
    tail: &'a str,
}

impl<'a> Remote<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let (head, port, tail) = match text.rsplit_once(" port ") {
            Some((head, after)) => {
                let (port, tail) = after.split_once(' ').unwrap_or((after, ""));
                (head, Some(port.parse().ok()?), tail)
            }
            None => (text, None, ""),
        };
        let (user, ip) = head.rsplit_once(' ')?;
        let user = user.strip_suffix(" from").unwrap_or(user);

        Some(Self { user, ip: ip.parse().ok()?, port, tail })
    }

    fn write(&self, parsed: &mut ParsedLine) {
        let user = match self.user.strip_prefix("invalid user ") {
            Some(user) => {
                parsed.insert("ssh.invalid_user", true);
                user
            }
            None => self.user,
        };
        parsed.insert("user", user);
        parsed.insert("client.ip", self.ip.to_string());
        if let Some(port) = self.port {
            parsed.insert("client.port", port);
        }
    }
}

fn read_login(text: &str, outcome: Outcome, parsed: &mut ParsedLine) {
    let Some((method, rest)) = text.split_once(" for ") else {
        return;
    };
    if method.contains(' ') {
        return;
    }
    let Some(remote) = Remote::parse(rest) else {
        return;
    };

    remote.write(parsed);
    parsed.insert("ssh.method", method);
    let key = remote.tail.split_once(": ").map(|(_, key)| key.split_whitespace());
    if let Some(mut key) = key
        && let (Some(kind), Some(fingerprint)) = (key.next(), key.next())
    {
        parsed.insert("ssh.key.type", kind);
        parsed.insert("ssh.key.fingerprint", fingerprint);
    }
    parsed.record(Action::SshLogin, outcome);
}

struct PamSession<'a> {
    service: &'a str,
    is_open: bool,
    user: &'a str,
}

impl<'a> PamSession<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let (service, rest) =
            text.strip_prefix("pam_unix(")?.split_once(":session): session ")?;
        let (is_open, rest) = match rest.strip_prefix("opened for user ") {
            Some(rest) => (true, rest),
            None => (false, rest.strip_prefix("closed for user ")?),
        };
        let user = rest.split(['(', ' ']).next().filter(|user| !user.is_empty())?;

        Some(Self { service, is_open, user })
    }
}

pub(super) fn read_ssh(text: &str, parsed: &mut ParsedLine) {
    if parsed
        .process_name()
        .is_some_and(|name| !SSH_PROCESSES.contains(&name))
    {
        return;
    }

    let text = text.strip_suffix(" [preauth]").unwrap_or(text);
    if let Some(rest) = text.strip_prefix("Accepted ") {
        read_login(rest, Outcome::Success, parsed);
        return;
    }
    if let Some(rest) = text.strip_prefix("Failed ") {
        read_login(rest, Outcome::Failure, parsed);
        return;
    }

    if let Some((rest, is_invalid)) =
        SSH_FAILURES.iter().find_map(|(prefix, is_invalid)| {
            text.strip_prefix(prefix).map(|rest| (rest, *is_invalid))
        })
        && let Some(remote) = Remote::parse(rest)
    {
        remote.write(parsed);
        if is_invalid {
            parsed.insert("ssh.invalid_user", true);
        }
        parsed.record(Action::SshLogin, Outcome::Failure);
        return;
    }

    if let Some(session) = PamSession::parse(text)
        && session.service == "sshd"
        && !session.is_open
    {
        parsed.insert("user", session.user);
        parsed.record(Action::SshLogout, Outcome::Success);
    }
}

pub(super) fn read_sudo(text: &str, parsed: &mut ParsedLine) {
    if parsed.process_name() != Some("sudo") {
        return;
    }
    let Some((user, fields)) = text.split_once(" : ") else {
        return;
    };

    parsed.insert("user", user.trim());
    for field in fields.split(" ; ") {
        match field.trim().split_once('=') {
            Some(("USER", target)) => parsed.insert("sudo.target_user", target),
            Some(("TTY", tty)) => parsed.insert("sudo.tty", tty),
            Some(("COMMAND", command)) => parsed.insert("process.command_line", command),
            _ => {}
        }
    }

    let outcome = if SUDO_FAILURES.iter().any(|failure| fields.contains(failure)) {
        Outcome::Failure
    } else {
        Outcome::Success
    };
    parsed.record(Action::Sudo, outcome);
}

pub(super) fn read_su(text: &str, parsed: &mut ParsedLine) {
    if parsed.process_name() != Some("su") {
        return;
    }

    let (outcome, rest) = match text.strip_prefix("FAILED SU ") {
        Some(rest) => (Outcome::Failure, rest),
        None => (Outcome::Success, text),
    };
    let Some((target, rest)) =
        rest.strip_prefix("(to ").and_then(|rest| rest.split_once(") "))
    else {
        return;
    };
    let (user, tty) = rest.rsplit_once(" on ").unwrap_or((rest, ""));

    parsed.insert("user", user);
    parsed.insert("su.target_user", target);
    if !tty.is_empty() && tty != "none" {
        parsed.insert("su.tty", tty);
    }
    parsed.record(Action::Su, outcome);
}

fn quoted(text: &str) -> Option<(&str, &str)> {
    text.strip_prefix('\'')?.split_once('\'')
}

fn read_field<'a>(fields: &'a str, key: &str) -> Option<&'a str> {
    fields
        .split(", ")
        .find_map(|field| field.strip_prefix(key)?.strip_prefix('='))
}

fn read_membership(text: &str, parsed: &mut ParsedLine) -> bool {
    let Some((verb, rest)) = text.split_once(' ') else {
        return false;
    };
    let joiner = match verb {
        "add" => " to group ",
        "delete" => " from group ",
        _ => return false,
    };
    let Some((user, rest)) = quoted(rest) else {
        return false;
    };
    let Some((group, _)) = rest.strip_prefix(joiner).and_then(quoted) else {
        return false;
    };

    parsed.insert("user.target", user);
    parsed.insert("group.name", group);
    true
}

fn read_gpasswd(text: &str, parsed: &mut ParsedLine) -> bool {
    let Some(rest) = text.strip_prefix("user ") else {
        return false;
    };
    let Some((target, rest)) = rest.split_once(' ') else {
        return false;
    };
    let joined = rest
        .strip_prefix("added by ")
        .and_then(|rest| rest.split_once(" to group "))
        .or_else(|| rest.strip_prefix("removed by ")?.split_once(" from group "));
    let Some((user, group)) = joined else {
        return false;
    };

    parsed.insert("user", user);
    parsed.insert("user.target", target);
    parsed.insert("group.name", group);
    true
}

pub(super) fn read_account(text: &str, parsed: &mut ParsedLine) {
    let Some(process) = parsed.process_name() else {
        return;
    };
    if !ACCOUNT_PROCESSES.contains(&process) {
        return;
    }

    if let Some(fields) = text.strip_prefix("new user: ")
        && let Some(user) = read_field(fields, "name")
    {
        parsed.insert("user.target", user);
        parsed.record(Action::UserAdd, Outcome::Success);
        return;
    }
    if let Some(fields) = text.strip_prefix("new group: ")
        && let Some(group) = read_field(fields, "name")
    {
        parsed.insert("group.name", group);
        parsed.record(Action::GroupAdd, Outcome::Success);
        return;
    }
    if let Some((user, _)) = text.strip_prefix("delete user ").and_then(quoted) {
        parsed.insert("user.target", user);
        parsed.record(Action::UserDelete, Outcome::Success);
        return;
    }
    if let Some((group, " removed")) = text.strip_prefix("group ").and_then(quoted) {
        parsed.insert("group.name", group);
        parsed.record(Action::GroupDelete, Outcome::Success);
        return;
    }
    if let Some(user) = text
        .split_once(":chauthtok): password changed for ")
        .map(|(_, user)| user.trim())
    {
        parsed.insert("user.target", user);
        parsed.record(Action::PasswordChange, Outcome::Success);
        return;
    }
    if !text.contains(" shadow group ")
        && (read_membership(text, parsed) || read_gpasswd(text, parsed))
    {
        parsed.record(Action::UserChange, Outcome::Success);
        return;
    }
    if let Some((user, _)) = text.strip_prefix("change user ").and_then(quoted) {
        parsed.insert("user.target", user);
        parsed.record(Action::UserChange, Outcome::Success);
    }
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::LogFormat;

    use crate::analytics::log_parse::tests::parse;
    use crate::analytics::log_parse::{ParsedLine, parse_text_line};

    fn event(parsed: &ParsedLine) -> (&str, &str) {
        (
            parsed.attributes["event.action"].as_str().unwrap(),
            parsed.attributes["event.outcome"].as_str().unwrap(),
        )
    }

    #[test]
    fn reads_ssh_logins() {
        let key = parse(
            "sshd[812]: Accepted publickey for arik from 203.0.113.7 port 52144 ssh2: ED25519 SHA256:Xk2p9cQv4m1eFzA8sT0uW3yR6nB5dJ7hL2gK9aV1oPc",
        );
        assert_eq!(event(&key), ("ssh_login", "success"));
        assert_eq!(key.severity, None);
        assert_eq!(key.attributes["process.name"], "sshd");
        assert_eq!(key.attributes["process.pid"], 812);
        assert_eq!(key.attributes["user"], "arik");
        assert_eq!(key.attributes["client.ip"], "203.0.113.7");
        assert_eq!(key.attributes["client.port"], 52144);
        assert_eq!(key.attributes["ssh.method"], "publickey");
        assert_eq!(key.attributes["ssh.key.type"], "ED25519");
        assert_eq!(
            key.attributes["ssh.key.fingerprint"],
            "SHA256:Xk2p9cQv4m1eFzA8sT0uW3yR6nB5dJ7hL2gK9aV1oPc"
        );

        let password = parse(
            "Oct 16 01:41:50 worker-1 sshd-session[23014]: Accepted password for deploy from 2001:db8::7 port 50122 ssh2",
        );
        assert_eq!(event(&password), ("ssh_login", "success"));
        assert_eq!(password.attributes["ssh.method"], "password");
        assert_eq!(password.attributes["client.ip"], "2001:db8::7");
        assert!(!password.attributes.contains_key("ssh.key.type"));

        let pam = parse(
            "sshd[1]: Accepted keyboard-interactive/pam for arik from 198.51.100.4 port 2201 ssh2",
        );
        assert_eq!(pam.attributes["ssh.method"], "keyboard-interactive/pam");
    }

    #[test]
    fn reads_journal_lines_as_system_logs() {
        let line = parse_text_line(
            "sshd[812]: Accepted password for deploy from 203.0.113.7 port 1 ssh2",
            LogFormat::System,
        );
        assert_eq!(event(&line), ("ssh_login", "success"));
        assert_eq!(line.attributes["log.format"], "system");
        assert!(!line.attributes.contains_key("log.parse_failed"));

        let error = parse_text_line(
            "sshd[9]: error: maximum authentication attempts exceeded for root from 45.155.205.233 port 41022 ssh2 [preauth]",
            LogFormat::System,
        );
        assert_eq!(event(&error), ("ssh_login", "failure"));
        assert_eq!(error.severity, Some(17));
    }

    #[test]
    fn reads_ssh_failures() {
        let invalid = parse(
            "sshd[9]: Failed password for invalid user admin from 45.155.205.233 port 50122 ssh2",
        );
        assert_eq!(event(&invalid), ("ssh_login", "failure"));
        assert_eq!(invalid.severity, Some(13));
        assert_eq!(invalid.attributes["user"], "admin");
        assert_eq!(invalid.attributes["ssh.invalid_user"], true);

        let key = parse(
            "sshd[9]: Failed publickey for root from 45.155.205.233 port 50122 ssh2: RSA SHA256:abc",
        );
        assert_eq!(key.attributes["ssh.key.fingerprint"], "SHA256:abc");

        let unknown =
            parse("sshd[9]: Invalid user oracle from 45.155.205.233 port 41022");
        assert_eq!(event(&unknown), ("ssh_login", "failure"));
        assert_eq!(unknown.attributes["user"], "oracle");
        assert_eq!(unknown.attributes["ssh.invalid_user"], true);

        let closed = parse(
            "sshd[9]: Connection closed by authenticating user root 45.155.205.233 port 41022 [preauth]",
        );
        assert_eq!(event(&closed), ("ssh_login", "failure"));
        assert_eq!(closed.attributes["user"], "root");
        assert_eq!(closed.attributes["client.ip"], "45.155.205.233");

        let closed_invalid = parse(
            "sshd[9]: Connection closed by invalid user ubnt 45.155.205.233 port 41022 [preauth]",
        );
        assert_eq!(closed_invalid.attributes["user"], "ubnt");

        let maximum = parse(
            "sshd[9]: maximum authentication attempts exceeded for root from 45.155.205.233 port 41022 ssh2 [preauth]",
        );
        assert_eq!(event(&maximum), ("ssh_login", "failure"));

        let noise =
            parse("sshd[9]: Connection closed by 45.155.205.233 port 41022 [preauth]");
        assert!(!noise.attributes.contains_key("event.action"));
    }

    #[test]
    fn takes_the_address_sshd_wrote_not_the_one_in_the_user_name() {
        let forged = parse(
            "sshd[9]: Invalid user x from 10.0.0.1 port 22 from 45.155.205.233 port 41022",
        );
        assert_eq!(forged.attributes["client.ip"], "45.155.205.233");
        assert_eq!(forged.attributes["user"], "x from 10.0.0.1 port 22");

        let not_an_address = parse("sshd[9]: Invalid user x from nowhere port 41022");
        assert!(!not_an_address.attributes.contains_key("event.action"));
        assert!(!not_an_address.attributes.contains_key("client.ip"));
    }

    #[test]
    fn reads_ssh_only_from_sshd() {
        let app = parse(
            "Oct 16 01:41:50 worker-1 myapp[2]: Accepted password for root from 203.0.113.7 port 1 ssh2",
        );
        assert!(!app.attributes.contains_key("event.action"));

        let logout =
            parse("sshd[9]: pam_unix(sshd:session): session closed for user arik");
        assert_eq!(event(&logout), ("ssh_logout", "success"));
        assert_eq!(logout.attributes["user"], "arik");

        let opened = parse(
            "sshd[9]: pam_unix(sshd:session): session opened for user arik(uid=1000) by (uid=0)",
        );
        assert!(!opened.attributes.contains_key("event.action"));
    }

    #[test]
    fn reads_auth_log_lines_with_iso_times() {
        let line = parse(
            "2026-11-27T01:40:29.617602+06:00 daroyan-grants su[512727]: (to root) arik on pts/0",
        );
        assert_eq!(event(&line), ("su", "success"));
        assert_eq!(line.message, "(to root) arik on pts/0");
        assert_eq!(line.at, Some(1_795_722_029_617));
        assert_eq!(line.attributes["process.pid"], 512_727);

        let sudo = parse(
            "2026-11-27T01:40:27.754306+06:00 web sudo:   tester : USER=root ; COMMAND=/usr/bin/true",
        );
        assert_eq!(event(&sudo), ("sudo", "success"));

        let nested = parse("sshd[9]: kex[2]: Invalid user x from 1.2.3.4 port 5");
        assert_eq!(nested.attributes["process.name"], "sshd");
    }

    #[test]
    fn reads_sudo() {
        let command = parse(
            "sudo[512712]:   tester : PWD=/home/tester ; USER=root ; COMMAND=/usr/bin/true",
        );
        assert_eq!(event(&command), ("sudo", "success"));
        assert_eq!(command.attributes["user"], "tester");
        assert_eq!(command.attributes["sudo.target_user"], "root");
        assert_eq!(command.attributes["process.command_line"], "/usr/bin/true");

        let denied = parse(
            "sudo[3]:      eve : user NOT in sudoers ; TTY=pts/1 ; PWD=/home/eve ; USER=root ; COMMAND=/bin/bash",
        );
        assert_eq!(event(&denied), ("sudo", "failure"));
        assert_eq!(denied.severity, Some(13));
    }

    #[test]
    fn reads_su() {
        let switched = parse("su[512727]: (to root) arik on pts/0");
        assert_eq!(event(&switched), ("su", "success"));
        assert_eq!(switched.attributes["user"], "arik");
        assert_eq!(switched.attributes["su.target_user"], "root");
        assert_eq!(switched.attributes["su.tty"], "pts/0");

        let failed = parse("su[4]: FAILED SU (to root) eve on pts/1");
        assert_eq!(event(&failed), ("su", "failure"));
        assert_eq!(failed.attributes["user"], "eve");

        let no_tty = parse("su[4]: (to tester) root on none");
        assert!(!no_tty.attributes.contains_key("su.tty"));
    }

    #[test]
    fn reads_account_changes() {
        let user = parse(
            "useradd[7]: new user: name=tester, UID=1000, GID=1000, home=/home/tester, shell=/bin/bash, from=none",
        );
        assert_eq!(event(&user), ("user_add", "success"));
        assert_eq!(user.attributes["user.target"], "tester");

        let group = parse("useradd[7]: new group: name=tester, GID=1000");
        assert_eq!(event(&group), ("group_add", "success"));
        assert_eq!(group.attributes["group.name"], "tester");

        let sudoer = parse("usermod[8]: add 'tester' to group 'sudo'");
        assert_eq!(event(&sudoer), ("user_change", "success"));
        assert_eq!(sudoer.attributes["user.target"], "tester");
        assert_eq!(sudoer.attributes["group.name"], "sudo");

        let shadow = parse("usermod[8]: add 'tester' to shadow group 'sudo'");
        assert!(!shadow.attributes.contains_key("event.action"));

        let gpasswd = parse("gpasswd[9]: user tester added by root to group docker");
        assert_eq!(event(&gpasswd), ("user_change", "success"));
        assert_eq!(gpasswd.attributes["user"], "root");
        assert_eq!(gpasswd.attributes["group.name"], "docker");

        let shell =
            parse("usermod[8]: change user 'tester' shell from '/bin/sh' to '/bin/bash'");
        assert_eq!(event(&shell), ("user_change", "success"));

        let deleted = parse("userdel[10]: delete user 'tester'");
        assert_eq!(event(&deleted), ("user_delete", "success"));

        let group_deleted = parse("groupdel[11]: group 'devs' removed");
        assert_eq!(event(&group_deleted), ("group_delete", "success"));
        assert_eq!(group_deleted.attributes["group.name"], "devs");

        let password = parse(
            "chpasswd[12]: pam_unix(chpasswd:chauthtok): password changed for tester",
        );
        assert_eq!(event(&password), ("password_change", "success"));
        assert_eq!(password.attributes["user.target"], "tester");

        let elsewhere = parse("myapp[1]: new user: name=tester, UID=1000");
        assert!(!elsewhere.attributes.contains_key("event.action"));
    }
}
