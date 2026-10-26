use std::collections::HashMap;

use duckdb::params;
use serde::Serialize;
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::analytics::uptime_results::{DayBar, fill_days, to_day_start};
use crate::error::Result;

pub const DAY_MS: i64 = 86_400_000;
const LIST_RUNS: u32 = 12;

#[derive(Serialize, ToSchema, Clone, Debug, PartialEq)]
pub struct CronRun {
    pub scheduled_at: Option<i64>,
    pub started_at: Option<i64>,
    pub finished_at: i64,
    #[schema(example = "ok")]
    pub status: String,
    pub exit_code: Option<u8>,
    pub duration_ms: Option<u32>,
    pub output: Option<String>,
}

#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct CronRunBar {
    pub at: i64,
    #[schema(example = "ok")]
    pub status: String,
}

#[derive(Clone, Debug, Default)]
pub struct CronSummary {
    pub bars: Vec<CronRunBar>,
    pub last_duration_ms: Option<u32>,
}

#[derive(Serialize, ToSchema, Debug, Default)]
pub struct CronStats {
    pub success_rate: Option<f64>,
    pub average_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub runs_today: u32,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct CronPoint {
    pub at: i64,
    pub duration_ms: Option<f64>,
    pub delay_ms: Option<f64>,
    pub total: u32,
    pub ok: u32,
}

pub async fn append_runs(
    analytics: &Analytics,
    cron_id: String,
    runs: Vec<CronRun>,
) -> Result<()> {
    analytics
        .write(move |conn| {
            let mut appender = conn.appender("cron_runs")?;
            for run in runs {
                appender.append_row(params![
                    cron_id,
                    run.scheduled_at,
                    run.started_at,
                    run.finished_at,
                    run.status,
                    run.exit_code,
                    run.duration_ms,
                    run.output,
                ])?;
            }
            appender.flush()
        })
        .await
}

pub async fn summarize_crons(
    analytics: &Analytics,
) -> Result<HashMap<String, CronSummary>> {
    analytics
        .read(|conn| {
            let mut statement = conn.prepare(
                "SELECT cron_id, finished_at, status, duration_ms FROM \
                 (SELECT *, row_number() OVER (PARTITION BY cron_id ORDER BY finished_at DESC) AS position \
                 FROM cron_runs) WHERE position <= ? ORDER BY cron_id, finished_at",
            )?;
            let mut summaries: HashMap<String, CronSummary> = HashMap::new();
            for row in statement.query_map([LIST_RUNS], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    CronRunBar { at: row.get(1)?, status: row.get(2)? },
                    row.get::<_, Option<u32>>(3)?,
                ))
            })? {
                let (id, bar, duration_ms) = row?;
                let summary = summaries.entry(id).or_default();
                if bar.status != "missed" {
                    summary.last_duration_ms = duration_ms;
                }
                summary.bars.push(bar);
            }
            Ok(summaries)
        })
        .await
}

pub async fn read_cron_stats(
    analytics: &Analytics,
    cron_id: String,
    now_ms: i64,
    bars: u32,
) -> Result<(CronStats, Vec<CronRunBar>)> {
    analytics
        .read(move |conn| {
            let stats = conn.query_row(
                "SELECT count(*), count(*) FILTER (WHERE status IN ('ok', 'late')), avg(duration_ms), \
                 quantile_cont(duration_ms, 0.95), count(*) FILTER (WHERE finished_at >= ?) \
                 FROM cron_runs WHERE cron_id = ? AND finished_at >= ?",
                params![now_ms - DAY_MS, cron_id, now_ms - 30 * DAY_MS],
                |row| {
                    let total: u32 = row.get(0)?;
                    let ok: u32 = row.get(1)?;
                    Ok(CronStats {
                        success_rate: (total > 0).then(|| f64::from(ok) / f64::from(total)),
                        average_ms: row.get(2)?,
                        p95_ms: row.get(3)?,
                        runs_today: row.get(4)?,
                    })
                },
            )?;
            let mut statement = conn.prepare(
                "SELECT finished_at, status FROM cron_runs WHERE cron_id = ? \
                 ORDER BY finished_at DESC LIMIT ?",
            )?;
            let mut recent = statement
                .query_map(params![cron_id, bars], |row| {
                    Ok(CronRunBar { at: row.get(0)?, status: row.get(1)? })
                })?
                .collect::<duckdb::Result<Vec<_>>>()?;
            recent.reverse();
            Ok((stats, recent))
        })
        .await
}

pub async fn list_cron_points(
    analytics: &Analytics,
    cron_id: String,
    since: i64,
    bucket_ms: i64,
) -> Result<Vec<CronPoint>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT finished_at // ? * ? AS bucket, avg(duration_ms), \
                 avg(coalesce(started_at, finished_at) - scheduled_at) FILTER (WHERE status <> 'missed'), \
                 count(*), count(*) FILTER (WHERE status IN ('ok', 'late')) \
                 FROM cron_runs WHERE cron_id = ? AND finished_at >= ? GROUP BY ALL ORDER BY bucket",
            )?;
            statement
                .query_map(params![bucket_ms, bucket_ms, cron_id, since], |row| {
                    Ok(CronPoint {
                        at: row.get(0)?,
                        duration_ms: row.get(1)?,
                        delay_ms: row.get(2)?,
                        total: row.get(3)?,
                        ok: row.get(4)?,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn list_recent_runs(
    analytics: &Analytics,
    cron_id: String,
    limit: u32,
) -> Result<Vec<CronRun>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT scheduled_at, started_at, finished_at, status, exit_code, duration_ms, output \
                 FROM cron_runs WHERE cron_id = ? ORDER BY finished_at DESC LIMIT ?",
            )?;
            statement
                .query_map(params![cron_id, limit], |row| {
                    Ok(CronRun {
                        scheduled_at: row.get(0)?,
                        started_at: row.get(1)?,
                        finished_at: row.get(2)?,
                        status: row.get(3)?,
                        exit_code: row.get(4)?,
                        duration_ms: row.get(5)?,
                        output: row.get(6)?,
                    })
                })?
                .collect()
        })
        .await
}

pub async fn delete_cron_runs(analytics: &Analytics, cron_id: String) -> Result<()> {
    analytics
        .write(move |conn| {
            conn.execute("DELETE FROM cron_runs WHERE cron_id = ?", [cron_id])
                .map(|_| ())
        })
        .await
}

pub async fn summarize_cron_days(
    analytics: &Analytics,
    now_ms: i64,
    days: i64,
) -> Result<HashMap<String, Vec<DayBar>>> {
    let since = to_day_start(now_ms, days);
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT cron_id, finished_at // 86400000 * 86400000 AS day, count(*), \
                 count(*) FILTER (WHERE status IN ('failed', 'missed')), count(*) FILTER (WHERE status = 'late') \
                 FROM cron_runs WHERE finished_at >= ? GROUP BY ALL",
            )?;
            let mut by_cron: HashMap<String, Vec<DayBar>> = HashMap::new();
            for row in statement.query_map([since], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    DayBar { day: row.get(1)?, total: row.get(2)?, down: row.get(3)?, slow: row.get(4)? },
                ))
            })? {
                let (id, bar) = row?;
                by_cron.entry(id).or_default().push(bar);
            }
            Ok(by_cron
                .into_iter()
                .map(|(id, rows)| (id, fill_days(rows, since, days)))
                .collect())
        })
        .await
}
