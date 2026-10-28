use serde_json::Value;

use super::{ParsedLine, SEVERITY_ERROR, SEVERITY_WARN, read_level_word};

pub(super) fn read_ssh_failure(text: &str, parsed: &mut ParsedLine) {
    let Some(after) = text.split_once("Failed password for ").map(|(_, after)| after)
    else {
        return;
    };
    let after = after.strip_prefix("invalid user ").unwrap_or(after);
    if let Some(user) = after.split_whitespace().next() {
        parsed.attributes.insert("user".into(), Value::from(user));
    }
    if let Some(ip) = after
        .split_once(" from ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
    {
        parsed.attributes.insert("client.ip".into(), Value::from(ip));
    }
    parsed.severity = Some(parsed.severity.unwrap_or(SEVERITY_WARN).max(SEVERITY_WARN));
}

pub(super) fn read_oom_kill(text: &str, parsed: &mut ParsedLine) {
    let Some((_, after)) = text.split_once("Killed process ") else {
        return;
    };
    let mut parts = after.split_whitespace();
    if let Some(pid) = parts.next().and_then(|pid| pid.parse::<u64>().ok()) {
        parsed.attributes.insert("process.pid".into(), Value::from(pid));
    }
    if let Some(name) = parts
        .next()
        .and_then(|name| name.strip_prefix('(')?.strip_suffix(')'))
    {
        parsed.attributes.insert("process.name".into(), Value::from(name));
    }
    parsed.severity = Some(SEVERITY_ERROR);
}

fn process_name(parsed: &ParsedLine) -> &str {
    parsed
        .attributes
        .get("process.name")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn raise_to_warn(parsed: &mut ParsedLine) {
    parsed.severity = Some(parsed.severity.unwrap_or(SEVERITY_WARN).max(SEVERITY_WARN));
}

pub(super) fn read_sudo(text: &str, parsed: &mut ParsedLine) {
    if process_name(parsed) != "sudo" {
        return;
    }
    let Some((user, fields)) = text.split_once(" : ") else {
        return;
    };
    parsed.attributes.insert("user".into(), Value::from(user.trim()));
    for field in fields.split(" ; ") {
        match field.trim().split_once('=') {
            Some(("USER", target)) => {
                parsed
                    .attributes
                    .insert("sudo.target_user".into(), Value::from(target));
            }
            Some(("TTY", tty)) => {
                parsed.attributes.insert("sudo.tty".into(), Value::from(tty));
            }
            Some(("COMMAND", command)) => {
                parsed
                    .attributes
                    .insert("process.command_line".into(), Value::from(command));
            }
            _ => {}
        }
    }
    if fields.contains("incorrect password") || fields.contains("NOT in sudoers") {
        raise_to_warn(parsed);
    }
}

pub(super) fn read_cron(text: &str, parsed: &mut ParsedLine) {
    if !matches!(process_name(parsed), "CRON" | "cron" | "crond") {
        return;
    }
    let Some((user, rest)) =
        text.strip_prefix('(').and_then(|after| after.split_once(") "))
    else {
        return;
    };
    parsed.attributes.insert("user".into(), Value::from(user));
    if let Some(command) =
        rest.strip_prefix("CMD (").and_then(|after| after.strip_suffix(')'))
    {
        parsed
            .attributes
            .insert("process.command_line".into(), Value::from(command));
    }
}

pub(super) fn read_ufw(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (action, fields) = text.strip_prefix("[UFW ")?.split_once("] ")?;
    let mut values = std::collections::HashMap::new();
    for field in fields.split_whitespace() {
        if let Some((key, value)) = field.split_once('=')
            && !value.is_empty()
        {
            values.insert(key, value);
        }
    }
    parsed
        .attributes
        .insert("firewall.action".into(), Value::from(action));
    let mappings = [
        ("SRC", "source.ip"),
        ("DST", "destination.ip"),
        ("PROTO", "network.transport"),
        ("SPT", "source.port"),
        ("DPT", "destination.port"),
        ("IN", "network.interface"),
    ];
    for (key, target) in mappings {
        let Some(value) = values.get(key) else { continue };
        let value = match value.parse::<u64>() {
            Ok(number) if key.ends_with("PT") => Value::from(number),
            _ => Value::from(*value),
        };
        parsed.attributes.insert(target.into(), value);
    }
    let source = values.get("SRC").copied().unwrap_or("?");
    let destination = values.get("DST").copied().unwrap_or("?");
    let protocol = values.get("PROTO").copied().unwrap_or_default();
    Some(match values.get("DPT") {
        Some(port) => format!("UFW {action} {protocol} {source} → {destination}:{port}"),
        None => format!("UFW {action} {protocol} {source} → {destination}"),
    })
}

pub(super) fn read_fail2ban(text: &str, parsed: &mut ParsedLine) -> Option<String> {
    let (logger, rest) = text.split_once(' ')?;
    if !logger.starts_with("fail2ban.") {
        return None;
    }
    let (pid, rest) = rest.strip_prefix('[')?.split_once("]: ")?;
    let (level, rest) = rest.trim_start().split_once(char::is_whitespace)?;
    parsed.severity = Some(read_level_word(level)?);
    parsed.attributes.insert("logger".into(), Value::from(logger));
    if let Ok(pid) = pid.parse::<u64>() {
        parsed.attributes.insert("process.pid".into(), Value::from(pid));
    }
    let rest = rest.trim_start();
    let Some((jail, action)) =
        rest.strip_prefix('[').and_then(|after| after.split_once("] "))
    else {
        return Some(rest.to_string());
    };
    parsed.attributes.insert("fail2ban.jail".into(), Value::from(jail));
    let mut words = action.split_whitespace();
    if let (Some(verb), Some(ip)) = (words.next(), words.next())
        && matches!(verb, "Ban" | "Unban" | "Found" | "Restore")
    {
        parsed.attributes.insert("fail2ban.action".into(), Value::from(verb));
        parsed.attributes.insert("client.ip".into(), Value::from(ip));
        if verb == "Ban" {
            raise_to_warn(parsed);
        }
    }
    Some(rest.to_string())
}

#[cfg(test)]
mod tests {
    use crate::analytics::log_parse::tests::parse;

    #[test]
    fn reads_sudo_and_cron() {
        let sudo = parse(
            "Oct 28 01:50:02 web sudo:   arik : TTY=pts/0 ; PWD=/home/arik ; USER=root ; COMMAND=/usr/bin/apt upgrade",
        );
        assert_eq!(sudo.attributes["user"], "arik");
        assert_eq!(sudo.attributes["sudo.target_user"], "root");
        assert_eq!(sudo.attributes["process.command_line"], "/usr/bin/apt upgrade");
        assert_eq!(sudo.severity, None);
        let denied = parse(
            "Oct 28 01:50:02 web sudo:   eve : 3 incorrect password attempts ; TTY=pts/1 ; PWD=/home/eve ; USER=root ; COMMAND=/bin/bash",
        );
        assert_eq!(denied.severity, Some(13));

        let cron = parse(
            "Oct 28 01:50:02 web CRON[1234]: (root) CMD (/usr/local/bin/backup.sh)",
        );
        assert_eq!(cron.attributes["user"], "root");
        assert_eq!(cron.attributes["process.command_line"], "/usr/local/bin/backup.sh");
    }

    #[test]
    fn reads_ufw_and_fail2ban() {
        let ufw = parse(
            "Oct 28 01:50:02 web kernel: [12345.678] [UFW BLOCK] IN=eth0 OUT= MAC=aa:bb SRC=45.1.2.3 DST=10.0.0.2 LEN=60 PROTO=TCP SPT=51234 DPT=22 WINDOW=64240",
        );
        assert_eq!(ufw.message, "UFW BLOCK TCP 45.1.2.3 → 10.0.0.2:22");
        assert_eq!(ufw.attributes["firewall.action"], "BLOCK");
        assert_eq!(ufw.attributes["source.ip"], "45.1.2.3");
        assert_eq!(ufw.attributes["destination.port"], 22);
        assert!(!ufw.attributes.contains_key("thread"));

        let ban = parse(
            "2026-10-28 01:50:02,123 fail2ban.actions [1234]: NOTICE  [sshd] Ban 45.1.2.3",
        );
        assert_eq!(ban.severity, Some(13));
        assert_eq!(ban.message, "[sshd] Ban 45.1.2.3");
        assert_eq!(ban.attributes["fail2ban.jail"], "sshd");
        assert_eq!(ban.attributes["fail2ban.action"], "Ban");
        assert_eq!(ban.attributes["client.ip"], "45.1.2.3");
        let found = parse(
            "2026-10-28 01:50:02,123 fail2ban.filter [1234]: INFO    [sshd] Found 45.1.2.3 - 2026-10-28 01:50:01",
        );
        assert_eq!(found.severity, Some(9));
    }
}
