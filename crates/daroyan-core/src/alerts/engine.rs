use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use daroyan_protocol::host::Host;
use serde_json::json;
use tokio::task::JoinHandle;

use crate::agents::Agents;
use crate::alerts::evaluate::{
    Level, Reading, ServerSnapshot, find_readings, format_percent, is_past, make_reading,
    rate,
};
use crate::alerts::integrations::IntegrationConfig;
use crate::alerts::notify::{Notice, send_notice};
use crate::alerts::rules::{
    Condition, NoData, RuleOptions, Scope, Target, describe_rule, format_clock,
    format_duration, format_ms, read_log_severity, read_time_zone,
};
use crate::analytics::Analytics;
use crate::analytics::log_lines::{LogMatch, count_logs_by_server};
use crate::analytics::server_states::aggregate_server_metric;
use crate::analytics::uptime_results::{aggregate_check_metric, list_cert_expiries};
use crate::db::Database;
use crate::db::entities::alert_mutes::Model as AlertMute;
use crate::db::entities::alert_rules::Model as AlertRule;
use crate::db::entities::alerts::Model as Alert;
use crate::db::entities::crons::Model as CronModel;
use crate::db::entities::integrations::Model as Integration;
use crate::db::entities::servers::Model as Server;
use crate::db::entities::uptime_checks::Model as UptimeCheck;
use crate::db::ids::{AlertId, AlertMuteId, AlertRuleId, IntegrationId};
use crate::db::queries::alert_deliveries::NewAlertDelivery;
use crate::db::queries::alerts::NewAlert;
use crate::error::Result;
use crate::id::{Prefix, new_id};
use crate::incidents::{RuleFire, declare_from_rule, list_maintained_targets};
use crate::servers::report::LastReport;

const TICK: Duration = Duration::from_secs(15);
const DAY_MS: f64 = 86_400_000.0;
const LIMIT_WINDOW_SECS: i64 = 3600;

type AlertKey = (String, String, String);

pub struct ParsedRule {
    pub condition: Condition,
    pub scope: Scope,
    pub integration_ids: Vec<String>,
    pub options: RuleOptions,
}

pub fn parse_rule(rule: &AlertRule) -> Option<ParsedRule> {
    Some(ParsedRule {
        condition: serde_json::from_value(rule.condition.clone()).ok()?,
        scope: serde_json::from_value(rule.scope.clone()).ok()?,
        integration_ids: serde_json::from_value(rule.integration_ids.clone()).ok()?,
        options: serde_json::from_value(rule.options.clone()).unwrap_or_else(|error| {
            tracing::warn!(rule_id = %rule.id, %error, "unreadable alert rule options, using defaults");
            RuleOptions::default()
        }),
    })
}

impl ParsedRule {
    fn recipients(&self, level: Level) -> &[String] {
        if level == Level::Warning {
            &self.options.warning_integration_ids
        } else {
            &self.integration_ids
        }
    }
}

struct ServerFacts<'a> {
    server: &'a Server,
    labels: Vec<String>,
    mem_total: u64,
    report: Option<LastReport>,
}

struct TargetReadings {
    id: String,
    name: String,
    target: Target,
    readings: Option<Vec<Reading>>,
}

pub struct AlertEngine {
    database: Database,
    agents: Agents,
    analytics: Analytics,
    app_url: Option<String>,
    pending: HashMap<AlertKey, DateTime<Utc>>,
    clearing: HashMap<AlertKey, DateTime<Utc>>,
    mutes: Vec<AlertMute>,
    sent: HashMap<String, Vec<DateTime<Utc>>>,
    limited: HashMap<String, DateTime<Utc>>,
    maintained: HashSet<String>,
}

enum Slot {
    Send,
    Drop,
    Summary(DateTime<Utc>),
}

pub fn spawn_alert_engine(
    database: Database,
    agents: Agents,
    analytics: Analytics,
    app_url: Option<String>,
) -> JoinHandle<()> {
    let mut engine = AlertEngine::new(database, agents, analytics, app_url);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        loop {
            tick.tick().await;
            if let Err(error) = engine.check_alerts(Utc::now()).await {
                tracing::error!(error = ?error, "check alerts");
            }
        }
    })
}

