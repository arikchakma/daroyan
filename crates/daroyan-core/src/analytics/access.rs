use std::collections::HashMap;

use daroyan_protocol::log::{ACCESS_SERVICE, ACCESS_STREAM};
use duckdb::types::Value as SqlValue;
use duckdb::{Connection, params, params_from_iter};
use serde::Serialize;
use serde_json::{Map, Value};
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::analytics::log_lines::LogRow;
use crate::analytics::log_parse::SSH_PROCESSES;
use crate::error::Result;

const DAY_MS: i64 = 86_400_000;
const LAST_SEEN: &str = "ssh.ip_last_seen";
const SOURCE_CHUNK: usize = 500;

type SourceKey = (String, String);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Login {
    pub server_id: String,
    pub at: i64,
    pub user: String,
    pub ip: String,
    pub method: Option<String>,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub server_id: String,
    pub at: i64,
    pub pid: Option<i64>,
    pub ip: String,
    pub port: Option<i64>,
    pub kind: FailureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Attempt,
    Probe,
    Other,
}

impl From<Option<String>> for FailureKind {
    fn from(method: Option<String>) -> Self {
        match method.as_deref() {
            None => Self::Other,
            Some("none") => Self::Probe,
            Some(_) => Self::Attempt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Since {
    Ingested(i64),
    Logged(i64),
}

impl Since {
    fn column(self) -> (&'static str, i64) {
        match self {
            Self::Ingested(at) => ("ingested_at", at),
            Self::Logged(at) => ("logged_at", at),
        }
    }
}

#[derive(Clone, Copy)]
struct Seen {
    first: i64,
    last: i64,
}

impl Seen {
    fn add(&mut self, at: i64) {
        self.first = self.first.min(at);
        self.last = self.last.max(at);
    }
}

fn text<'a>(attributes: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

fn read_success(row: &LogRow) -> Option<(String, Map<String, Value>)> {
    if row.service != ACCESS_SERVICE || row.stream != ACCESS_STREAM {
        return None;
    }
    let raw = row.attributes.as_deref()?;
    if !raw.contains("\"ssh_login\"") {
        return None;
    }

    let attributes: Map<String, Value> = serde_json::from_str(raw).ok()?;
    let is_sshd = text(&attributes, "process.name")
        .is_some_and(|name| SSH_PROCESSES.contains(&name));
    let is_success = text(&attributes, "event.action") == Some("ssh_login")
        && text(&attributes, "event.outcome") == Some("success");
    let ip = text(&attributes, "client.ip")?.to_string();

    (is_sshd && is_success).then_some((ip, attributes))
}

fn read_last_seen<'a>(
    conn: &Connection,
    keys: impl Iterator<Item = &'a SourceKey>,
) -> duckdb::Result<HashMap<SourceKey, i64>> {
    let keys: Vec<&SourceKey> = keys.collect();
    let mut seen = HashMap::new();
    for chunk in keys.chunks(SOURCE_CHUNK) {
        let marks = vec!["(?, ?)"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT server_id, ip, last_seen FROM access_sources \
             JOIN (VALUES {marks}) AS wanted(server_id, ip) USING (server_id, ip)"
        );
        let values = chunk.iter().flat_map(|(server_id, ip)| [server_id, ip]);
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), |row| {
            Ok(((row.get(0)?, row.get(1)?), row.get(2)?))
        })?;
        for row in rows {
            let (key, last_seen) = row?;
            seen.insert(key, last_seen);
        }
    }

    Ok(seen)
}

fn upsert_sources(
    conn: &Connection,
    sources: &HashMap<SourceKey, Seen>,
) -> duckdb::Result<()> {
    let sources: Vec<_> = sources.iter().collect();
    for chunk in sources.chunks(SOURCE_CHUNK) {
        let marks = vec!["(?, ?, ?, ?)"; chunk.len()].join(", ");
        let sql = format!(
            "INSERT INTO access_sources (server_id, ip, first_seen, last_seen) VALUES {marks} \
             ON CONFLICT (server_id, ip) DO UPDATE SET \
             first_seen = least(access_sources.first_seen, excluded.first_seen), \
             last_seen = greatest(access_sources.last_seen, excluded.last_seen)"
        );
        let values = chunk.iter().flat_map(|((server_id, ip), seen)| {
            [
                SqlValue::Text(server_id.clone()),
                SqlValue::Text(ip.clone()),
                SqlValue::BigInt(seen.first),
                SqlValue::BigInt(seen.last),
            ]
        });
        conn.execute(&sql, params_from_iter(values))?;
    }

    Ok(())
}

