use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use daroyan_protocol::report::TopProcess;
use reqwest::Url;

use crate::alerts::actions::{DECLARE, MUTE_DAY, MUTE_HOUR, RESTART};
use crate::alerts::evaluate::Level;
use crate::alerts::facts::{
    AlertFacts, CertFacts, CheckFacts, DiskFacts, LogFacts, OfflineFacts, ProcessFacts,
    UsageFacts,
};
use crate::alerts::notify::{Details, Field, Link, LinkStyle, Snippet, Text, Tone};
use crate::alerts::rules::{
    Condition, RuleOptions, Target, describe_log_level, describe_rule, format_moment,
    format_ms, read_time_zone,
};
use crate::analytics::log_lines::read_level;
use crate::db::entities::alert_rules::Model as AlertRule;
use crate::db::entities::alerts::Model as Alert;
use crate::servers::status::get_percent;

const MAX_SNIPPET_LINE: usize = 200;
const TOP_PROCESSES: usize = 3;
const BYTE_UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Firing,
    Escalated,
    Reminder { lasted_secs: u32 },
    MuteEnded { lasted_secs: u32 },
    Resolved { lasted_secs: u32 },
}

impl Moment {
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::Resolved { .. })
    }

    pub fn tone(self, severity: &str) -> Tone {
        match (self, Level::read(severity)) {
            (Self::Resolved { .. }, _) => Tone::Resolved,
            (Self::Escalated, _) | (_, Level::Critical) => Tone::Critical,
            _ => Tone::Warning,
        }
    }

    fn note(self) -> Text {
        match self {
            Self::Escalated => Text::from("It was a warning before."),
            Self::Reminder { lasted_secs } => Text::from("Still firing after ")
                .bold(format_span(u64::from(lasted_secs)))
                .plain("."),
            Self::MuteEnded { lasted_secs } => {
                Text::from("The mute ended. It is still firing after ")
                    .bold(format_span(u64::from(lasted_secs)))
                    .plain(".")
            }
            Self::Firing | Self::Resolved { .. } => Text::default(),
        }
    }
}

pub struct AlertMessage<'a> {
    pub rule: &'a AlertRule,
    pub condition: &'a Condition,
    pub options: &'a RuleOptions,
    pub alert: &'a Alert,
    pub facts: &'a AlertFacts,
    pub target: Target,
    pub target_id: &'a str,
    pub target_name: &'a str,
    pub labels: &'a [String],
    pub app_url: Option<&'a str>,
    pub interactive: bool,
    pub now: DateTime<Utc>,
}

