use chrono::{DateTime, Utc};

use crate::alerts::evaluate::Level;
use crate::alerts::notify::{Details, Field, Link, Text, Tone};
use crate::alerts::rules::{
    Condition, RuleOptions, Target, describe_rule, format_duration, format_moment,
    read_time_zone,
};
use crate::db::entities::alert_rules::Model as AlertRule;
use crate::db::entities::alerts::Model as Alert;

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
}

pub struct AlertMessage<'a> {
    pub rule: &'a AlertRule,
    pub condition: &'a Condition,
    pub options: &'a RuleOptions,
    pub alert: &'a Alert,
    pub target: Target,
    pub target_id: &'a str,
    pub target_name: &'a str,
    pub labels: &'a [String],
    pub app_url: Option<&'a str>,
    pub now: DateTime<Utc>,
}

impl AlertMessage<'_> {
    pub fn details(&self, moment: Moment) -> Details {
        let alert = self.alert;
        let summary = match moment {
            Moment::Firing => Text::from(format!("{}: ", alert.subject))
                .bold(alert.value.as_str())
                .plain("."),
            Moment::Escalated => Text::from(format!("{}: ", alert.subject))
                .bold(alert.value.as_str())
                .plain(". It was a warning before."),
            Moment::Reminder { lasted_secs } | Moment::MuteEnded { lasted_secs } => {
                Text::from(format!("{}: ", alert.subject))
                    .bold(alert.value.as_str())
                    .plain(", still firing after ")
                    .bold(format_duration(lasted_secs))
                    .plain(".")
            }
            Moment::Resolved { lasted_secs } => {
                Text::from(format!("{} is back to normal after ", alert.subject))
                    .bold(format_duration(lasted_secs))
                    .plain(".")
            }
        };
        let time_zone = read_time_zone(self.options.time_zone.as_deref());
        let mut fields = vec![
            Field::new(self.target_kind(), self.target_label()),
            Field::new("Started", format_moment(alert.started_at, self.now, time_zone)),
            Field::new("Rule", &self.rule.name),
        ];
        if !moment.is_resolved() {
            let for_secs = u32::try_from(self.rule.for_secs).unwrap_or_default();
            fields.push(Field::new(
                "Fires when",
                describe_rule(self.condition, self.options, for_secs),
            ));
        }
        let note = match moment {
            Moment::MuteEnded { .. } => {
                Text::from("The mute ended and it is still firing.")
            }
            _ => Text::default(),
        };
        Details {
            summary,
            fields,
            note,
            links: self.links(moment),
            ..Details::default()
        }
    }

    fn links(&self, moment: Moment) -> Vec<Link> {
        let Some(app_url) = self.app_url.map(|url| url.trim_end_matches('/')) else {
            return Vec::new();
        };
        let (label, path) = match self.target {
            Target::Server => ("Open server", "servers"),
            Target::Check => ("Open check", "uptime"),
            Target::Cron => ("Open cron", "crons"),
        };
        let open =
            Link::new(label, format!("{app_url}/{path}/{}", self.target_id)).primary();
        if moment.is_resolved() {
            return vec![open];
        }
        let view = Link::new("View alert", format!("{app_url}/alerts/{}", self.rule.id));
        let runbook = self
            .options
            .runbook_url
            .as_deref()
            .map(|url| Link::new("Runbook", url));
        [open, view].into_iter().chain(runbook).collect()
    }

    fn target_kind(&self) -> &'static str {
        match self.target {
            Target::Server => "Server",
            Target::Check => "Check",
            Target::Cron => "Cron",
        }
    }

    fn target_label(&self) -> String {
        std::iter::once(self.target_name)
            .chain(self.labels.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, hour, minute, 0).unwrap()
    }

    fn make_rule() -> AlertRule {
        AlertRule {
            id: "rule_1".to_string(),
            name: "InboxJot error spike".to_string(),
            enabled: true,
            condition: json!({}),
            for_secs: 0,
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

    fn make_alert() -> Alert {
        Alert {
            id: "alert_1".to_string(),
            rule_id: "rule_1".to_string(),
            server_id: Some("server_1".to_string()),
            check_id: None,
            cron_id: None,
            subject: "Logs".to_string(),
            value: "11 lines".to_string(),
            severity: "critical".to_string(),
            status: "firing".to_string(),
            started_at: at(8, 54),
            resolved_at: None,
            last_notified_at: None,
            reminders_sent: 0,
        }
    }

    fn make_message<'a>(
        rule: &'a AlertRule,
        condition: &'a Condition,
        options: &'a RuleOptions,
        alert: &'a Alert,
        labels: &'a [String],
    ) -> AlertMessage<'a> {
        AlertMessage {
            rule,
            condition,
            options,
            alert,
            target: Target::Server,
            target_id: "server_1",
            target_name: "Inboxjot Server",
            labels,
            app_url: Some("https://daroyan.test/"),
            now: at(9, 0),
        }
    }

    #[test]
    fn builds_a_firing_message() {
        let (rule, alert) = (make_rule(), make_alert());
        let condition = Condition::Cpu { above: 90.0 };
        let options = RuleOptions {
            runbook_url: Some("https://wiki.test/cpu".to_string()),
            ..RuleOptions::default()
        };
        let labels = vec!["production".to_string()];
        let message = make_message(&rule, &condition, &options, &alert, &labels);
        let details = message.details(Moment::Firing);
        assert_eq!(details.summary.to_string(), "Logs: 11 lines.");
        assert_eq!(
            details.fields,
            vec![
                Field::new("Server", "Inboxjot Server · production"),
                Field::new("Started", "Today at 8:54 AM UTC"),
                Field::new("Rule", "InboxJot error spike"),
                Field::new("Fires when", "CPU above 90%"),
            ]
        );
        assert_eq!(
            details.links,
            vec![
                Link::new("Open server", "https://daroyan.test/servers/server_1")
                    .primary(),
                Link::new("View alert", "https://daroyan.test/alerts/rule_1"),
                Link::new("Runbook", "https://wiki.test/cpu"),
            ]
        );
        assert_eq!(Moment::Firing.tone("warning"), Tone::Warning);
        assert_eq!(Moment::Escalated.tone("warning"), Tone::Critical);
    }

    #[test]
    fn builds_a_resolved_message() {
        let (rule, alert) = (make_rule(), make_alert());
        let condition = Condition::Cpu { above: 90.0 };
        let options = RuleOptions::default();
        let message = make_message(&rule, &condition, &options, &alert, &[]);
        let details = message.details(Moment::Resolved { lasted_secs: 3060 });
        assert_eq!(details.summary.to_string(), "Logs is back to normal after 51m.");
        assert_eq!(details.fields.len(), 3);
        assert_eq!(details.links.len(), 1);
        assert_eq!(Moment::Resolved { lasted_secs: 0 }.tone("critical"), Tone::Resolved);
    }

    #[test]
    fn leaves_out_links_without_an_app_url() {
        let (rule, alert) = (make_rule(), make_alert());
        let condition = Condition::Offline;
        let options = RuleOptions::default();
        let message = AlertMessage {
            app_url: None,
            ..make_message(&rule, &condition, &options, &alert, &[])
        };
        assert!(message.details(Moment::Firing).links.is_empty());
    }
}