pub(crate) fn remember_logins(
    conn: &Connection,
    rows: &mut [LogRow],
) -> duckdb::Result<()> {
    let mut logins: Vec<(usize, SourceKey, Map<String, Value>)> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let (ip, attributes) = read_success(row)?;
            Some((index, (row.server_id.clone(), ip), attributes))
        })
        .collect();
    if logins.is_empty() {
        return Ok(());
    }

    logins.sort_by_key(|(index, _, _)| rows[*index].at);
    let stored = read_last_seen(conn, logins.iter().map(|(_, key, _)| key))?;
    let mut sources: HashMap<SourceKey, Seen> = HashMap::new();

    for (index, key, mut attributes) in logins {
        let row = &mut rows[index];
        let previous = stored
            .get(&key)
            .copied()
            .into_iter()
            .chain(sources.get(&key).map(|seen| seen.last))
            .max();
        if let Some(previous) = previous {
            attributes.insert(LAST_SEEN.into(), Value::from(previous));
            row.attributes = Some(Value::Object(attributes).to_string());
        }

        let at = row.at;
        sources
            .entry(key)
            .and_modify(|seen| seen.add(at))
            .or_insert(Seen { first: at, last: at });
    }

    upsert_sources(conn, &sources)
}

fn access_filter(action: &str, outcome: &str) -> (String, Vec<SqlValue>) {
    let marks = vec!["?"; SSH_PROCESSES.len()].join(", ");
    let sql = format!(
        "service = ? AND stream = ? \
         AND json_extract_string(attributes, '$.\"process.name\"') IN ({marks}) \
         AND json_extract_string(attributes, '$.\"event.action\"') = ? \
         AND json_extract_string(attributes, '$.\"event.outcome\"') = ?"
    );
    let values = [ACCESS_SERVICE, ACCESS_STREAM]
        .into_iter()
        .chain(SSH_PROCESSES)
        .chain([action, outcome])
        .map(|value| SqlValue::Text(value.to_string()))
        .collect();

    (sql, values)
}

pub(crate) async fn list_logins(
    analytics: &Analytics,
    since: Since,
) -> Result<Vec<Login>> {
    analytics
        .read(move |conn| {
            let (column, at) = since.column();
            let (filter, mut values) = access_filter("ssh_login", "success");
            let sql = format!(
                "SELECT server_id, logged_at, \
                 coalesce(json_extract_string(attributes, '$.\"user\"'), ''), \
                 client_ip, \
                 json_extract_string(attributes, '$.\"ssh.method\"'), \
                 TRY_CAST(json_extract_string(attributes, '$.\"{LAST_SEEN}\"') AS BIGINT) \
                 FROM log_lines WHERE {column} >= ? AND {filter} \
                 AND client_ip IS NOT NULL ORDER BY logged_at"
            );
            values.insert(0, SqlValue::BigInt(at));

            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok(Login {
                        server_id: row.get(0)?,
                        at: row.get(1)?,
                        user: row.get(2)?,
                        ip: row.get(3)?,
                        method: row.get(4)?,
                        last_seen: row.get(5)?,
                    })
                })?
                .collect()
        })
        .await
}

pub(crate) async fn list_failures(
    analytics: &Analytics,
    since: i64,
    ips: Vec<String>,
) -> Result<Vec<Failure>> {
    if ips.is_empty() {
        return Ok(Vec::new());
    }

    analytics
        .read(move |conn| {
            let (filter, mut values) = access_filter("ssh_login", "failure");
            let marks = vec!["?"; ips.len()].join(", ");
            let sql = format!(
                "SELECT server_id, logged_at, \
                 TRY_CAST(json_extract_string(attributes, '$.\"process.pid\"') AS BIGINT), \
                 client_ip, \
                 TRY_CAST(json_extract_string(attributes, '$.\"client.port\"') AS BIGINT), \
                 json_extract_string(attributes, '$.\"ssh.method\"') \
                 FROM log_lines WHERE logged_at >= ? AND {filter} \
                 AND client_ip IN ({marks})"
            );
            values.insert(0, SqlValue::BigInt(since));
            values.extend(ips.into_iter().map(SqlValue::Text));

            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values), |row| {
                    Ok(Failure {
                        server_id: row.get(0)?,
                        at: row.get(1)?,
                        pid: row.get(2)?,
                        ip: row.get(3)?,
                        port: row.get(4)?,
                        kind: FailureKind::from(row.get::<_, Option<String>>(5)?),
                    })
                })?
                .collect()
        })
        .await
}

