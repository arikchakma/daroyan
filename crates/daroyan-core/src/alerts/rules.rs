use chrono::{DateTime, Datelike, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{Error, ErrorName, Result};

pub const MAX_FOR_SECS: u32 = 86_400;

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    Offline,
    Cpu {
        above: f64,
    },
    Memory {
        above: f64,
    },
    Disk {
        above: f64,
    },
    Load {
        above: f64,
    },
    ProcessStopped {
        name: Option<String>,
    },
    PortClosed {
        name: Option<String>,
    },
    Logs {
        level: String,
        count: u32,
        within_secs: u32,
        service: Option<String>,
        text: Option<String>,
    },
    UptimeDown,
    ResponseTime {
        above: f64,
    },
    FirstByte {
        above: f64,
    },
    CertDays {
        below: f64,
    },
    CronMissed,
    CronFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Server,
    Check,
    Cron,
}

impl Target {
    pub fn label(self) -> &'static str {
        match self {
            Self::Server => "Server",
            Self::Check => "Check",
            Self::Cron => "Cron",
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::Server => "servers",
            Self::Check => "uptime",
            Self::Cron => "crons",
        }
    }

    pub fn url(self, app_url: &str, id: &str) -> String {
        format!("{}/{}/{id}", app_url.trim_end_matches('/'), self.path())
    }
}

impl Condition {
    pub fn target(&self) -> Target {
        match self {
            Self::UptimeDown
            | Self::ResponseTime { .. }
            | Self::FirstByte { .. }
            | Self::CertDays { .. } => Target::Check,
            Self::CronMissed | Self::CronFailed => Target::Cron,
            _ => Target::Server,
        }
    }

    pub fn critical(&self) -> Option<f64> {
        match self {
            Self::Cpu { above }
            | Self::Memory { above }
            | Self::Disk { above }
            | Self::Load { above }
            | Self::ResponseTime { above }
            | Self::FirstByte { above } => Some(*above),
            Self::CertDays { below } => Some(*below),
            Self::Logs { count, .. } => Some(f64::from(*count)),
            _ => None,
        }
    }

    pub fn is_below(&self) -> bool {
        matches!(self, Self::CertDays { .. })
    }

    pub fn has_aggregation(&self) -> bool {
        matches!(
            self,
            Self::Cpu { .. }
                | Self::Memory { .. }
                | Self::Disk { .. }
                | Self::Load { .. }
                | Self::ResponseTime { .. }
                | Self::FirstByte { .. }
        )
    }
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    #[default]
    Avg,
    Max,
    Min,
    P95,
}

