use std::collections::{BTreeSet, HashMap};

use crate::alerts::rules::{Condition, format_duration};
use crate::analytics::Analytics;
use crate::analytics::access::{
    Failure, FailureKind, Login, Since, list_failures, list_first_logins, list_logins,
};
use crate::error::Result;

pub(crate) const LOGIN_WINDOW_MS: i64 = 15 * 60_000;
const LEARNING_MS: i64 = 86_400_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoginRule {
    NewIp { days: u32 },
    Root,
    AfterFailures { failures: u32, within_secs: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoginHit {
    pub server_id: String,
    pub ip: String,
    pub at: i64,
    pub label: String,
}

impl Condition {
    pub(crate) fn login_rule(&self) -> Option<LoginRule> {
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

#[derive(Debug, Default)]
struct History {
    failures: HashMap<(String, String), Vec<Failure>>,
    first_logins: HashMap<String, i64>,
}

impl History {
    fn with_failures(failures: Vec<Failure>) -> Self {
        let mut grouped: HashMap<(String, String), Vec<Failure>> = HashMap::new();
        for failure in failures {
            let key = (failure.server_id.clone(), failure.ip.clone());
            grouped.entry(key).or_default().push(failure);
        }

        Self { failures: grouped, ..Self::default() }
    }

    fn failures_for(&self, login: &Login) -> &[Failure] {
        self.failures
            .get(&(login.server_id.clone(), login.ip.clone()))
            .map_or(&[], Vec::as_slice)
    }

    fn has_learned(&self, login: &Login) -> bool {
        self.first_logins
            .get(&login.server_id)
            .is_some_and(|first| login.at - first >= LEARNING_MS)
    }
}

#[derive(Default)]
struct Connection {
    attempts: u32,
    has_other: bool,
}

impl Connection {
    fn add(&mut self, kind: FailureKind) {
        match kind {
            FailureKind::Attempt => self.attempts += 1,
            FailureKind::Other => self.has_other = true,
            FailureKind::Probe => {}
        }
    }

    fn count(&self) -> u32 {
        if self.attempts > 0 { self.attempts } else { u32::from(self.has_other) }
    }
}

fn count_attempts(login: &Login, failures: &[Failure], window_ms: i64) -> u32 {
    let mut connections: HashMap<(Option<i64>, Option<i64>, usize), Connection> =
        HashMap::new();
    let matching = failures
        .iter()
        .enumerate()
        .filter(|(_, failure)| (login.at - window_ms..=login.at).contains(&failure.at));
    for (index, failure) in matching {
        let alone = if failure.port.is_some() { 0 } else { index };
        connections
            .entry((failure.pid, failure.port, alone))
            .or_default()
            .add(failure.kind);
    }

    connections.values().map(Connection::count).sum()
}

impl LoginRule {
    fn describe(self, login: &Login, history: &History) -> Option<String> {
        let who = login.who();
        let ip = &login.ip;
        match self {
            Self::NewIp { .. } if !history.has_learned(login) => None,
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
                let attempts =
                    count_attempts(login, history.failures_for(login), window_ms);
                (attempts >= least).then(|| {
                    format!(
                        "{who} logged in from {ip} after {attempts} failed attempts in {}",
                        format_duration(within_secs)
                    )
                })
            }
        }
    }

    fn find_hits(self, logins: &[Login], history: &History) -> Vec<LoginHit> {
        logins
            .iter()
            .filter_map(|login| {
                Some(LoginHit {
                    server_id: login.server_id.clone(),
                    ip: login.ip.clone(),
                    at: login.at,
                    label: self.describe(login, history)?,
                })
            })
            .collect()
    }
}

pub(crate) async fn find_login_hits(
    analytics: &Analytics,
    rule: LoginRule,
    since: Since,
) -> Result<Vec<LoginHit>> {
    let logins = list_logins(analytics, since).await?;
    let Some(first) = logins.first() else {
        return Ok(Vec::new());
    };

    let history = match rule {
        LoginRule::NewIp { .. } => History {
            first_logins: list_first_logins(analytics).await?,
            ..History::default()
        },
        LoginRule::AfterFailures { within_secs, .. } => {
            let ips: BTreeSet<String> =
                logins.iter().map(|login| login.ip.clone()).collect();
            let from = first.at - i64::from(within_secs) * 1000;
            History::with_failures(
                list_failures(analytics, from, ips.into_iter().collect()).await?,
            )
        }
        LoginRule::Root => History::default(),
    };

    Ok(rule.find_hits(&logins, &history))
}

#[cfg(test)]
mod tests {
    use super::{DAY_MS, History, LEARNING_MS, LoginRule, count_attempts};
    use crate::analytics::access::{Failure, FailureKind, Login};

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

    fn failure(at: i64, port: Option<i64>, kind: FailureKind) -> Failure {
        Failure {
            server_id: "srv_1".to_string(),
            at,
            pid: Some(9),
            ip: "203.0.113.7".to_string(),
            port,
            kind,
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

        let history = History {
            first_logins: [("srv_1".to_string(), now - 60 * DAY_MS)].into(),
            ..History::default()
        };
        let labels: Vec<String> = rule
            .find_hits(&logins, &history)
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
    fn learns_addresses_for_a_day_first() {
        let now = 100 * DAY_MS;
        let rule = LoginRule::NewIp { days: 30 };
        let logins = [login("arik", now, None)];
        let learning = History {
            first_logins: [("srv_1".to_string(), now - LEARNING_MS + 1)].into(),
            ..History::default()
        };
        assert!(rule.find_hits(&logins, &learning).is_empty());
        assert!(rule.find_hits(&logins, &History::default()).is_empty());

        let learned = History {
            first_logins: [("srv_1".to_string(), now - LEARNING_MS)].into(),
            ..History::default()
        };
        assert_eq!(rule.find_hits(&logins, &learned).len(), 1);
    }

    #[test]
    fn finds_root_logins() {
        let hits = LoginRule::Root.find_hits(
            &[login("root", 1, None), login("deploy", 2, None)],
            &History::default(),
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "root logged in from 203.0.113.7 with publickey");
    }

    #[test]
    fn counts_each_failed_attempt_once() {
        let success = login("deploy", 610_000, Some(1));
        let failures = [
            failure(100_000, Some(5001), FailureKind::Attempt),
            failure(100_001, Some(5001), FailureKind::Other),
            failure(200_000, Some(5002), FailureKind::Attempt),
            failure(200_001, Some(5002), FailureKind::Attempt),
            failure(300_000, Some(5003), FailureKind::Other),
            failure(400_000, None, FailureKind::Attempt),
            failure(450_000, Some(5005), FailureKind::Probe),
            failure(1, Some(5004), FailureKind::Attempt),
        ];
        assert_eq!(count_attempts(&success, &failures, 600_000), 5);

        let rule = LoginRule::AfterFailures { failures: 5, within_secs: 600 };
        let history = History::with_failures(failures.to_vec());
        let hits = rule.find_hits(std::slice::from_ref(&success), &history);
        assert_eq!(
            hits[0].label,
            "deploy logged in from 203.0.113.7 after 5 failed attempts in 10m"
        );

        let strict = LoginRule::AfterFailures { failures: 6, within_secs: 600 };
        assert!(strict.find_hits(&[success], &history).is_empty());
    }
}