pub(crate) async fn list_first_logins(
    analytics: &Analytics,
) -> Result<HashMap<String, i64>> {
    analytics
        .read(|conn| {
            conn.prepare(
                "SELECT server_id, min(first_seen) FROM access_sources GROUP BY server_id",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
        })
        .await
}

#[derive(Serialize, ToSchema, Debug, Clone, PartialEq)]
pub struct AccessEvent {
    pub at: i64,
    pub action: String,
    pub outcome: String,
    pub user: Option<String>,
    pub ip: Option<String>,
    pub method: Option<String>,
    pub fingerprint: Option<String>,
    pub target: Option<String>,
    pub group: Option<String>,
    pub command: Option<String>,
    pub tty: Option<String>,
    pub ip_last_seen: Option<i64>,
}

#[derive(Serialize, ToSchema, Debug, Clone, PartialEq)]
pub struct AccessView {
    pub events: Vec<AccessEvent>,
    pub failed_logins: u64,
}

pub async fn read_access(
    analytics: &Analytics,
    server_id: String,
    now: i64,
    limit: u32,
) -> Result<AccessView> {
    analytics
        .read(move |conn| {
            let field = |key: &str| format!("json_extract_string(attributes, '$.\"{key}\"')");
            let action = field("event.action");
            let outcome = field("event.outcome");
            let base = "FROM log_lines WHERE server_id = ? AND service = ? AND stream = ?";
            let sql = format!(
                "SELECT logged_at, {action}, coalesce({outcome}, 'success'), {}, client_ip, {}, {}, \
                 coalesce({}, {}, {}), {}, {}, coalesce({}, {}), TRY_CAST({} AS BIGINT) \
                 FROM (SELECT logged_at, client_ip, attributes {base} AND {action} IS NOT NULL \
                 AND NOT ({action} = 'ssh_login' AND {outcome} = 'failure') \
                 ORDER BY logged_at DESC LIMIT ?) ORDER BY logged_at DESC",
                field("user"),
                field("ssh.method"),
                field("ssh.key.fingerprint"),
                field("sudo.target_user"),
                field("su.target_user"),
                field("user.target.name"),
                field("group.name"),
                field("process.command_line"),
                field("login.tty"),
                field("su.tty"),
                field(LAST_SEEN),
            );
            let events = conn
                .prepare(&sql)?
                .query_map(
                    params![server_id, ACCESS_SERVICE, ACCESS_STREAM, limit],
                    |row| {
                        Ok(AccessEvent {
                            at: row.get(0)?,
                            action: row.get(1)?,
                            outcome: row.get(2)?,
                            user: row.get(3)?,
                            ip: row.get(4)?,
                            method: row.get(5)?,
                            fingerprint: row.get(6)?,
                            target: row.get(7)?,
                            group: row.get(8)?,
                            command: row.get(9)?,
                            tty: row.get(10)?,
                            ip_last_seen: row.get(11)?,
                        })
                    },
                )?
                .collect::<duckdb::Result<Vec<_>>>()?;

            let failed_logins: i64 = conn.query_row(
                &format!(
                    "SELECT CAST(coalesce(sum(CASE WHEN attempts > 0 THEN attempts \
                     ELSE least(others, 1) END), 0) AS BIGINT) FROM ( \
                     SELECT count(*) FILTER (WHERE method <> 'none') AS attempts, \
                     count(*) FILTER (WHERE method IS NULL) AS others FROM ( \
                     SELECT rowid, client_ip, {method} AS method, {pid} AS pid, {port} AS port \
                     {base} AND logged_at >= ? AND {action} = 'ssh_login' \
                     AND {outcome} = 'failure') \
                     GROUP BY pid, client_ip, port, CASE WHEN port IS NULL THEN rowid END)",
                    method = field("ssh.method"),
                    pid = field("process.pid"),
                    port = field("client.port"),
                ),
                params![server_id, ACCESS_SERVICE, ACCESS_STREAM, now - DAY_MS],
                |row| row.get(0),
            )?;

let failed_logins = u64::try_from(failed_logins).unwrap_or_default();
            Ok(AccessView { events, failed_logins })
        })
        .await
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::log::LogBatch;
    use serde_json::json;

    use super::{
        FailureKind, LAST_SEEN, Since, list_failures, list_first_logins, list_logins,
        read_access,
    };
    use crate::analytics::Analytics;
    use crate::analytics::archive::create_log_tables;

    fn batch(lines: &[(u64, &str, bool, &str)]) -> LogBatch {
        let lines = lines
            .iter()
            .map(|(at, service, access, message)| {
                serde_json::from_value(json!({
                    "at": at,
                    "service": service,
                    "stream": "journal",
                    "access": access,
                    "severity": 9,
                    "message": message,
                    "format": "system",
                }))
                .unwrap()
            })
            .collect();

        LogBatch { lines, ..LogBatch::default() }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn remembers_the_last_login_from_each_address() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-access-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("access.duckdb")).unwrap();
        analytics
            .run(|conn| {
                create_log_tables(conn)?;
                conn.execute_batch(include_str!(
                    "../../../daroyan-migration/duckdb/0015_metrics.sql"
                ))?;
                conn.execute_batch(include_str!(
                    "../../../daroyan-migration/duckdb/0016_access_sources.sql"
                ))?;
                conn.execute_batch(
                    "CREATE TABLE server_states (server_id VARCHAR); \
                     CREATE TABLE disk_states (server_id VARCHAR); \
                     CREATE TABLE server_events (server_id VARCHAR); \
                     CREATE TABLE spans (server_id VARCHAR);",
                )
            })
            .unwrap();
        let accepted =
            "sshd[9]: Accepted publickey for arik from 203.0.113.7 port 5 ssh2";

        analytics.buffer_logs(
            "srv_1",
            batch(&[
                (1_000, "access", true, accepted),
                (2_000, "access", true, accepted),
                (2_500, "access", true, "sshd[9]: Failed password for root from 45.155.205.233 port 7 ssh2"),
                (2_600, "access", true, "sshd[9]: Connection closed by authenticating user root 45.155.205.233 port 7 [preauth]"),
                (3_000, "app", false, "sshd[9]: Accepted password for root from 6.6.6.6 port 1 ssh2"),
                (3_000, "access", false, "sshd[9]: Accepted password for root from 6.6.6.7 port 1 ssh2"),
            ]),
        );
        analytics.flush().await.unwrap();
        analytics.buffer_logs("srv_1", batch(&[(9_000, "access", true, accepted)]));
        analytics.buffer_logs("srv_2", batch(&[(9_500, "access", true, accepted)]));
        analytics.flush().await.unwrap();

        let logins = list_logins(&analytics, Since::Logged(0)).await.unwrap();
        let seen: Vec<(&str, i64, Option<i64>)> = logins
            .iter()
            .map(|login| (login.server_id.as_str(), login.at, login.last_seen))
            .collect();
        assert_eq!(
            seen,
            [
                ("srv_1", 1_000, None),
                ("srv_1", 2_000, Some(1_000)),
                ("srv_1", 9_000, Some(2_000)),
                ("srv_2", 9_500, None),
            ]
        );
        assert_eq!(logins[0].user, "arik");
        assert_eq!(logins[0].method.as_deref(), Some("publickey"));

        let failures = list_failures(&analytics, 0, vec!["45.155.205.233".to_string()])
            .await
            .unwrap();
        assert_eq!(failures.len(), 2);
        assert_eq!(
            failures
                .iter()
                .filter(|failure| failure.kind == FailureKind::Attempt)
                .count(),
            1
        );
        assert!(failures.iter().all(|failure| failure.port == Some(7)));

        let attributes: String = analytics
            .read(|conn| {
                conn.query_row(
                    "SELECT attributes FROM log_lines WHERE logged_at = 2000",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert!(attributes.contains(&format!("\"{LAST_SEEN}\":1000")));

        let first = list_first_logins(&analytics).await.unwrap();
        assert_eq!(first["srv_1"], 1_000);
        assert_eq!(first["srv_2"], 9_500);

        let view = read_access(&analytics, "srv_1".to_string(), 10_000, 50)
            .await
            .unwrap();
        assert_eq!(view.failed_logins, 1);
        let shown: Vec<(i64, &str, Option<i64>)> = view
            .events
            .iter()
            .map(|event| (event.at, event.action.as_str(), event.ip_last_seen))
            .collect();
        assert_eq!(
            shown,
            [
                (9_000, "ssh_login", Some(2_000)),
                (2_000, "ssh_login", Some(1_000)),
                (1_000, "ssh_login", None)
            ]
        );
        assert_eq!(view.events[0].fingerprint, None);
        assert_eq!(view.events[0].method.as_deref(), Some("publickey"));

        analytics.delete_server_rows("srv_1".to_string()).await.unwrap();
        let left: i64 = analytics
            .read(|conn| {
                conn.query_row("SELECT count(*) FROM access_sources", [], |row| {
                    row.get(0)
                })
            })
            .await
            .unwrap();
        assert_eq!(left, 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