impl Aggregation {
    pub fn to_sql(self, column: &str) -> String {
        match self {
            Self::Avg => format!("avg({column})"),
            Self::Max => format!("max({column})"),
            Self::Min => format!("min({column})"),
            Self::P95 => format!("quantile_cont({column}, 0.95)"),
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Self::Avg => "Average",
            Self::Max => "Max",
            Self::Min => "Min",
            Self::P95 => "p95",
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoData {
    #[default]
    Keep,
    Alert,
    Resolve,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RuleOptions {
    pub aggregation: Aggregation,
    pub window_secs: u32,
    pub warning: Option<f64>,
    pub resolve_at: Option<f64>,
    pub resolve_for_secs: u32,
    pub no_data: NoData,
    pub remind_every_secs: Option<u32>,
    pub remind_times: u32,
    pub runbook_url: Option<String>,
    pub severity_label: Option<String>,
    pub warning_integration_ids: Vec<String>,
    pub message_limit: Option<u32>,
    pub quiet_hours: Vec<QuietWindow>,
    pub time_zone: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
pub struct QuietWindow {
    pub days: Vec<u8>,
    pub from: String,
    pub to: String,
}

pub const MAX_REMINDERS: u32 = 10;
pub const MAX_MESSAGE_LIMIT: u32 = 100;
const MAX_QUIET_WINDOWS: usize = 5;
const MAX_RUNBOOK_URL: usize = 2000;
const DAY_MINUTES: u32 = 1440;
const WEEK_MINUTES: u32 = 7 * DAY_MINUTES;

pub fn read_time_zone(name: Option<&str>) -> Tz {
    name.and_then(|name| name.parse().ok()).unwrap_or(Tz::UTC)
}

pub fn format_clock(at: DateTime<Utc>, time_zone: Tz) -> String {
    let clock = at.with_timezone(&time_zone).format("%H:%M").to_string();
    if time_zone == Tz::UTC { format!("{clock} UTC") } else { clock }
}

pub fn format_moment(at: DateTime<Utc>, now: DateTime<Utc>, time_zone: Tz) -> String {
    let local = at.with_timezone(&time_zone);
    let today = now.with_timezone(&time_zone).date_naive();

    let day = match (today - local.date_naive()).num_days() {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ => local.format("%b %-d").to_string(),
    };

    let zone = if time_zone == Tz::UTC { " UTC" } else { "" };
    format!("{day} at {}{zone}", local.format("%-I:%M %p"))
}

fn read_clock(text: &str) -> Option<u32> {
    let (hours, minutes) = text.split_once(':')?;
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

impl RuleOptions {
    pub fn is_quiet(&self, now: DateTime<Utc>) -> bool {
        if self.quiet_hours.is_empty() {
            return false;
        }
        let local = now.with_timezone(&read_time_zone(self.time_zone.as_deref()));
        let minute = local.weekday().num_days_from_monday() * DAY_MINUTES
            + local.hour() * 60
            + local.minute();
        self.quiet_hours.iter().any(|window| {
            let (Some(from), Some(to)) =
                (read_clock(&window.from), read_clock(&window.to))
            else {
                return false;
            };
            let length = match (to + DAY_MINUTES - from) % DAY_MINUTES {
                0 => DAY_MINUTES,
                length => length,
            };
            window.days.iter().any(|day| {
                let start = u32::from(*day) * DAY_MINUTES + from;
                (minute + WEEK_MINUTES - start) % WEEK_MINUTES < length
            })
        })
    }
}

pub fn read_log_severity(level: &str) -> Option<u8> {
    match level {
        "debug" => Some(5),
        "info" => Some(9),
        "warn" => Some(13),
        "error" => Some(17),
        _ => None,
    }
}

pub fn describe_log_level(level: &str) -> &str {
    match level {
        "warn" => "warning",
        other => other,
    }
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    All,
    Label { label: String },
    Servers { server_ids: Vec<String> },
    Checks { check_ids: Vec<String> },
    Crons { cron_ids: Vec<String> },
}

impl Scope {
    pub fn includes(&self, server_id: &str, labels: &[String]) -> bool {
        match self {
            Self::All => true,
            Self::Label { label } => labels.iter().any(|item| item == label),
            Self::Servers { server_ids } => server_ids.iter().any(|id| id == server_id),
            Self::Checks { .. } | Self::Crons { .. } => false,
        }
    }

    pub fn includes_check(&self, check_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Checks { check_ids } => check_ids.iter().any(|id| id == check_id),
            _ => false,
        }
    }

    pub fn without(&self, target_id: &str) -> Option<Self> {
        let keep = |ids: &[String]| -> Option<Vec<String>> {
            let left: Vec<String> =
                ids.iter().filter(|id| *id != target_id).cloned().collect();
            (!left.is_empty()).then_some(left)
        };
        match self {
            Self::Servers { server_ids } => {
                keep(server_ids).map(|server_ids| Self::Servers { server_ids })
            }
            Self::Checks { check_ids } => {
                keep(check_ids).map(|check_ids| Self::Checks { check_ids })
            }
            Self::Crons { cron_ids } => {
                keep(cron_ids).map(|cron_ids| Self::Crons { cron_ids })
            }
            other => Some(other.clone()),
        }
    }

    pub fn includes_cron(&self, cron_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Crons { cron_ids } => cron_ids.iter().any(|id| id == cron_id),
            _ => false,
        }
    }
}

pub fn validate_rule(condition: &Condition, scope: &Scope, for_secs: u32) -> Result<()> {
    validate_options(condition, &RuleOptions::default(), scope, for_secs)
}

pub fn validate_options(
    condition: &Condition,
    options: &RuleOptions,
    scope: &Scope,
    for_secs: u32,
) -> Result<()> {
    let invalid = |message: &str| Err(Error::new(ErrorName::ValidationError, message));
    match condition {
        Condition::Cpu { above }
        | Condition::Memory { above }
        | Condition::Disk { above }
            if !(0.0..100.0).contains(above) =>
        {
            return invalid("The threshold must be between 0 and 100 percent");
        }
        Condition::Load { above } if !above.is_finite() || *above <= 0.0 => {
            return invalid("The load threshold must be above 0");
        }
        Condition::ResponseTime { above } | Condition::FirstByte { above }
            if !(1.0..=60_000.0).contains(above) =>
        {
            return invalid("The time must be between 1 ms and 60 seconds");
        }
        Condition::CertDays { below } if !(1.0..=365.0).contains(below) => {
            return invalid("The days must be between 1 and 365");
        }
        Condition::ProcessStopped { name: Some(name) }
        | Condition::PortClosed { name: Some(name) }
            if name.trim().is_empty() =>
        {
            return invalid("Leave the name out to match any, or give a name");
        }
        Condition::Logs { level, .. } if read_log_severity(level).is_none() => {
            return invalid("The level must be debug, info, warn or error");
        }
        Condition::Logs { count, .. } if !(1..=100_000).contains(count) => {
            return invalid("The count must be between 1 and 100,000");
        }
        Condition::Logs { within_secs, .. }
            if !(60..=MAX_FOR_SECS).contains(within_secs) =>
        {
            return invalid("The window must be between 1 minute and 24 hours");
        }
        Condition::Logs { service: Some(text), .. }
        | Condition::Logs { text: Some(text), .. }
            if text.trim().is_empty() || text.len() > 200 =>
        {
            return invalid(
                "Leave the source and text out to match any, or give up to 200 characters",
            );
        }
        _ => {}
    }
    match (condition.target(), scope) {
        (_, Scope::Label { label }) if label.trim().is_empty() => {
            return invalid("Pick a label");
        }
        (_, Scope::Servers { server_ids }) if server_ids.is_empty() => {
            return invalid("Pick at least one server");
        }
        (_, Scope::Checks { check_ids }) if check_ids.is_empty() => {
            return invalid("Pick at least one check");
        }
        (_, Scope::Crons { cron_ids }) if cron_ids.is_empty() => {
            return invalid("Pick at least one cron");
        }
        (
            Target::Cron,
            Scope::Label { .. } | Scope::Servers { .. } | Scope::Checks { .. },
        ) => {
            return invalid("Cron rules cover all crons or picked crons");
        }
        (Target::Check, Scope::Crons { .. }) => {
            return invalid("Uptime rules cover all checks or picked checks");
        }
        (Target::Check, Scope::Label { .. } | Scope::Servers { .. }) => {
            return invalid("Uptime rules cover all checks or picked checks");
        }
        (Target::Server, Scope::Checks { .. } | Scope::Crons { .. }) => {
            return invalid("Server rules cover servers, not checks");
        }
        _ => {}
    }
    if for_secs > MAX_FOR_SECS {
        return invalid("The duration can be at most 24 hours");
    }
    if options.window_secs > MAX_FOR_SECS || options.resolve_for_secs > MAX_FOR_SECS {
        return invalid("Windows can be at most 24 hours");
    }
    let critical = condition.critical();
    let is_past = |value: f64, limit: f64| {
        if condition.is_below() { value < limit } else { value > limit }
    };
    if let Some(warning) = options.warning {
        let Some(critical) = critical else {
            return invalid("This condition has no warning level");
        };
        if !warning.is_finite() || is_past(warning, critical) || warning == critical {
            return invalid("The warning limit must come before the critical limit");
        }
    }
    if let Some(resolve_at) = options.resolve_at {
        let Some(limit) = options.warning.or(critical) else {
            return invalid("This condition has no recovery limit");
        };
        if !resolve_at.is_finite() || is_past(resolve_at, limit) {
            return invalid("The recovery limit must be on the safe side of the limits");
        }
    }
    if let Some(every) = options.remind_every_secs
        && (!(300..=MAX_FOR_SECS).contains(&every)
            || !(1..=MAX_REMINDERS).contains(&options.remind_times))
    {
        return invalid("Remind every 5 minutes to 24 hours, up to 10 times");
    }
    if options
        .message_limit
        .is_some_and(|limit| !(1..=MAX_MESSAGE_LIMIT).contains(&limit))
    {
        return invalid("The message limit must be between 1 and 100 an hour");
    }
    if options.quiet_hours.len() > MAX_QUIET_WINDOWS {
        return invalid("Add up to 5 quiet hour windows");
    }
    for window in &options.quiet_hours {
        if window.days.is_empty() || window.days.iter().any(|day| *day > 6) {
            return invalid("Pick at least one day for quiet hours");
        }
        if read_clock(&window.from).is_none() || read_clock(&window.to).is_none() {
            return invalid("Quiet hours need times like 02:00");
        }
    }
    if options
        .time_zone
        .as_deref()
        .is_some_and(|name| name.parse::<Tz>().is_err())
    {
        return invalid("The time zone is unknown");
    }
    if let Some(url) = &options.runbook_url
        && parse_runbook_url(url).is_none()
    {
        return invalid(
            "The runbook URL must be an http:// or https:// link under 2000 characters",
        );
    }
    if options
        .severity_label
        .as_ref()
        .is_some_and(|label| label.trim().is_empty() || label.len() > 20)
    {
        return invalid("The severity label can be up to 20 characters");
    }
    Ok(())
}

pub fn parse_runbook_url(url: &str) -> Option<reqwest::Url> {
    if url.len() > MAX_RUNBOOK_URL {
        return None;
    }

    let parsed = reqwest::Url::parse(url).ok()?;
    let is_web = matches!(parsed.scheme(), "http" | "https") && parsed.host().is_some();

    (is_web && parsed.as_str().len() <= MAX_RUNBOOK_URL).then_some(parsed)
}

pub fn describe_condition(condition: &Condition, for_secs: u32) -> String {
    let what = match condition {
        Condition::Offline => "Server offline".to_string(),
        Condition::Cpu { above } => format!("CPU above {above}%"),
        Condition::Memory { above } => format!("Memory above {above}%"),
        Condition::Disk { above } => format!("Disk above {above}%"),
        Condition::Load { above } => format!("Load above {above}"),
        Condition::ProcessStopped { name } => {
            format!("{} stopped", name.as_deref().unwrap_or("A process"))
        }
        Condition::PortClosed { name } => {
            format!("{} closed", name.as_deref().unwrap_or("A port"))
        }
        Condition::UptimeDown => "Uptime check down".to_string(),
        Condition::ResponseTime { above } => {
            format!("Response time above {}", format_ms(*above))
        }
        Condition::FirstByte { above } => {
            format!("First byte time above {}", format_ms(*above))
        }
        Condition::CertDays { below } => {
            format!("Certificate expires in under {below} days")
        }
        Condition::CronMissed => "Cron missed a run".to_string(),
        Condition::CronFailed => "Cron failed".to_string(),
        Condition::Logs { level, count, within_secs, service, text } => {
            let mut what = format!(
                "{count}+ {} lines in {}",
                describe_log_level(level),
                format_duration(*within_secs)
            );
            if let Some(service) = service {
                what.push_str(&format!(" from {service}"));
            }
            if let Some(text) = text {
                what.push_str(&format!(" matching \"{text}\""));
            }
            what
        }
    };
    if for_secs == 0 { what } else { format!("{what} for {}", format_duration(for_secs)) }
}

pub fn describe_rule(
    condition: &Condition,
    options: &RuleOptions,
    for_secs: u32,
) -> String {
    let mut what = describe_condition(condition, 0);
    if condition.has_aggregation() && options.window_secs > 0 {
        what = format!(
            "{} {} over {}",
            options.aggregation.describe(),
            lowercase_first(&what),
            format_duration(options.window_secs)
        );
    }
    if let Some(warning) = options.warning {
        let warning = match condition {
            Condition::ResponseTime { .. } | Condition::FirstByte { .. } => {
                format_ms(warning)
            }
            Condition::Cpu { .. } | Condition::Memory { .. } | Condition::Disk { .. } => {
                format!("{warning}%")
            }
            _ => warning.to_string(),
        };
        what.push_str(&format!(", warning at {warning}"));
    }
    if for_secs == 0 { what } else { format!("{what} for {}", format_duration(for_secs)) }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    if chars.clone().nth(1).is_some_and(char::is_uppercase) {
        return text.to_string();
    }
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

pub fn format_ms(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{ms:.0} ms");
    }
    let secs = format!("{:.1}", ms / 1000.0);
    format!("{} s", secs.trim_end_matches(".0"))
}

pub fn format_duration(secs: u32) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ if secs.is_multiple_of(3600) => format!("{}h", secs / 3600),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn reads_and_checks_log_conditions() {
        let condition: Condition = serde_json::from_str(
            r#"{"kind":"logs","level":"error","count":10,"within_secs":300,"service":"Linkity API","text":null}"#,
        )
        .unwrap();
        assert_eq!(
            describe_condition(&condition, 0),
            "10+ error lines in 5m from Linkity API"
        );
        assert!(validate_rule(&condition, &Scope::All, 0).is_ok());
        let bad = Condition::Logs {
            level: "loud".into(),
            count: 1,
            within_secs: 300,
            service: None,
            text: None,
        };
        assert!(validate_rule(&bad, &Scope::All, 0).is_err());
        let short = Condition::Logs {
            level: "warn".into(),
            count: 1,
            within_secs: 10,
            service: None,
            text: None,
        };
        assert!(validate_rule(&short, &Scope::All, 0).is_err());
    }
    use super::*;

    #[test]
    fn reads_conditions_and_scopes_as_tagged_json() {
        let condition: Condition =
            serde_json::from_str(r#"{"kind":"process_stopped","name":"Nginx"}"#).unwrap();
        assert_eq!(
            condition,
            Condition::ProcessStopped { name: Some("Nginx".to_string()) }
        );
        let scope: Scope =
            serde_json::from_str(r#"{"kind":"servers","server_ids":["srv_1"]}"#).unwrap();
        assert!(scope.includes("srv_1", &[]));
        assert!(!scope.includes("srv_2", &[]));
        let offline: Condition = serde_json::from_str(r#"{"kind":"offline"}"#).unwrap();
        assert_eq!(offline, Condition::Offline);
    }

    #[test]
    fn takes_a_deleted_target_out_of_a_scope() {
        let crons = Scope::Crons { cron_ids: vec!["crn_1".into(), "crn_2".into()] };
        assert_eq!(
            crons.without("crn_1"),
            Some(Scope::Crons { cron_ids: vec!["crn_2".into()] })
        );
        assert_eq!(crons.without("crn_3"), Some(crons.clone()));
        let one = Scope::Checks { check_ids: vec!["chk_1".into()] };
        assert_eq!(one.without("chk_1"), None);
        assert_eq!(Scope::All.without("srv_1"), Some(Scope::All));
        let label = Scope::Label { label: "web".into() };
        assert_eq!(label.without("srv_1"), Some(label.clone()));
    }

    #[test]
    fn rejects_bad_rules() {
        let all = Scope::All;
        assert!(validate_rule(&Condition::Cpu { above: 90.0 }, &all, 300).is_ok());
        assert!(validate_rule(&Condition::Cpu { above: 120.0 }, &all, 0).is_err());
        assert!(validate_rule(&Condition::Load { above: 0.0 }, &all, 0).is_err());
        assert!(validate_rule(&Condition::Offline, &all, 90_000).is_err());
        let empty = Scope::Servers { server_ids: Vec::new() };
        assert!(validate_rule(&Condition::Offline, &empty, 0).is_err());
    }

    #[test]
    fn finds_quiet_hours_across_midnight() {
        let options = RuleOptions {
            quiet_hours: vec![QuietWindow {
                days: vec![0],
                from: "23:00".into(),
                to: "01:00".into(),
            }],
            time_zone: Some("Asia/Dhaka".into()),
            ..RuleOptions::default()
        };
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        assert!(options.is_quiet(at("2026-10-19T17:30:00Z")));
        assert!(options.is_quiet(at("2026-10-19T18:30:00Z")));
        assert!(!options.is_quiet(at("2026-10-19T19:30:00Z")));
        assert!(!options.is_quiet(at("2026-10-20T17:30:00Z")));
        assert!(validate_options(&Condition::Offline, &options, &Scope::All, 0).is_ok());
    }

    #[test]
    fn describes_conditions() {
        assert_eq!(
            describe_condition(&Condition::Cpu { above: 90.0 }, 300),
            "CPU above 90% for 5m"
        );
        assert_eq!(describe_condition(&Condition::Offline, 0), "Server offline");
        assert_eq!(
            describe_condition(&Condition::ProcessStopped { name: None }, 60),
            "A process stopped for 1m"
        );
        assert_eq!(format_duration(5400), "1h 30m");
        let windowed = RuleOptions {
            window_secs: 60,
            aggregation: Aggregation::Max,
            ..RuleOptions::default()
        };
        assert_eq!(
            describe_rule(&Condition::Cpu { above: 90.0 }, &windowed, 0),
            "Max CPU above 90% over 1m"
        );
        assert_eq!(
            describe_rule(&Condition::Memory { above: 90.0 }, &windowed, 0),
            "Max memory above 90% over 1m"
        );
    }
}
