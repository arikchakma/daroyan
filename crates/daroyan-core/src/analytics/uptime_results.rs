use std::collections::HashMap;

use duckdb::{OptionalExt, params};
use serde::Serialize;
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::error::Result;
use crate::uptime::probe::Probe;

pub const DAY_MS: i64 = 86_400_000;

#[derive(Serialize, ToSchema, Clone, Debug, Default)]
#[schema(as = UptimeDay)]
pub struct DayBar {
    pub day: i64,
    pub total: u32,
    pub down: u32,
    pub slow: u32,
}

#[derive(Clone, Debug, Default)]
pub struct CheckSummary {
    pub status: Option<String>,
    pub response_ms: Option<u32>,
    pub checked_at: Option<i64>,
    pub uptime: Option<f64>,
    pub days: Vec<DayBar>,
}

#[derive(Serialize, ToSchema, Debug)]
#[schema(as = UptimeResult)]
pub struct ResultItem {
    pub at: i64,
    pub status: String,
    pub code: Option<u16>,
    pub response_ms: Option<u32>,
    pub error: Option<String>,
    pub dns_ms: Option<u32>,
    pub connect_ms: Option<u32>,
    pub tls_ms: Option<u32>,
    pub ttfb_ms: Option<u32>,
}

impl From<Probe> for ResultItem {
    fn from(probe: Probe) -> Self {
        Self {
            at: probe.at.timestamp_millis(),
            status: probe.status.to_string(),
            code: probe.code,
            response_ms: probe.response_ms,
            error: probe.error,
            dns_ms: probe.timings.dns_ms,
            connect_ms: probe.timings.connect_ms,
            tls_ms: probe.timings.tls_ms,
            ttfb_ms: probe.timings.ttfb_ms,
        }
    }
}

#[derive(Serialize, ToSchema, Debug)]
#[schema(as = UptimePoint)]
pub struct ResponsePoint {
    pub at: i64,
    pub response_ms: Option<f64>,
    pub dns_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    pub total: u32,
    pub down: u32,
}

#[derive(Serialize, ToSchema, Debug, Default)]
#[schema(as = UptimeStats)]
pub struct CheckStats {
    pub uptime: Option<f64>,
    pub average_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub checks_today: u32,
    pub cert_expires_at: Option<i64>,
}

pub async fn append_result(
    analytics: &Analytics,
    check_id: String,
    probe: Probe,
) -> Result<()> {
    analytics
        .write(move |conn| {
            conn.execute(
                "INSERT INTO uptime_results (check_id, checked_at, status, code, response_ms, error, \
                 cert_expires_at, dns_ms, connect_ms, tls_ms, ttfb_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    check_id,
                    probe.at.timestamp_millis(),
                    probe.status,
                    probe.code,
                    probe.response_ms,
                    probe.error,
                    probe.cert_expires_at.map(|at| at.timestamp_millis()),
                    probe.timings.dns_ms,
                    probe.timings.connect_ms,
                    probe.timings.tls_ms,
                    probe.timings.ttfb_ms,
                ],
            )
            .map(|_| ())
        })
        .await
}

pub async fn count_recent(
    analytics: &Analytics,
    check_id: String,
    status: &'static str,
    last: u32,
) -> Result<u32> {
    analytics
        .read(move |conn| {
            conn.query_row(
                "SELECT count(*) FILTER (WHERE status = ?) FROM \
                 (SELECT status FROM uptime_results WHERE check_id = ? ORDER BY checked_at DESC LIMIT ?)",
                params![status, check_id, last],
                |row| row.get(0),
            )
        })
        .await
}

pub struct WindowSource {
    pub table: &'static str,
    pub id_column: &'static str,
    pub time_column: &'static str,
    pub bad: &'static str,
}

pub const CHECK_WINDOWS: WindowSource = WindowSource {
    table: "uptime_results",
    id_column: "check_id",
    time_column: "checked_at",
    bad: "status = 'down'",
};

pub const CRON_WINDOWS: WindowSource = WindowSource {
    table: "cron_runs",
    id_column: "cron_id",
    time_column: "finished_at",
    bad: "status IN ('failed', 'missed')",
};