impl AlertMessage<'_> {
    pub fn details(&self, moment: Moment) -> Details {
        let mut details = match (moment, self.facts) {
            (Moment::Resolved { lasted_secs }, facts) => {
                self.resolved(lasted_secs, facts)
            }
            (_, AlertFacts::Logs(facts)) => self.logs(facts),
            (_, AlertFacts::Usage(facts)) => self.usage(facts),
            (_, AlertFacts::Process(facts)) => self.process(facts),
            (_, AlertFacts::Check(facts)) => self.check(facts),
            (_, AlertFacts::Disk(facts)) => self.disk(facts),
            (_, AlertFacts::Offline(facts)) => self.offline(facts),
            (_, AlertFacts::Cert(facts)) => self.cert(facts),
            (_, AlertFacts::None) => self.generic(),
        };
        if details.note.is_empty() {
            details.note = moment.note();
        }
        if !moment.is_resolved() {
            details.links.extend(self.firing_links());
            details.links.extend(self.action_links());
        }
        details
    }

    fn generic(&self) -> Details {
        let alert = self.alert;
        Details {
            summary: Text::from(format!("{}: ", alert.subject))
                .bold(alert.value.as_str())
                .plain("."),
            fields: vec![
                self.target_field(),
                self.started_field(),
                Field::new("Rule", &self.rule.name),
                Field::new(
                    "Fires when",
                    describe_rule(self.condition, self.options, self.for_secs()),
                ),
            ],
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn logs(&self, facts: &LogFacts) -> Details {
        let Condition::Logs { level, within_secs, service, .. } = self.condition else {
            return self.generic();
        };
        let found = read_count(&self.alert.value);
        let lines = format!("{found} {} lines", describe_log_level(level));
        let mut fields = vec![self.target_field()];
        if let Some(service) = service {
            fields.push(Field::new("Source", service));
        }
        fields.push(self.started_field());
        fields.push(Field::new("Rule", &self.rule.name));
        let snippet = (!facts.top.is_empty()).then(|| Snippet {
            label: Some(
                if level == "error" { "Top errors" } else { "Top messages" }.to_string(),
            ),
            lines: facts
                .top
                .iter()
                .map(|(message, count)| format!("{count}× {}", to_one_line(message)))
                .collect(),
        });
        Details {
            summary: Text::default().bold(lines).plain(format!(
                " in the last {}. {}",
                format_span(u64::from(*within_secs)),
                self.describe_trigger()
            )),
            fields,
            snippet,
            links: self.logs_link().into_iter().map(Link::primary).collect(),
            ..Details::default()
        }
    }

    fn usage(&self, facts: &UsageFacts) -> Details {
        let alert = self.alert;
        let held = match self.for_secs() {
            0 => String::new(),
            secs => format!(" for {}", format_span(u64::from(secs))),
        };
        let memory = get_percent(facts.mem_used, facts.mem_total);
        let mut top: Vec<&TopProcess> = facts.top.iter().collect();
        if matches!(self.condition, Condition::Memory { .. }) {
            top.sort_by_key(|item| std::cmp::Reverse(item.mem_used));
        } else {
            top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        }
        let width = top
            .iter()
            .take(TOP_PROCESSES)
            .map(|item| item.name.chars().count())
            .max()
            .unwrap_or(0);
        let snippet = (!top.is_empty()).then(|| Snippet {
            label: Some("Top processes".to_string()),
            lines: top
                .iter()
                .take(TOP_PROCESSES)
                .map(|item| {
                    format!(
                        "{:<width$}  {:>5} cores  {:>7}",
                        item.name,
                        format!("{:.1}", item.cpu / 100.0),
                        format_bytes(item.mem_used)
                    )
                })
                .collect(),
        });
        Details {
            summary: Text::from(format!("{} at ", alert.subject))
                .bold(alert.value.as_str())
                .plain(format!("{held}. {}", self.describe_trigger())),
            fields: vec![
                self.target_field(),
                Field::new(
                    "Load",
                    format!("{:.1} on {} cores", facts.load1, facts.cores),
                ),
                Field::new(
                    "Memory",
                    format!(
                        "{memory:.0}% · {}",
                        format_used_of(facts.mem_used, facts.mem_total)
                    ),
                ),
                self.started_field(),
            ],
            snippet,
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn process(&self, facts: &ProcessFacts) -> Details {
        let name = self.alert.subject.as_str();
        let exit = facts.exit.clone().unwrap_or_default();
        let summary = match (&exit.signal, exit.code) {
            _ if exit.oom => Text::default()
                .bold(name)
                .plain(" ran out of memory, so the kernel killed it."),
            _ if exit.stopped => Text::default().bold(name).plain(
                " was stopped on purpose, with docker stop or docker kill. It has not started again.",
            ),
            (Some(signal), _) => Text::default()
                .bold(name)
                .plain(" was killed by ")
                .bold(signal.as_str())
                .plain(format!(". {}", explain_signal(signal))),
            (None, Some(0)) => Text::default()
                .bold(name)
                .plain(" exited with code ")
                .bold("0")
                .plain(". It finished without an error, but it is no longer running."),
            (None, Some(code)) => Text::default()
                .bold(name)
                .plain(" exited with code ")
                .bold(code.to_string())
                .plain(". Its last log lines usually show why."),
            (None, None) => Text::default().bold(name).plain(" stopped."),
        };
        let mut fields = vec![self.target_field()];
        let reason = match (exit.oom, &exit.signal, exit.code) {
            (true, _, _) => Some("Out of memory".to_string()),
            _ if exit.stopped => Some("Stopped on purpose".to_string()),
            (false, Some(signal), Some(code)) => Some(format!("{signal} · code {code}")),
            (false, Some(signal), None) => Some(signal.clone()),
            (false, None, Some(code)) => Some(format!("Code {code}")),
            (false, None, None) => None,
        };
        if let Some(reason) = reason {
            fields.push(Field::new("Exit", reason));
        }
        if let Some(secs) = exit.ran_secs {
            fields.push(Field::new("Ran for", format_span(secs)));
        }
        if let Some(restarts) = exit.restarts.filter(|count| *count > 0) {
            fields.push(Field::new("Restarts", restarts.to_string()));
        }
        fields.push(self.started_field());
        let time_zone = self.time_zone();
        let snippet = (!facts.last_lines.is_empty()).then(|| Snippet {
            label: Some("Last log lines".to_string()),
            lines: facts
                .last_lines
                .iter()
                .map(|line| {
                    format!(
                        "{} {:<5} {}",
                        line.at.with_timezone(&time_zone).format("%H:%M:%S"),
                        read_level(line.severity).to_uppercase(),
                        to_one_line(&line.message)
                    )
                })
                .collect(),
        });
        let mut links = self.open_target();
        if self.interactive && facts.controllable {
            let restart = Link::new("Restart", self.alert_url().unwrap_or_default())
                .primary()
                .action(RESTART, &self.alert.id);
            links = std::iter::once(restart)
                .chain(
                    links
                        .into_iter()
                        .map(|link| Link { style: LinkStyle::Default, ..link }),
                )
                .collect();
        }
        if snippet.is_some() {
            links.extend(self.logs_link());
        }
        Details {
            summary,
            fields,
            snippet,
            links,
            ..Details::default()
        }
    }

    fn check(&self, facts: &CheckFacts) -> Details {
        let summary = if facts.failures >= 2 {
            Text::default()
                .bold(self.target_name)
                .plain(format!(" failed {} checks in a row.", facts.failures))
        } else {
            Text::default().bold(self.target_name).plain(" is down.")
        };
        let error = facts
            .latest
            .as_ref()
            .and_then(|latest| latest.error.clone())
            .unwrap_or_else(|| self.alert.value.trim_start_matches("Down: ").to_string());
        let mut fields = vec![Field::new("URL", &facts.url), Field::new("Error", error)];
        if let Some(up) = &facts.last_up {
            let mut value = format_moment(up.at, self.now, self.time_zone());
            if let Some(code) = up.code {
                value.push_str(&format!(" · {code}"));
            }
            if let Some(ms) = up.response_ms {
                value.push_str(&format!(" in {}", format_ms(f64::from(ms))));
            }
            fields.push(Field::new("Last good check", value));
        }
        fields.push(self.started_field());
        let snippet = facts.latest.as_ref().map(|latest| {
            let is_https = facts.url.starts_with("https://");
            let steps = [
                Some(("DNS", latest.dns_ms)),
                Some(("Connect", latest.connect_ms)),
                is_https.then_some(("TLS", latest.tls_ms)),
                Some(("First byte", latest.ttfb_ms)),
            ];
            let failed = if latest
                .error
                .as_deref()
                .is_some_and(|error| error.to_lowercase().contains("timed out"))
            {
                "timed out"
            } else {
                "failed"
            };
            let mut parts = Vec::new();
            for (label, ms) in steps.into_iter().flatten() {
                match ms {
                    Some(ms) => parts.push(format!("{label} {ms} ms")),
                    None => {
                        parts.push(format!("{label} {failed}"));
                        break;
                    }
                }
            }
            Snippet { label: None, lines: vec![parts.join(" · ")] }
        });
        Details {
            summary,
            fields,
            snippet,
            links: self.open_target(),
            context: Some(format!(
                "Checked every {}",
                format_span(u64::from(facts.interval_secs))
            )),
            ..Details::default()
        }
    }

    fn disk(&self, facts: &DiskFacts) -> Details {
        let used = get_percent(facts.used, facts.total);
        let free = facts.total.saturating_sub(facts.used);
        let mut fields = vec![self.target_field()];
        if let Some(trend) = facts.trend {
            let window =
                format_span(u64::try_from(trend.window_ms / 1000).unwrap_or_default());
            if trend.bytes_per_hour >= 1.0 {
                fields.push(Field::new(
                    "Growth",
                    format!(
                        "+{}/hour over the last {window}",
                        format_bytes(trend.bytes_per_hour as u64)
                    ),
                ));
                let hours = free as f64 / trend.bytes_per_hour;
                fields.push(Field::new(
                    "Full in",
                    format!(
                        "About {} at this rate",
                        format_span((hours * 3600.0) as u64)
                    ),
                ));
            } else {
                fields.push(Field::new(
                    "Growth",
                    format!("Not growing over the last {window}"),
                ));
            }
        }
        fields.push(self.started_field());
        Details {
            summary: Text::from("The disk at ")
                .bold(facts.mount.as_str())
                .plain(" is ")
                .bold(format!("{used:.0}%"))
                .plain(format!(
                    " full: {}. {} {} free.",
                    format_used_of(facts.used, facts.total),
                    format_bytes(free),
                    if free == 1 { "is" } else { "are" }
                )),
            fields,
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn offline(&self, facts: &OfflineFacts) -> Details {
        let summary = match facts.last_seen_at {
            Some(at) => Text::from("The agent has not reported for ")
                .bold(format_span((self.now - at).num_seconds().max(0).unsigned_abs()))
                .plain("."),
            None => Text::from("The agent has never reported."),
        };
        let mut fields = vec![self.target_field()];
        if let Some(at) = facts.last_seen_at {
            fields.push(Field::new(
                "Last report",
                format_moment(at, self.now, self.time_zone()),
            ));
        }
        if let Some(version) = &facts.agent_version {
            fields.push(Field::new("Agent", version));
        }
        let mut note = Text::default();
        if let Some(checks) = facts.checks.filter(|checks| checks.total > 0) {
            fields.push(Field::new(
                "Uptime checks",
                format!("{} of {} still pass", checks.passing, checks.total),
            ));
            if checks.passing == checks.total {
                note = Text::from("The uptime checks still pass, so the server is up. The agent probably stopped. Run ")
                    .bold("systemctl status daroyan")
                    .plain(" on the server.");
            }
        }
        Details {
            summary,
            fields,
            note,
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn cert(&self, facts: &CertFacts) -> Details {
        let Some(expires_at) = facts.expires_at else { return self.generic() };
        let time_zone = self.time_zone();
        let days = (expires_at - self.now).num_days();
        let left = match days {
            ..=0 => "less than a day".to_string(),
            1 => "1 day".to_string(),
            _ => format!("{days} days"),
        };
        let local = expires_at.with_timezone(&time_zone);
        Details {
            summary: Text::from("The certificate for ")
                .bold(facts.domain.as_str())
                .plain(" expires in ")
                .bold(left)
                .plain(format!(", on {}.", local.format("%b %-d"))),
            fields: vec![
                self.target_field(),
                Field::new(
                    "Expires",
                    format!(
                        "{} at {}{}",
                        local.format("%b %-d"),
                        local.format("%-I:%M %p"),
                        zone_suffix(time_zone)
                    ),
                ),
            ],
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn resolved(&self, lasted_secs: u32, facts: &AlertFacts) -> Details {
        let lasted = format_span(u64::from(lasted_secs));
        if let (
            AlertFacts::Logs(LogFacts { run: Some(run), .. }),
            Condition::Logs { level, within_secs, .. },
        ) = (facts, self.condition)
        {
            let level = describe_log_level(level);
            let mut summary = Text::from("Back to normal after ").bold(lasted).plain(".");
            if let Some(at) = run.last_at.and_then(DateTime::from_timestamp_millis) {
                summary = summary.plain(format!(
                    " The last {level} line was at {}.",
                    format_time(at, self.time_zone())
                ));
            }
            return Details {
                summary,
                fields: vec![
                    Field::new(
                        "Peak",
                        format!(
                            "{} {level} lines in {}",
                            run.peak,
                            format_span(u64::from(*within_secs))
                        ),
                    ),
                    Field::new("Total", format!("{} {level} lines", run.total)),
                ],
                links: self.logs_link().into_iter().collect(),
                ..Details::default()
            };
        }
        Details {
            summary: Text::from(format!(
                "{} is back to normal after ",
                self.alert.subject
            ))
            .bold(lasted)
            .plain("."),
            fields: vec![
                self.target_field(),
                self.started_field(),
                Field::new("Rule", &self.rule.name),
            ],
            links: self.open_target(),
            ..Details::default()
        }
    }

    fn describe_trigger(&self) -> String {
        let held = match self.for_secs() {
            0 => String::new(),
            secs => format!(" for {}", format_span(u64::from(secs))),
        };
        match self.condition {
            Condition::Logs { count, within_secs, .. } => format!(
                "The rule fires at {count} lines in {}.",
                format_span(u64::from(*within_secs))
            ),
            Condition::Cpu { above }
            | Condition::Memory { above }
            | Condition::Disk { above } => {
                format!("The rule fires above {above}%{held}.")
            }
            Condition::Load { above } => format!("The rule fires above {above}{held}."),
            _ => format!(
                "The rule fires on: {}.",
                describe_rule(self.condition, self.options, self.for_secs())
            ),
        }
    }

    fn for_secs(&self) -> u32 {
        u32::try_from(self.rule.for_secs).unwrap_or_default()
    }

    fn time_zone(&self) -> Tz {
        read_time_zone(self.options.time_zone.as_deref())
    }

    fn started_field(&self) -> Field {
        Field::new(
            "Started",
            format_moment(self.alert.started_at, self.now, self.time_zone()),
        )
    }

    fn target_field(&self) -> Field {
        let kind = match self.target {
            Target::Server => "Server",
            Target::Check => "Check",
            Target::Cron => "Cron",
        };
        let label = std::iter::once(self.target_name)
            .chain(self.labels.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" · ");
        Field::new(kind, label)
    }

    fn app_url(&self) -> Option<&str> {
        self.app_url.map(|url| url.trim_end_matches('/'))
    }

    fn open_target(&self) -> Vec<Link> {
        let Some(app_url) = self.app_url() else { return Vec::new() };
        let (label, path) = match self.target {
            Target::Server => ("Open server", "servers"),
            Target::Check => ("Open check", "uptime"),
            Target::Cron => ("Open cron", "crons"),
        };
        vec![Link::new(label, format!("{app_url}/{path}/{}", self.target_id)).primary()]
    }

    fn logs_link(&self) -> Option<Link> {
        let app_url = self.app_url()?;
        let mut params =
            vec![("server_id", self.target_id.to_string()), ("range", "1h".to_string())];
        match self.condition {
            Condition::Logs { level, service, text, .. } => {
                let view = match level.as_str() {
                    "error" | "warn" | "info" => level.as_str(),
                    _ => "all",
                };
                params.push(("level", view.to_string()));
                params.extend(service.iter().map(|service| ("service", service.clone())));
                params.extend(text.iter().map(|text| ("q", text.clone())));
            }
            Condition::ProcessStopped { .. } => {
                params.push(("service", self.alert.subject.clone()));
            }
            _ => {}
        }
        let url = Url::parse_with_params(&format!("{app_url}/logs"), params).ok()?;
        Some(Link::new("Open logs", url.as_str()))
    }

    fn alert_url(&self) -> Option<String> {
        self.app_url()
            .map(|app_url| format!("{app_url}/alerts/{}", self.rule.id))
    }

    fn action_links(&self) -> Vec<Link> {
        let Some(url) = self.alert_url().filter(|_| self.interactive) else {
            return Vec::new();
        };
        let id = self.alert.id.as_str();
        let mut links = Vec::new();
        match self.condition {
            Condition::Offline | Condition::ProcessStopped { .. } => {}
            Condition::CertDays { .. } => {
                links.push(Link::new("Mute 1 day", url.as_str()).action(MUTE_DAY, id));
            }
            _ => links.push(Link::new("Mute 1h", url.as_str()).action(MUTE_HOUR, id)),
        }
        if Level::read(&self.alert.severity) == Level::Critical
            && matches!(self.condition, Condition::Logs { .. } | Condition::UptimeDown)
        {
            links.push(Link::new("Declare incident", url).danger().action(DECLARE, id));
        }
        links
    }

    fn firing_links(&self) -> Vec<Link> {
        let Some(app_url) = self.app_url() else { return Vec::new() };
        let view = Link::new("View alert", format!("{app_url}/alerts/{}", self.rule.id));
        let runbook = self
            .options
            .runbook_url
            .as_deref()
            .map(|url| Link::new("Runbook", url));
        std::iter::once(view).chain(runbook).collect()
    }
}

fn read_count(value: &str) -> u64 {
    value
        .split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or_default()
}

fn to_one_line(message: &str) -> String {
    let line = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_SNIPPET_LINE {
        return line;
    }
    let mut cut: String = line.chars().take(MAX_SNIPPET_LINE - 1).collect();
    cut.push('…');
    cut
}

fn explain_signal(signal: &str) -> &'static str {
    match signal {
        "SIGTERM" | "SIGINT" | "SIGHUP" => "Something asked it to stop.",
        "SIGKILL" => "Something forced it to stop.",
        "SIGSEGV" | "SIGBUS" | "SIGILL" | "SIGFPE" => "It crashed.",
        "SIGABRT" => "It aborted itself, usually after a failed check.",
        _ => "Its last log lines usually show why.",
    }
}

pub fn format_span(secs: u64) -> String {
    let plural = |count: u64, unit: &str| {
        if count == 1 { format!("1 {unit}") } else { format!("{count} {unit}s") }
    };
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min", secs / 60),
        3600..86_400 => match secs % 3600 / 60 {
            0 => plural(secs / 3600, "hour"),
            minutes => format!("{} {minutes} min", plural(secs / 3600, "hour")),
        },
        _ => match secs % 86_400 / 3600 {
            0 => plural(secs / 86_400, "day"),
            hours => {
                format!("{} {}", plural(secs / 86_400, "day"), plural(hours, "hour"))
            }
        },
    }
}

fn format_time(at: DateTime<Utc>, time_zone: Tz) -> String {
    format!(
        "{}{}",
        at.with_timezone(&time_zone).format("%-I:%M %p"),
        zone_suffix(time_zone)
    )
}

fn zone_suffix(time_zone: Tz) -> &'static str {
    if time_zone == Tz::UTC { " UTC" } else { "" }
}

fn split_bytes(bytes: u64) -> (f64, usize) {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < BYTE_UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    (value, unit)
}

fn round_for_display(value: f64) -> String {
    if value >= 10.0 || value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

pub fn format_bytes(bytes: u64) -> String {
    let (value, unit) = split_bytes(bytes);
    format!("{} {}", round_for_display(value), BYTE_UNITS[unit])
}

pub fn format_used_of(used: u64, total: u64) -> String {
    let (_, unit) = split_bytes(total);
    let divisor = 1024_f64.powi(i32::try_from(unit).unwrap_or_default());
    format!(
        "{} of {} {}",
        round_for_display(used as f64 / divisor),
        round_for_display(total as f64 / divisor),
        BYTE_UNITS[unit]
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::alerts::facts::{CheckCount, CheckResult, LogLine};
    use crate::analytics::log_lines::LogRun;
    use crate::analytics::server_events::ProcessExit;
    use crate::analytics::server_states::DiskTrend;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, hour, minute, 0).unwrap()
    }

    fn make_rule(for_secs: i64) -> AlertRule {
        AlertRule {
            id: "rule_1".to_string(),
            name: "InboxJot error spike".to_string(),
            enabled: true,
            condition: json!({}),
            for_secs,
            scope: json!({}),
            integration_ids: json!([]),
            options: json!({}),
            last_fired_at: None,
            quiet_until: None,
            declare_incident: false,
            declare_severity: None,
            declare_hidden_page_ids: json!([]),
            created_at: at(0, 0),
            updated_at: at(0, 0),
        }
    }

    fn make_alert(subject: &str, value: &str) -> Alert {
        Alert {
            id: "alert_1".to_string(),
            rule_id: "rule_1".to_string(),
            server_id: Some("server_1".to_string()),
            check_id: None,
            cron_id: None,
            subject: subject.to_string(),
            value: value.to_string(),
            severity: "critical".to_string(),
            status: "firing".to_string(),
            started_at: at(8, 54),
            resolved_at: None,
            last_notified_at: None,
            reminders_sent: 0,
        }
    }

    struct Case {
        rule: AlertRule,
        condition: Condition,
        options: RuleOptions,
        alert: Alert,
        facts: AlertFacts,
        target: Target,
        labels: Vec<String>,
        interactive: bool,
    }

    impl Case {
        fn new(condition: Condition, alert: Alert, facts: AlertFacts) -> Self {
            Self {
                rule: make_rule(0),
                condition,
                options: RuleOptions::default(),
                alert,
                facts,
                target: Target::Server,
                labels: vec!["production".to_string()],
                interactive: false,
            }
        }

        fn details(&self, moment: Moment) -> Details {
            AlertMessage {
                rule: &self.rule,
                condition: &self.condition,
                options: &self.options,
                alert: &self.alert,
                facts: &self.facts,
                target: self.target,
                target_id: "server_1",
                target_name: "Inboxjot Server",
                labels: &self.labels,
                app_url: Some("https://daroyan.test/"),
                interactive: self.interactive,
                now: at(9, 0),
            }
            .details(moment)
        }
    }

    fn labels(links: &[Link]) -> Vec<&str> {
        links.iter().map(|link| link.label.as_str()).collect()
    }

    fn logs_condition() -> Condition {
        Condition::Logs {
            level: "error".to_string(),
            count: 10,
            within_secs: 300,
            service: Some("Inboxjot API".to_string()),
            text: None,
        }
    }

    #[test]
    fn builds_an_error_spike() {
        let case = Case::new(
            logs_condition(),
            make_alert("Inboxjot API", "11 error lines in 5m"),
            AlertFacts::Logs(LogFacts {
                top: vec![("HTTP export failed\n  status: 404".to_string(), 11)],
                run: None,
            }),
        );
        let details = case.details(Moment::Firing);
        assert_eq!(
            details.summary.to_string(),
            "11 error lines in the last 5 min. The rule fires at 10 lines in 5 min."
        );
        assert_eq!(
            details.fields,
            vec![
                Field::new("Server", "Inboxjot Server · production"),
                Field::new("Source", "Inboxjot API"),
                Field::new("Started", "Today at 8:54 AM UTC"),
                Field::new("Rule", "InboxJot error spike"),
            ]
        );
        assert_eq!(
            details.snippet,
            Some(Snippet {
                label: Some("Top errors".to_string()),
                lines: vec!["11× HTTP export failed status: 404".to_string()],
            })
        );
        assert_eq!(labels(&details.links), ["Open logs", "View alert"]);
        assert_eq!(
            details.links[0].url,
            "https://daroyan.test/logs?server_id=server_1&range=1h&level=error&service=Inboxjot+API"
        );
    }

    #[test]
    fn adds_action_buttons_when_slack_buttons_are_set_up() {
        let mut spike = Case::new(
            logs_condition(),
            make_alert("Inboxjot API", "11 error lines in 5m"),
            AlertFacts::Logs(LogFacts::default()),
        );
        assert_eq!(
            labels(&spike.details(Moment::Firing).links),
            ["Open logs", "View alert"]
        );
        spike.interactive = true;
        let links = spike.details(Moment::Firing).links;
        assert_eq!(
            labels(&links),
            ["Open logs", "View alert", "Mute 1h", "Declare incident"]
        );
        assert_eq!(links[2].action.as_ref().unwrap().id, MUTE_HOUR);
        assert_eq!(links[2].action.as_ref().unwrap().value, "alert_1");
        assert_eq!(links[3].style, LinkStyle::Danger);
        assert!(
            spike
                .details(Moment::Resolved { lasted_secs: 60 })
                .links
                .iter()
                .all(|link| link.action.is_none())
        );

        let mut crashed = Case::new(
            Condition::ProcessStopped { name: None },
            make_alert("worker", "Stopped"),
            AlertFacts::Process(ProcessFacts {
                controllable: true,
                ..ProcessFacts::default()
            }),
        );
        crashed.interactive = true;
        let links = crashed.details(Moment::Firing).links;
        assert_eq!(labels(&links), ["Restart", "Open server", "View alert"]);
        assert_eq!(links[0].style, LinkStyle::Primary);
        assert_eq!(links[1].style, LinkStyle::Default);

        let mut cert = Case::new(
            Condition::CertDays { below: 14.0 },
            make_alert("Certificate", "12 days left"),
            AlertFacts::None,
        );
        cert.interactive = true;
        cert.alert.severity = "warning".to_string();
        assert_eq!(
            labels(&cert.details(Moment::Firing).links),
            ["Open server", "View alert", "Mute 1 day"]
        );
    }

    #[test]
    fn builds_a_resolved_error_spike() {
        let case = Case::new(
            logs_condition(),
            make_alert("Inboxjot API", "11 error lines in 5m"),
            AlertFacts::Logs(LogFacts {
                top: Vec::new(),
                run: Some(LogRun {
                    total: 612,
                    peak: 40,
                    last_at: Some(at(9, 41).timestamp_millis()),
                }),
            }),
        );
        let details = case.details(Moment::Resolved { lasted_secs: 3060 });
        assert_eq!(
            details.summary.to_string(),
            "Back to normal after 51 min. The last error line was at 9:41 AM UTC."
        );
        assert_eq!(
            details.fields,
            vec![
                Field::new("Peak", "40 error lines in 5 min"),
                Field::new("Total", "612 error lines"),
            ]
        );
        assert_eq!(labels(&details.links), ["Open logs"]);
    }

    #[test]
    fn builds_high_cpu() {
        let mut case = Case::new(
            Condition::Cpu { above: 90.0 },
            make_alert("CPU", "96%"),
            AlertFacts::Usage(UsageFacts {
                load1: 4.2,
                cores: 3,
                mem_used: 1_181_116_006,
                mem_total: 3_972_844_748,
                top: vec![
                    TopProcess {
                        name: "postgres".to_string(),
                        count: 9,
                        cpu: 18.0,
                        mem_used: 1_288_490_189,
                    },
                    TopProcess {
                        name: "inboxjot-worker".to_string(),
                        count: 3,
                        cpu: 213.0,
                        mem_used: 851_443_712,
                    },
                    TopProcess {
                        name: "server.js".to_string(),
                        count: 1,
                        cpu: 5.0,
                        mem_used: 171_966_464,
                    },
                    TopProcess {
                        name: "sshd".to_string(),
                        count: 2,
                        cpu: 0.0,
                        mem_used: 9_000_000,
                    },
                ],
            }),
        );
        case.rule = make_rule(600);
        case.options.runbook_url = Some("https://wiki.test/cpu".to_string());
        let details = case.details(Moment::Firing);
        assert_eq!(
            details.summary.to_string(),
            "CPU at 96% for 10 min. The rule fires above 90% for 10 min."
        );
        assert_eq!(details.fields[1], Field::new("Load", "4.2 on 3 cores"));
        assert_eq!(details.fields[2], Field::new("Memory", "30% · 1.1 of 3.7 GB"));
        let snippet = details.snippet.clone().unwrap();
        assert_eq!(snippet.label.as_deref(), Some("Top processes"));
        assert_eq!(
            snippet.lines,
            [
                "inboxjot-worker    2.1 cores   812 MB",
                "postgres           0.2 cores   1.2 GB",
                "server.js          0.1 cores   164 MB",
            ]
        );
        assert_eq!(labels(&details.links), ["Open server", "View alert", "Runbook"]);
    }

    fn crash(exit: ProcessExit) -> Details {
        Case::new(
            Condition::ProcessStopped { name: None },
            make_alert("inboxjot-api-blue", "Stopped"),
            AlertFacts::Process(ProcessFacts {
                exit: Some(exit),
                last_lines: vec![LogLine {
                    at: at(8, 53),
                    severity: 17,
                    message: "failed to allocate 64 MB".to_string(),
                }],
                controllable: true,
            }),
        )
        .details(Moment::Firing)
    }

    #[test]
    fn builds_a_crashed_process() {
        let details = crash(ProcessExit {
            code: Some(137),
            signal: Some("SIGKILL".to_string()),
            oom: true,
            stopped: false,
            ran_secs: Some(187_200),
            restarts: Some(2),
        });
        assert_eq!(
            details.summary.to_string(),
            "inboxjot-api-blue ran out of memory, so the kernel killed it."
        );
        assert_eq!(
            details.fields[1..4],
            [
                Field::new("Exit", "Out of memory"),
                Field::new("Ran for", "2 days 4 hours"),
                Field::new("Restarts", "2"),
            ]
        );
        assert_eq!(
            details.snippet.unwrap().lines,
            ["08:53:00 ERROR failed to allocate 64 MB"]
        );
        assert_eq!(labels(&details.links), ["Open server", "Open logs", "View alert"]);
    }

    #[test]
    fn does_not_call_every_sigkill_out_of_memory() {
        let killed = crash(ProcessExit {
            code: Some(137),
            signal: Some("SIGKILL".to_string()),
            ..ProcessExit::default()
        });
        assert_eq!(
            killed.summary.to_string(),
            "inboxjot-api-blue was killed by SIGKILL. Something forced it to stop."
        );
        assert_eq!(killed.fields[1], Field::new("Exit", "SIGKILL · code 137"));
        let failed = crash(ProcessExit { code: Some(1), ..ProcessExit::default() });
        assert_eq!(
            failed.summary.to_string(),
            "inboxjot-api-blue exited with code 1. Its last log lines usually show why."
        );
        assert_eq!(failed.fields[1], Field::new("Exit", "Code 1"));
        let stopped = crash(ProcessExit {
            code: Some(143),
            signal: Some("SIGTERM".to_string()),
            stopped: true,
            ..ProcessExit::default()
        });
        assert_eq!(
            stopped.summary.to_string(),
            "inboxjot-api-blue was stopped on purpose, with docker stop or docker kill. It has not started again."
        );
        assert_eq!(stopped.fields[1], Field::new("Exit", "Stopped on purpose"));
        let unknown = crash(ProcessExit::default());
        assert_eq!(unknown.summary.to_string(), "inboxjot-api-blue stopped.");
        assert_eq!(unknown.fields[1].label, "Started");
    }

    #[test]
    fn builds_a_down_check() {
        let mut case = Case::new(
            Condition::UptimeDown,
            make_alert("Status", "Down: Timed out after 10 s"),
            AlertFacts::Check(CheckFacts {
                url: "https://api.inboxjot.com/v1/health".to_string(),
                interval_secs: 60,
                failures: 2,
                latest: Some(CheckResult {
                    at: at(8, 55),
                    code: None,
                    response_ms: None,
                    error: Some("Timed out after 10 s".to_string()),
                    dns_ms: Some(12),
                    connect_ms: Some(31),
                    tls_ms: Some(48),
                    ttfb_ms: None,
                }),
                last_up: Some(CheckResult {
                    at: at(8, 52),
                    code: Some(200),
                    response_ms: Some(118),
                    error: None,
                    dns_ms: None,
                    connect_ms: None,
                    tls_ms: None,
                    ttfb_ms: None,
                }),
            }),
        );
        case.target = Target::Check;
        case.labels = Vec::new();
        let details = case.details(Moment::Firing);
        assert_eq!(
            details.summary.to_string(),
            "Inboxjot Server failed 2 checks in a row."
        );
        assert_eq!(
            details.fields[..3],
            [
                Field::new("URL", "https://api.inboxjot.com/v1/health"),
                Field::new("Error", "Timed out after 10 s"),
                Field::new("Last good check", "Today at 8:52 AM UTC · 200 in 118 ms"),
            ]
        );
        assert_eq!(
            details.snippet.unwrap().lines,
            ["DNS 12 ms · Connect 31 ms · TLS 48 ms · First byte timed out"]
        );
        assert_eq!(details.context.as_deref(), Some("Checked every 1 min"));
        assert_eq!(labels(&details.links), ["Open check", "View alert"]);
    }

    #[test]
    fn names_the_step_that_failed() {
        let latest = CheckResult {
            at: at(8, 55),
            code: None,
            response_ms: None,
            error: Some("Connection refused".to_string()),
            dns_ms: Some(0),
            connect_ms: None,
            tls_ms: None,
            ttfb_ms: None,
        };
        let mut case = Case::new(
            Condition::UptimeDown,
            make_alert("Status", "Down: Connection refused"),
            AlertFacts::Check(CheckFacts {
                url: "http://127.0.0.1:4999/health".to_string(),
                interval_secs: 60,
                failures: 1,
                latest: Some(latest),
                last_up: None,
            }),
        );
        case.target = Target::Check;
        let details = case.details(Moment::Firing);
        assert_eq!(details.summary.to_string(), "Inboxjot Server is down.");
        assert_eq!(details.snippet.unwrap().lines, ["DNS 0 ms · Connect failed"]);
        assert_eq!(details.fields[1], Field::new("Error", "Connection refused"));
    }

    #[test]
    fn builds_a_full_disk() {
        const GB: u64 = 1024 * 1024 * 1024;
        let mut alert = make_alert("/", "87%");
        alert.severity = "warning".to_string();
        let case = Case::new(
            Condition::Disk { above: 85.0 },
            alert,
            AlertFacts::Disk(DiskFacts {
                mount: "/".to_string(),
                used: 65 * GB,
                total: 75 * GB,
                trend: Some(DiskTrend {
                    bytes_per_hour: GB as f64,
                    window_ms: 6 * 3_600_000,
                }),
            }),
        );
        let details = case.details(Moment::Firing);
        assert_eq!(
            details.summary.to_string(),
            "The disk at / is 87% full: 65 of 75 GB. 10 GB are free."
        );
        assert_eq!(
            details.fields[1],
            Field::new("Growth", "+1 GB/hour over the last 6 hours")
        );
        assert_eq!(
            details.fields[2],
            Field::new("Full in", "About 10 hours at this rate")
        );
    }

    #[test]
    fn builds_an_offline_server() {
        let case = Case::new(
            Condition::Offline,
            make_alert("Server", "Offline"),
            AlertFacts::Offline(OfflineFacts {
                last_seen_at: Some(at(8, 58)),
                agent_version: Some("0.0.3".to_string()),
                checks: Some(CheckCount { passing: 4, total: 4 }),
            }),
        );
        let details = case.details(Moment::Firing);
        assert_eq!(details.summary.to_string(), "The agent has not reported for 2 min.");
        assert_eq!(details.fields[3], Field::new("Uptime checks", "4 of 4 still pass"));
        assert!(details.note.to_string().contains("systemctl status daroyan"));
    }

    #[test]
    fn builds_an_expiring_certificate() {
        let mut case = Case::new(
            Condition::CertDays { below: 14.0 },
            make_alert("Certificate", "12 days left"),
            AlertFacts::Cert(CertFacts {
                domain: "app.inboxjot.com".to_string(),
                expires_at: Some(Utc.with_ymd_and_hms(2026, 11, 9, 23, 59, 0).unwrap()),
            }),
        );
        case.target = Target::Check;
        let details = case.details(Moment::Firing);
        assert_eq!(
            details.summary.to_string(),
            "The certificate for app.inboxjot.com expires in 12 days, on Nov 9."
        );
        assert_eq!(details.fields[1], Field::new("Expires", "Nov 9 at 11:59 PM UTC"));
    }

    #[test]
    fn falls_back_to_the_generic_layout() {
        let case = Case::new(
            Condition::PortClosed { name: None },
            make_alert("db:5432", "Closed"),
            AlertFacts::None,
        );
        let details = case.details(Moment::Reminder { lasted_secs: 1800 });
        assert_eq!(details.summary.to_string(), "db:5432: Closed.");
        assert_eq!(details.note.to_string(), "Still firing after 30 min.");
        assert_eq!(details.fields.len(), 4);
        let resolved = case.details(Moment::Resolved { lasted_secs: 59 });
        assert_eq!(resolved.summary.to_string(), "db:5432 is back to normal after 59 s.");
        assert_eq!(labels(&resolved.links), ["Open server"]);
        assert_eq!(Moment::Resolved { lasted_secs: 0 }.tone("critical"), Tone::Resolved);
        assert_eq!(Moment::Escalated.tone("warning"), Tone::Critical);
        assert_eq!(Moment::Firing.tone("warning"), Tone::Warning);
    }

    #[test]
    fn formats_spans_and_bytes() {
        assert_eq!(format_span(59), "59 s");
        assert_eq!(format_span(3060), "51 min");
        assert_eq!(format_span(3600), "1 hour");
        assert_eq!(format_span(7500), "2 hours 5 min");
        assert_eq!(format_span(187_200), "2 days 4 hours");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(
            format_used_of(65 * 1024_u64.pow(3), 75 * 1024_u64.pow(3)),
            "65 of 75 GB"
        );
    }
}