fn to_alert_key(alert: &Alert) -> AlertKey {
    (alert.rule_id.clone(), get_target_id(alert), alert.subject.clone())
}

fn get_target_id(alert: &Alert) -> String {
    alert
        .server_id
        .clone()
        .or_else(|| alert.check_id.clone())
        .or_else(|| alert.cron_id.clone())
        .unwrap_or_default()
}

fn get_target(alert: &Alert) -> Target {
    match (&alert.check_id, &alert.cron_id) {
        (Some(_), _) => Target::Check,
        (_, Some(_)) => Target::Cron,
        _ => Target::Server,
    }
}

impl AlertEngine {
    pub fn new(
        database: Database,
        agents: Agents,
        analytics: Analytics,
        app_url: Option<String>,
    ) -> Self {
        Self {
            database,
            agents,
            analytics,
            app_url,
            pending: HashMap::new(),
            clearing: HashMap::new(),
            mutes: Vec::new(),
            sent: HashMap::new(),
            limited: HashMap::new(),
            maintained: HashSet::new(),
        }
    }

    async fn count_logs(
        &self,
        condition: &Condition,
        now: DateTime<Utc>,
    ) -> Result<HashMap<String, u64>> {
        let Condition::Logs { level, within_secs, service, text, .. } = condition else {
            return Ok(HashMap::new());
        };
        let matcher = LogMatch {
            min_severity: read_log_severity(level).unwrap_or(17),
            service: service.clone(),
            text: text.clone(),
        };
        let since = now.timestamp_millis() - i64::from(*within_secs) * 1000;
        count_logs_by_server(&self.analytics, matcher, since).await
    }

    async fn aggregate_servers(
        &self,
        parsed: &ParsedRule,
        now: DateTime<Utc>,
    ) -> Result<Option<HashMap<String, f64>>> {
        let window = parsed.options.window_secs;
        let column = match parsed.condition {
            Condition::Cpu { .. } => "cpu",
            Condition::Memory { .. } => "mem_used",
            Condition::Load { .. } => "load1",
            Condition::Disk { .. } => "disk_used * 100.0 / nullif(disk_total, 0)",
            _ => return Ok(None),
        };
        if window == 0 {
            return Ok(None);
        }
        let since = now.timestamp_millis() - i64::from(window) * 1000;
        let expression = parsed.options.aggregation.to_sql(column);
        Ok(Some(aggregate_server_metric(&self.analytics, expression, since).await?))
    }

    async fn read_check_values(
        &self,
        parsed: &ParsedRule,
        now: DateTime<Utc>,
    ) -> Result<HashMap<String, f64>> {
        let column = match parsed.condition {
            Condition::ResponseTime { .. } => "response_ms",
            Condition::FirstByte { .. } => "ttfb_ms",
            Condition::CertDays { .. } => {
                let now_ms = now.timestamp_millis() as f64;
                return Ok(list_cert_expiries(&self.analytics)
                    .await?
                    .into_iter()
                    .map(|(id, at)| (id, ((at as f64 - now_ms) / DAY_MS).floor()))
                    .collect());
            }
            _ => return Ok(HashMap::new()),
        };
        let window = parsed.options.window_secs;
        let since =
            (window > 0).then(|| now.timestamp_millis() - i64::from(window) * 1000);
        let expression = parsed.options.aggregation.to_sql(column);
        aggregate_check_metric(&self.analytics, expression, column, since).await
    }