pub async fn count_in_windows(
    analytics: &Analytics,
    source: &'static WindowSource,
    id: String,
    windows: Vec<(i64, i64)>,
) -> Result<HashMap<i64, DayBar>> {
    if windows.is_empty() {
        return Ok(HashMap::new());
    }
    let time = source.time_column;
    let ranges = windows
        .iter()
        .map(|(start, end)| format!("({time} >= {start} AND {time} < {end})"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        "SELECT {time} // 86400000 * 86400000 AS day, count(*), count(*) FILTER (WHERE {}) \
         FROM {} WHERE {} = ? AND ({ranges}) GROUP BY ALL",
        source.bad, source.table, source.id_column
    );
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(&sql)?;
            let rows = statement.query_map(params![id], |row| {
                Ok(DayBar {
                    day: row.get(0)?,
                    total: row.get(1)?,
                    down: row.get(2)?,
                    slow: 0,
                })
            })?;
            let mut by_day = HashMap::new();
            for row in rows {
                let bar = row?;
                by_day.insert(bar.day, bar);
            }
            Ok(by_day)
        })
        .await
}

pub fn fill_days(rows: Vec<DayBar>, since: i64, count: i64) -> Vec<DayBar> {
    let by_day: HashMap<i64, DayBar> =
        rows.into_iter().map(|bar| (bar.day, bar)).collect();
    (0..count)
        .map(|index| {
            let day = since + index * DAY_MS;
            by_day
                .get(&day)
                .cloned()
                .unwrap_or(DayBar { day, ..Default::default() })
        })
        .collect()
}

pub fn to_day_start(now_ms: i64, days: i64) -> i64 {
    (now_ms / DAY_MS - days + 1) * DAY_MS
}

pub async fn summarize_checks(
    analytics: &Analytics,
    now_ms: i64,
    days: i64,
) -> Result<HashMap<String, CheckSummary>> {
    let since = to_day_start(now_ms, days);
    analytics
        .read(move |conn| {
            let mut summaries: HashMap<String, CheckSummary> = HashMap::new();
            let mut latest = conn.prepare(
                "SELECT check_id, arg_max(status, checked_at), arg_max(response_ms, checked_at), max(checked_at) \
                 FROM uptime_results GROUP BY check_id",
            )?;
            for row in latest.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })? {
                let (id, status, response_ms, checked_at) = row?;
                let summary = summaries.entry(id).or_default();
                summary.status = Some(status);
                summary.response_ms = response_ms;
                summary.checked_at = Some(checked_at);
            }
            let mut bars = conn.prepare(
                "SELECT check_id, checked_at // 86400000 * 86400000 AS day, count(*), \
                 count(*) FILTER (WHERE status = 'down'), count(*) FILTER (WHERE status = 'degraded') \
                 FROM uptime_results WHERE checked_at >= ? GROUP BY ALL",
            )?;
            let mut by_check: HashMap<String, Vec<DayBar>> = HashMap::new();
            for row in bars.query_map([since], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    DayBar { day: row.get(1)?, total: row.get(2)?, down: row.get(3)?, slow: row.get(4)? },
                ))
            })? {
                let (id, bar) = row?;
                by_check.entry(id).or_default().push(bar);
            }
            for (id, rows) in by_check {
                let total: u32 = rows.iter().map(|bar| bar.total).sum();
                let down: u32 = rows.iter().map(|bar| bar.down).sum();
                let summary = summaries.entry(id).or_default();
                summary.uptime = (total > 0).then(|| f64::from(total - down) / f64::from(total));
                summary.days = fill_days(rows, since, days);
            }
            Ok(summaries)
        })
        .await
}

