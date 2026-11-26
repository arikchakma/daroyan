use std::collections::{BTreeSet, HashMap};

use crate::alerts::rules::{Condition, format_duration};
use crate::analytics::Analytics;
use crate::analytics::access::{Failure, Login, Since, list_failures, list_logins};
use crate::error::Result;

pub const LOGIN_WINDOW_MS: i64 = 15 * 60_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginRule {
    NewIp { days: u32 },
    Root,
    AfterFailures { failures: u32, within_secs: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoginHit {
    pub server_id: String,
    pub ip: String,
    pub at: i64,
    pub label: String,
}

impl Condition {
    pub fn login_rule(&self) -> Option<LoginRule> {
        match self {
            Self::LoginNewIp { days } => Some(LoginRule::NewIp { days: *days }),
            Self::LoginRoot => Some(LoginRule::Root),
            Self::LoginAfterFailures { failures, within_secs } => {
                Some(LoginRule::AfterFailures {
                    failures: *failures,
                    within_secs: *within_secs,
                })
            }
            _ => None,
        }
    }
}

impl Login {
    fn who(&self) -> &str {
        if self.user.is_empty() { "Someone" } else { &self.user }
    }
}

fn count_attempts(login: &Login, failures: &[Failure], window_ms: i64) -> u32 {
    let mut by_port: HashMap<i64, u32> = HashMap::new();
    let mut unported = 0;
    let matching = failures.iter().filter(|failure| {
        failure.server_id == login.server_id
            && failure.ip == login.ip
            && (login.at - window_ms..=login.at).contains(&failure.at)
    });
    for failure in matching {
        match failure.port {
            Some(port) => {
                *by_port.entry(port).or_default() += u32::from(failure.is_attempt);
            }
            None => unported += 1,
        }
    }

    unported + by_port.into_values().map(|attempts| attempts.max(1)).sum::<u32>()
}

impl LoginRule {
    fn failure_window_ms(self) -> Option<i64> {
        match self {
            Self::AfterFailures { within_secs, .. } => {
                Some(i64::from(within_secs) * 1000)
            }
            _ => None,
        }
    }

    fn describe(self, login: &Login, failures: &[Failure]) -> Option<String> {
        let who = login.who();
        let ip = &login.ip;
        match self {
            Self::NewIp { days } => match login.last_seen.map(|seen| login.at - seen) {
                None => Some(format!(
                    "{who} logged in from {ip}, the first login from this address"
                )),
                Some(gap) if gap > i64::from(days) * DAY_MS => Some(format!(
                    "{who} logged in from {ip}, {} days after the last login from it",
                    gap / DAY_MS
                )),
                Some(_) => None,
            },
            Self::Root => (login.user == "root").then(|| match &login.method {
                Some(method) => format!("root logged in from {ip} with {method}"),
                None => format!("root logged in from {ip}"),
            }),
            Self::AfterFailures { failures: least, within_secs } => {
                let window_ms = i64::from(within_secs) * 1000;
                let attempts = count_attempts(login, failures, window_ms);
                (attempts >= least).then(|| {
                    format!(
                        "{who} logged in from {ip} after {attempts} failed attempts in {}",
                        format_duration(within_secs)
                    )
                })
            }
        }
    }

    pub fn find_hits(self, logins: &[Login], failures: &[Failure]) -> Vec<LoginHit> {
        logins
            .iter()
            .filter_map(|login| {
                Some(LoginHit {
                    server_id: login.server_id.clone(),
                    ip: login.ip.clone(),
                    at: login.at,
                    label: self.describe(login, failures)?,
                })
            })
            .collect()
    }
}

pub async fn find_login_hits(
    analytics: &Analytics,
    rule: LoginRule,
    since: Since,
) -> Result<Vec<LoginHit>> {
    let logins = list_logins(analytics, since).await?;
    let failures = match (rule.failure_window_ms(), logins.first()) {
        (Some(window_ms), Some(first)) => {
            let ips: BTreeSet<String> =
                logins.iter().map(|login| login.ip.clone()).collect();
            list_failures(analytics, first.at - window_ms, ips.into_iter().collect())
                .await?
        }
        _ => Vec::new(),
    };

    Ok(rule.find_hits(&logins, &failures))
}

#[cfg(test)]
mod tests {
    use super::{DAY_MS, LoginRule, count_attempts};
    use crate::analytics::access::{Failure, Login};

    fn login(user: &str, at: i64, last_seen: Option<i64>) -> Login {
        Login {
            server_id: "srv_1".to_string(),
            at,
            user: user.to_string(),
            ip: "203.0.113.7".to_string(),
            method: Some("publickey".to_string()),
            last_seen,
        }
    }

    fn failure(at: i64, port: Option<i64>, is_attempt: bool) -> Failure {
        Failure {
            server_id: "srv_1".to_string(),
            at,
            ip: "203.0.113.7".to_string(),
            port,
            is_attempt,
        }
    }

    #[test]
    fn finds_logins_from_new_addresses() {
        let now = 100 * DAY_MS;
        let rule = LoginRule::NewIp { days: 30 };
        let logins = [
            login("arik", now, None),
            login("arik", now, Some(now - 45 * DAY_MS)),
            login("arik", now, Some(now - 2 * DAY_MS)),
        ];

        let labels: Vec<String> = rule
            .find_hits(&logins, &[])
            .into_iter()
            .map(|hit| hit.label)
            .collect();
        assert_eq!(
            labels,
            [
                "arik logged in from 203.0.113.7, the first login from this address",
                "arik logged in from 203.0.113.7, 45 days after the last login from it",
            ]
        );
    }

    #[test]
    fn finds_root_logins() {
        let hits = LoginRule::Root
            .find_hits(&[login("root", 1, None), login("deploy", 2, None)], &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "root logged in from 203.0.113.7 with publickey");
    }

    #[test]
    fn counts_each_failed_attempt_once() {
        let success = login("deploy", 610_000, Some(1));
        let failures = [
            failure(100_000, Some(5001), true),
            failure(100_001, Some(5001), false),
            failure(200_000, Some(5002), true),
            failure(200_001, Some(5002), true),
            failure(300_000, Some(5003), false),
            failure(400_000, None, true),
            failure(1, Some(5004), true),
        ];
        assert_eq!(count_attempts(&success, &failures, 600_000), 5);

        let rule = LoginRule::AfterFailures { failures: 5, within_secs: 600 };
        let hits = rule.find_hits(std::slice::from_ref(&success), &failures);
        assert_eq!(
            hits[0].label,
            "deploy logged in from 203.0.113.7 after 5 failed attempts in 10m"
        );

        let strict = LoginRule::AfterFailures { failures: 6, within_secs: 600 };
        assert!(strict.find_hits(&[success], &failures).is_empty());
    }
}