    async fn read_targets(
        &self,
        parsed: &ParsedRule,
        facts: &[ServerFacts<'_>],
        checks: &[UptimeCheck],
        crons: &[CronModel],
        down: &HashMap<String, String>,
        cron_failures: &HashMap<(String, String), String>,
        now: DateTime<Utc>,
    ) -> Result<Vec<TargetReadings>> {
        if parsed.condition.target() == Target::Cron {
            let (kind, label) = match parsed.condition {
                Condition::CronMissed => ("missed", "Missed"),
                _ => ("failed", "Failed"),
            };
            return Ok(crons
                .iter()
                .filter(|cron| !cron.paused && parsed.scope.includes_cron(&cron.id))
                .map(|cron| TargetReadings {
                    id: cron.id.clone(),
                    name: cron.name.clone(),
                    target: Target::Cron,
                    readings: Some(
                        cron_failures
                            .get(&(cron.id.clone(), kind.to_string()))
                            .map(|cause| {
                                make_reading("Status", 1.0, format!("{label}: {cause}"))
                            })
                            .into_iter()
                            .collect(),
                    ),
                })
                .collect());
        }
        if parsed.condition.target() == Target::Check {
            let values = self.read_check_values(parsed, now).await?;
            return Ok(checks
                .iter()
                .filter(|check| !check.paused && parsed.scope.includes_check(&check.id))
                .map(|check| {
                    let readings = match &parsed.condition {
                        Condition::UptimeDown => Some(
                            down.get(&check.id)
                                .map(|cause| {
                                    make_reading("Status", 1.0, format!("Down: {cause}"))
                                })
                                .into_iter()
                                .collect(),
                        ),
                        Condition::CertDays { .. } => values.get(&check.id).map(|days| {
                            vec![make_reading(
                                "Certificate",
                                *days,
                                format!("{days:.0} days left"),
                            )]
                        }),
                        Condition::FirstByte { .. } => values.get(&check.id).map(|ms| {
                            vec![make_reading("First byte", *ms, format_ms(*ms))]
                        }),
                        _ => values.get(&check.id).map(|ms| {
                            vec![make_reading("Response time", *ms, format_ms(*ms))]
                        }),
                    };
                    TargetReadings {
                        id: check.id.clone(),
                        name: check.name.clone(),
                        target: Target::Check,
                        readings,
                    }
                })
                .collect());
        }
        let log_counts = self.count_logs(&parsed.condition, now).await?;
        let aggregates = self.aggregate_servers(parsed, now).await?;
        Ok(facts
            .iter()
            .filter(|server| parsed.scope.includes(&server.server.id, &server.labels))
            .map(|server| {
                let is_connected = self.agents.is_connected(&server.server.id);
                let snapshot = ServerSnapshot {
                    is_connected,
                    mem_total: server.mem_total,
                    report: server.report.as_ref(),
                    log_count: log_counts
                        .get(&server.server.id)
                        .copied()
                        .unwrap_or_default(),
                };
                let readings = match &aggregates {
                    Some(values) if is_connected => {
                        values.get(&server.server.id).map(|value| {
                            let (subject, value) = match parsed.condition {
                                Condition::Memory { .. } => (
                                    "Memory",
                                    if server.mem_total > 0 {
                                        value * 100.0 / server.mem_total as f64
                                    } else {
                                        0.0
                                    },
                                ),
                                Condition::Disk { .. } => ("Disk", *value),
                                Condition::Load { .. } => ("Load", *value),
                                _ => ("CPU", *value),
                            };
                            let label = if subject == "Load" {
                                format!("{value:.2}")
                            } else {
                                format_percent(value)
                            };
                            vec![make_reading(subject, value, label)]
                        })
                    }
                    Some(_) => None,
                    None => find_readings(&parsed.condition, &snapshot),
                };
                TargetReadings {
                    id: server.server.id.clone(),
                    name: server.server.name.clone(),
                    target: Target::Server,
                    readings,
                }
            })
            .collect())
    }

    pub async fn check_alerts(&mut self, now: DateTime<Utc>) -> Result<()> {
        let rules = self.database.list_alert_rules().await?;
        let servers = self.database.list_servers().await?;
        let checks = self.database.list_uptime_checks().await?;
        let crons = self.database.list_crons().await?;
        let cron_failures: HashMap<(String, String), String> = self
            .database
            .list_open_cron_failures()
            .await?
            .into_iter()
            .map(|failure| ((failure.cron_id, failure.kind), failure.cause))
            .collect();
        let firing = self.database.list_firing_alerts().await?;
        let integrations = self.database.list_integrations().await?;
        let down: HashMap<String, String> = self
            .database
            .list_open_uptime_outages()
            .await?
            .into_iter()
            .filter(|outage| outage.kind == "down")
            .map(|outage| (outage.check_id, outage.cause))
            .collect();
        self.maintained =
            list_maintained_targets(&self.database.list_open_incidents().await?)
                .into_iter()
                .collect();
        let facts: Vec<ServerFacts> = servers.iter().map(read_server_facts).collect();
        let target_names: HashMap<String, String> = servers
            .iter()
            .map(|server| (server.id.clone(), server.name.clone()))
            .chain(checks.iter().map(|check| (check.id.clone(), check.name.clone())))
            .chain(crons.iter().map(|cron| (cron.id.clone(), cron.name.clone())))
            .collect();
        self.end_mutes(&rules, &firing, &target_names, &integrations, now)
            .await?;
        let mut firing_by_key: HashMap<AlertKey, &Alert> =
            firing.iter().map(|alert| (to_alert_key(alert), alert)).collect();

        let mut in_scope = HashSet::new();
        let mut evaluated = HashSet::new();
        let mut breaching = HashSet::new();
        let mut values: HashMap<AlertKey, f64> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for rule in rules.iter().filter(|rule| rule.enabled) {
            let Some(parsed) = parse_rule(rule) else {
                tracing::warn!(rule_id = %rule.id, "skipped an unreadable alert rule");
                continue;
            };
            let for_secs = rule.for_secs;
            for target in self
                .read_targets(
                    &parsed,
                    &facts,
                    &checks,
                    &crons,
                    &down,
                    &cron_failures,
                    now,
                )
                .await?
            {
                names.insert(target.id.clone(), target.name.clone());
                let pair = (rule.id.clone(), target.id.clone());
                in_scope.insert(pair.clone());
                let (readings, forced) = match (target.readings, parsed.options.no_data) {
                    (Some(readings), _) => (readings, false),
                    (None, NoData::Keep) => continue,
                    (None, NoData::Resolve) => (Vec::new(), false),
                    (None, NoData::Alert) => {
                        (vec![make_reading("No data", 0.0, "No data".to_string())], true)
                    }
                };
                evaluated.insert(pair);
                for reading in readings {
                    let key =
                        (rule.id.clone(), target.id.clone(), reading.subject.clone());
                    let level = if forced {
                        Level::Critical
                    } else {
                        rate(&parsed.condition, parsed.options.warning, reading.value)
                    };
                    if level == Level::Ok {
                        values.insert(key, reading.value);
                        continue;
                    }
                    breaching.insert(key.clone());
                    self.clearing.remove(&key);
                    let since = *self.pending.entry(key.clone()).or_insert(now);
                    if (now - since).num_seconds() < for_secs {
                        continue;
                    }
                    match firing_by_key.get(&key) {
                        Some(alert) if level > Level::read(&alert.severity) => {
                            self.escalate_alert(
                                rule,
                                &parsed,
                                alert,
                                &target.name,
                                target.target,
                                &reading,
                                level,
                                &integrations,
                            )
                            .await?;
                        }
                        Some(_) => {}
                        None => {
                            self.fire_alert(
                                rule,
                                &parsed,
                                &target.id,
                                &target.name,
                                target.target,
                                reading,
                                level,
                                since,
                                &integrations,
                            )
                            .await?;
                        }
                    }
                }
            }
        }

        for alert in &firing {
            let key = to_alert_key(alert);
            let pair = (key.0.clone(), key.1.clone());
            let rule = rules.iter().find(|rule| rule.id == alert.rule_id && rule.enabled);
            let Some(rule) = rule.filter(|_| in_scope.contains(&pair)) else {
                self.database
                    .resolve_alert(&AlertId::from(alert.id.as_str()), now)
                    .await?;
                continue;
            };
            let Some(parsed) = parse_rule(rule) else { continue };
            if breaching.contains(&key) {
                self.remind(rule, &parsed, alert, &names, now, &integrations).await?;
                continue;
            }
            if !evaluated.contains(&pair) {
                continue;
            }
            if let (Some(resolve_at), Some(value)) =
                (parsed.options.resolve_at, values.get(&key))
                && !is_past(&parsed.condition, resolve_at, *value)
                && *value != resolve_at
            {
                self.clearing.remove(&key);
                continue;
            }
            let since = *self.clearing.entry(key.clone()).or_insert(now);
            if (now - since).num_seconds() < i64::from(parsed.options.resolve_for_secs) {
                continue;
            }
            self.clearing.remove(&key);
            firing_by_key.remove(&key);
            let name = names.get(&key.1).cloned().unwrap_or_else(|| key.1.clone());
            self.resolve_alert(rule, &parsed, alert, &name, now, &integrations)
                .await?;
        }

        self.pending.retain(|key, _| breaching.contains(key));
        let open: HashSet<AlertKey> = firing.iter().map(to_alert_key).collect();
        self.clearing.retain(|key, _| open.contains(key));
        Ok(())
    }

    fn describe(&self, parsed: &ParsedRule, rule: &AlertRule) -> String {
        let mut summary = describe_rule(
            &parsed.condition,
            &parsed.options,
            u32::try_from(rule.for_secs).unwrap_or_default(),
        );
        if let Some(url) = &parsed.options.runbook_url {
            summary.push_str(&format!("\nRunbook: {url}"));
        }
        summary
    }

    fn get_target_url(&self, target: Target, id: &str) -> Option<String> {
        let path = match target {
            Target::Server => "servers",
            Target::Check => "uptime",
            Target::Cron => "crons",
        };
        self.app_url
            .as_ref()
            .map(|url| format!("{}/{path}/{id}", url.trim_end_matches('/')))
    }

    fn make_title(parsed: &ParsedRule, prefix: &str, rule: &AlertRule) -> String {
        match &parsed.options.severity_label {
            Some(label) => format!("[{label}] {prefix}: {}", rule.name),
            None => format!("{prefix}: {}", rule.name),
        }
    }

    async fn fire_alert(
        &mut self,
        rule: &AlertRule,
        parsed: &ParsedRule,
        target_id: &str,
        target_name: &str,
        target: Target,
        reading: Reading,
        level: Level,
        since: DateTime<Utc>,
        integrations: &[Integration],
    ) -> Result<()> {
        tracing::info!(rule_id = %rule.id, target_id, subject = %reading.subject, level = level.as_str(), "alert firing");
        let target_of = |kind: Target| (target == kind).then(|| target_id.to_string());
        let alert = self
            .database
            .create_alert(NewAlert {
                id: new_id(Prefix::Alert),
                rule_id: rule.id.clone(),
                server_id: target_of(Target::Server),
                check_id: target_of(Target::Check),
                cron_id: target_of(Target::Cron),
                subject: reading.subject,
                value: reading.label,
                severity: level.as_str().to_string(),
                started_at: since,
            })
            .await?;
        self.database
            .mark_alert_rule_fired(&AlertRuleId::from(rule.id.as_str()), Utc::now())
            .await?;
        let prefix = if level == Level::Warning { "Warning" } else { "Critical" };
        let notice = Notice {
            event: "alert.firing",
            title: Self::make_title(parsed, prefix, rule),
            body: format!(
                "{target_name} · {}: {}\n{}",
                alert.subject,
                alert.value,
                self.describe(parsed, rule)
            ),
            url: self.get_target_url(target, target_id),
            data: describe_alert(rule, target, target_id, target_name, &alert),
        };
        self.notify(
            parsed.recipients(level),
            integrations,
            &notice,
            rule,
            parsed,
            &alert,
        )
        .await?;
        if self.maintained.contains(target_id) {
            return Ok(());
        }
        let fire = RuleFire {
            rule,
            target_kind: match target {
                Target::Server => "server",
                Target::Check => "check",
                Target::Cron => "cron",
            },
            target_id,
            target_name,
            integration_ids: &parsed.integration_ids,
        };
        if let Err(error) =
            declare_from_rule(&self.database, self.app_url.as_deref(), fire).await
        {
            tracing::error!(rule_id = %rule.id, error = ?error, "declare an incident");
        }
        Ok(())
    }

    async fn escalate_alert(
        &mut self,
        rule: &AlertRule,
        parsed: &ParsedRule,
        alert: &Alert,
        target_name: &str,
        target: Target,
        reading: &Reading,
        level: Level,
        integrations: &[Integration],
    ) -> Result<()> {
        let now = Utc::now();
        self.database
            .escalate_alert(
                &AlertId::from(alert.id.as_str()),
                level.as_str(),
                reading.label.clone(),
                now,
            )
            .await?;
        let escalated = Alert {
            severity: level.as_str().to_string(),
            value: reading.label.clone(),
            ..alert.clone()
        };
        let target_id = get_target_id(alert);
        let notice = Notice {
            event: "alert.firing",
            title: Self::make_title(parsed, "Critical", rule),
            body: format!(
                "{target_name} · {}: {} (was a warning)\n{}",
                alert.subject,
                reading.label,
                self.describe(parsed, rule)
            ),
            url: self.get_target_url(target, &target_id),
            data: describe_alert(rule, target, &target_id, target_name, &escalated),
        };
        self.notify(parsed.recipients(level), integrations, &notice, rule, parsed, alert)
            .await
    }

    async fn remind(
        &mut self,
        rule: &AlertRule,
        parsed: &ParsedRule,
        alert: &Alert,
        names: &HashMap<String, String>,
        now: DateTime<Utc>,
        integrations: &[Integration],
    ) -> Result<()> {
        let Some(every) = parsed.options.remind_every_secs else {
            return Ok(());
        };
        let sent = u32::try_from(alert.reminders_sent).unwrap_or_default();
        let last = alert.last_notified_at.unwrap_or(alert.started_at);
        if sent >= parsed.options.remind_times
            || (now - last).num_seconds() < i64::from(every)
        {
            return Ok(());
        }
        self.database
            .mark_alert_reminded(
                &AlertId::from(alert.id.as_str()),
                alert.reminders_sent + 1,
                now,
            )
            .await?;
        let level = Level::read(&alert.severity);
        let target_id = get_target_id(alert);
        let target = get_target(alert);
        let target_name =
            names.get(&target_id).cloned().unwrap_or_else(|| target_id.clone());
        let lasted =
            u32::try_from((now - alert.started_at).num_seconds()).unwrap_or_default();
        let notice = Notice {
            event: "alert.firing",
            title: Self::make_title(parsed, "Still firing", rule),
            body: format!(
                "{target_name} · {}: {} for {}\n{}",
                alert.subject,
                alert.value,
                format_duration(lasted),
                self.describe(parsed, rule)
            ),
            url: self.get_target_url(target, &target_id),
            data: describe_alert(rule, target, &target_id, &target_name, alert),
        };
        self.notify(parsed.recipients(level), integrations, &notice, rule, parsed, alert)
            .await
    }

    async fn resolve_alert(
        &mut self,
        rule: &AlertRule,
        parsed: &ParsedRule,
        alert: &Alert,
        target_name: &str,
        now: DateTime<Utc>,
        integrations: &[Integration],
    ) -> Result<()> {
        tracing::info!(alert_id = %alert.id, "alert resolved");
        self.database
            .resolve_alert(&AlertId::from(alert.id.as_str()), now)
            .await?;
        let lasted =
            u32::try_from((now - alert.started_at).num_seconds()).unwrap_or_default();
        let resolved = Alert {
            resolved_at: Some(now),
            status: "resolved".to_string(),
            ..alert.clone()
        };
        let target_id = get_target_id(alert);
        let target = get_target(alert);
        let notice = Notice {
            event: "alert.resolved",
            title: Self::make_title(parsed, "Resolved", rule),
            body: format!(
                "{target_name} · {} is back to normal after {}",
                alert.subject,
                format_duration(lasted)
            ),
            url: self.get_target_url(target, &target_id),
            data: describe_alert(rule, target, &target_id, target_name, &resolved),
        };
        self.notify(
            parsed.recipients(Level::read(&alert.severity)),
            integrations,
            &notice,
            rule,
            parsed,
            alert,
        )
        .await
    }

    fn is_muted(&self, rule_id: &str, alert: &Alert) -> bool {
        let target_id = get_target_id(alert);
        self.mutes.iter().any(|mute| mute.covers(rule_id, &target_id))
    }

    async fn end_mutes(
        &mut self,
        rules: &[AlertRule],
        firing: &[Alert],
        names: &HashMap<String, String>,
        integrations: &[Integration],
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mutes = self.database.list_alert_mutes().await?;
        let covered = |mute: &AlertMute| -> Vec<&Alert> {
            firing
                .iter()
                .filter(|alert| mute.covers(&alert.rule_id, &get_target_id(alert)))
                .collect()
        };
        let (ended, active): (Vec<AlertMute>, Vec<AlertMute>) =
            mutes.into_iter().partition(|mute| match mute.ends_at {
                Some(at) => at <= now,
                None => covered(mute).is_empty(),
            });
        self.mutes = active;
        for mute in ended {
            self.database
                .delete_alert_mute(&AlertMuteId::from(mute.id.as_str()))
                .await?;
            if mute.ends_at.is_none() {
                continue;
            }
            let Some(rule) =
                rules.iter().find(|rule| rule.id == mute.rule_id && rule.enabled)
            else {
                continue;
            };
            let Some(parsed) = parse_rule(rule) else { continue };
            for alert in covered(&mute) {
                if self.is_muted(&rule.id, alert) {
                    continue;
                }
                let target_id = get_target_id(alert);
                let target = get_target(alert);
                let target_name =
                    names.get(&target_id).cloned().unwrap_or_else(|| target_id.clone());
                let lasted = u32::try_from((now - alert.started_at).num_seconds())
                    .unwrap_or_default();
                let notice = Notice {
                    event: "alert.firing",
                    title: Self::make_title(&parsed, "Still firing", rule),
                    body: format!(
                        "{target_name} · {}: {} for {}\nThe mute ended and it is still firing.",
                        alert.subject,
                        alert.value,
                        format_duration(lasted)
                    ),
                    url: self.get_target_url(target, &target_id),
                    data: describe_alert(rule, target, &target_id, &target_name, alert),
                };
                let level = Level::read(&alert.severity);
                self.notify(
                    parsed.recipients(level),
                    integrations,
                    &notice,
                    rule,
                    &parsed,
                    alert,
                )
                .await?;
            }
        }
        Ok(())
    }

    fn take_slot(&mut self, rule_id: &str, limit: u32, now: DateTime<Utc>) -> Slot {
        let sent = self.sent.entry(rule_id.to_string()).or_default();
        sent.retain(|at| (now - *at).num_seconds() < LIMIT_WINDOW_SECS);
        if sent.len() < usize::try_from(limit).unwrap_or(usize::MAX) {
            sent.push(now);
            return Slot::Send;
        }
        if self.limited.get(rule_id).is_some_and(|until| *until > now) {
            return Slot::Drop;
        }
        let until = sent
            .first()
            .map_or(now, |at| *at + chrono::Duration::seconds(LIMIT_WINDOW_SECS));
        self.limited.insert(rule_id.to_string(), until);
        Slot::Summary(until)
    }

    async fn notify(
        &mut self,
        recipients: &[String],
        integrations: &[Integration],
        notice: &Notice,
        rule: &AlertRule,
        parsed: &ParsedRule,
        alert: &Alert,
    ) -> Result<()> {
        let now = Utc::now();
        if self.is_muted(&rule.id, alert)
            || parsed.options.is_quiet(now)
            || self.maintained.contains(&get_target_id(alert))
        {
            return Ok(());
        }
        let log = DeliveryLog {
            rule_id: rule.id.clone(),
            alert_id: Some(alert.id.clone()),
        };
        let Some(limit) = parsed.options.message_limit else {
            notify_integrations(&self.database, recipients, integrations, notice, &log);
            return Ok(());
        };
        match self.take_slot(&rule.id, limit, now) {
            Slot::Send => {
                notify_integrations(
                    &self.database,
                    recipients,
                    integrations,
                    notice,
                    &log,
                );
            }
            Slot::Drop => {}
            Slot::Summary(until) => {
                self.database
                    .mark_alert_rule_quiet(&AlertRuleId::from(rule.id.as_str()), until)
                    .await?;
                let time_zone = read_time_zone(parsed.options.time_zone.as_deref());
                let summary = Notice {
                    event: "alert.limited",
                    title: Self::make_title(parsed, "Limit reached", rule),
                    body: format!(
                        "{limit} messages in the last hour, so this rule is quiet until {}\nSee every alert in Daroyan.",
                        format_clock(until, time_zone)
                    ),
                    url: self.app_url.as_ref().map(|url| {
                        format!("{}/alerts/{}", url.trim_end_matches('/'), rule.id)
                    }),
                    data: json!({
                        "rule": { "id": rule.id, "name": rule.name },
                        "quiet_until": until.format(crate::time::ISO_FORMAT).to_string(),
                    }),
                };
                let log = DeliveryLog { rule_id: rule.id.clone(), alert_id: None };
                notify_integrations(
                    &self.database,
                    recipients,
                    integrations,
                    &summary,
                    &log,
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct DeliveryLog {
    pub rule_id: String,
    pub alert_id: Option<String>,
}

pub fn notify_integrations(
    database: &Database,
    integration_ids: &[String],
    integrations: &[Integration],
    notice: &Notice,
    log: &DeliveryLog,
) {
    for integration in
        integrations.iter().filter(|item| integration_ids.contains(&item.id))
    {
        let database = database.clone();
        let integration = integration.clone();
        let notice = notice.clone();
        let log = log.clone();
        tokio::spawn(async move {
            if let Err(error) =
                deliver_notice(&database, &integration, &notice, Some(log)).await
            {
                tracing::warn!(integration_id = %integration.id, error = %error, "send an alert");
            }
        });
    }
}

pub async fn deliver_notice(
    database: &Database,
    integration: &Integration,
    notice: &Notice,
    log: Option<DeliveryLog>,
) -> Result<(), String> {
    let outcome =
        match serde_json::from_value::<IntegrationConfig>(integration.config.clone()) {
            Ok(config) => send_notice(&config, notice).await,
            Err(_) => {
                Err("The integration settings are unreadable. Edit and save it again"
                    .to_string())
            }
        };
    if let Some(log) = log {
        let delivery = NewAlertDelivery {
            id: new_id(Prefix::AlertDelivery),
            rule_id: log.rule_id,
            alert_id: log.alert_id,
            integration_id: integration.id.clone(),
            integration_name: integration.name.clone(),
            message: notice.summarize(),
            error: outcome.clone().err(),
            sent_at: Utc::now(),
        };
        if let Err(error) = database.create_alert_delivery(delivery).await {
            tracing::error!(integration_id = %integration.id, error = %error, "log a delivery");
        }
    }
    let id = IntegrationId::from(integration.id.as_str());
    if let Err(error) =
        database.record_integration_delivery(&id, outcome.clone().err()).await
    {
        tracing::error!(integration_id = %integration.id, error = %error, "record a delivery");
    }
    outcome
}

fn read_server_facts(server: &Server) -> ServerFacts<'_> {
    ServerFacts {
        server,
        labels: serde_json::from_value(server.labels.clone()).unwrap_or_else(|error| {
            tracing::warn!(server_id = %server.id, %error, "unreadable server labels, using none");
            Vec::new()
        }),
        mem_total: serde_json::from_value::<Host>(server.host.clone())
            .map(|host| host.mem_total)
            .unwrap_or_default(),
        report: server
            .last_report
            .clone()
            .and_then(|report| serde_json::from_value(report).ok()),
    }
}

fn describe_alert(
    rule: &AlertRule,
    target: Target,
    target_id: &str,
    target_name: &str,
    alert: &Alert,
) -> serde_json::Value {
    let target_key = match target {
        Target::Server => "server",
        Target::Check => "check",
        Target::Cron => "cron",
    };
    json!({
        "id": alert.id,
        "status": alert.status,
        "severity": alert.severity,
        "rule": { "id": rule.id, "name": rule.name },
        (target_key): { "id": target_id, "name": target_name },
        "subject": alert.subject,
        "value": alert.value,
        "started_at": alert.started_at.format(crate::time::ISO_FORMAT).to_string(),
        "resolved_at": alert.resolved_at.map(|at| at.format(crate::time::ISO_FORMAT).to_string()),
    })
}