pub async fn read_check_stats(
    analytics: &Analytics,
    check_id: String,
    now_ms: i64,
    days: i64,
) -> Result<(CheckStats, Vec<DayBar>)> {
    let since = to_day_start(now_ms, days);
    let month = now_ms - 30 * DAY_MS;
    analytics
        .read(move |conn| {
            let mut stats = conn.query_row(
                "SELECT count(*), count(*) FILTER (WHERE status <> 'down'), avg(response_ms), \
                 quantile_cont(response_ms, 0.95), count(*) FILTER (WHERE checked_at >= ?) \
                 FROM uptime_results WHERE check_id = ? AND checked_at >= ?",
                params![now_ms - DAY_MS, check_id, month],
                |row| {
                    let total: u32 = row.get(0)?;
                    let up: u32 = row.get(1)?;
                    Ok(CheckStats {
                        uptime: (total > 0).then(|| f64::from(up) / f64::from(total)),
                        average_ms: row.get(2)?,
                        p95_ms: row.get(3)?,
                        checks_today: row.get(4)?,
                        cert_expires_at: None,
                    })
                },
            )?;
            stats.cert_expires_at = conn
                .query_row(
                    "SELECT cert_expires_at FROM uptime_results WHERE check_id = ? \
                     AND cert_expires_at IS NOT NULL ORDER BY checked_at DESC LIMIT 1",
                    [&check_id],
                    |row| row.get(0),
                )
                .optional()?;
            let mut bars = conn.prepare(
                "SELECT checked_at // 86400000 * 86400000 AS day, count(*), \
                 count(*) FILTER (WHERE status = 'down'), count(*) FILTER (WHERE status = 'degraded') \
                 FROM uptime_results WHERE check_id = ? AND checked_at >= ? GROUP BY ALL",
            )?;
            let rows = bars
                .query_map(params![check_id, since], |row| {
                    Ok(DayBar { day: row.get(0)?, total: row.get(1)?, down: row.get(2)?, slow: row.get(3)? })
                })?
                .collect::<duckdb::Result<Vec<_>>>()?;
            Ok((stats, fill_days(rows, since, days)))
        })
        .await
}

pub async fn list_response_points(
    analytics: &Analytics,
    check_id: String,
    since: i64,
    bucket_ms: i64,
) -> Result<Vec<ResponsePoint>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT checked_at // ? * ? AS bucket, avg(response_ms), avg(dns_ms), avg(tls_ms), count(*), \
                 count(*) FILTER (WHERE status = 'down') \
                 FROM uptime_results WHERE check_id = ? AND checked_at >= ? GROUP BY ALL ORDER BY bucket",
            )?;
            statement
                .query_map(params![bucket_ms, bucket_ms, check_id, since], |row| {
                    Ok(ResponsePoint {
                        at: row.get(0)?,
                        response_ms: row.get(1)?,
                        dns_ms: row.get(2)?,
                        tls_ms: row.get(3)?,
                        total: row.get(4)?,
                        down: row.get(5)?,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn list_recent_results(
    analytics: &Analytics,
    check_id: String,
    limit: u32,
) -> Result<Vec<ResultItem>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT checked_at, status, code, response_ms, error, dns_ms, connect_ms, tls_ms, ttfb_ms \
                 FROM uptime_results \
                 WHERE check_id = ? ORDER BY checked_at DESC LIMIT ?",
            )?;
            statement
                .query_map(params![check_id, limit], |row| {
                    Ok(ResultItem {
                        at: row.get(0)?,
                        status: row.get(1)?,
                        code: row.get(2)?,
                        response_ms: row.get(3)?,
                        error: row.get(4)?,
                        dns_ms: row.get(5)?,
                        connect_ms: row.get(6)?,
                        tls_ms: row.get(7)?,
                        ttfb_ms: row.get(8)?,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn delete_check_results(analytics: &Analytics, check_id: String) -> Result<()> {
    analytics
        .write(move |conn| {
            conn.execute("DELETE FROM uptime_results WHERE check_id = ?", [check_id])
                .map(|_| ())
        })
        .await
}

pub async fn aggregate_check_metric(
    analytics: &Analytics,
    expression: String,
    column: &'static str,
    since: Option<i64>,
) -> Result<HashMap<String, f64>> {
    analytics
        .read(move |conn| {
            let sql = match since {
                Some(_) => format!(
                    "SELECT check_id, {expression} FROM uptime_results \
                     WHERE checked_at >= ? AND {column} IS NOT NULL GROUP BY check_id"
                ),
                None => format!(
                    "SELECT check_id, arg_max({column}, checked_at) FROM uptime_results \
                     WHERE checked_at >= ? AND {column} IS NOT NULL GROUP BY check_id"
                ),
            };
            let mut statement = conn.prepare(&sql)?;
            statement
                .query_map([since.unwrap_or(0)], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
                })?
                .filter_map(|row| match row {
                    Ok((id, Some(value))) => Some(Ok((id, value))),
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect()
        })
        .await
}

pub async fn list_cert_expiries(analytics: &Analytics) -> Result<HashMap<String, i64>> {
    analytics
        .read(|conn| {
            let mut statement = conn.prepare(
                "SELECT check_id, arg_max(cert_expires_at, checked_at) FROM uptime_results \
                 WHERE cert_expires_at IS NOT NULL GROUP BY check_id",
            )?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })
        .await
}
