pub mod access;
pub mod archive;
pub mod cron_runs;
pub mod log_lines;
pub mod log_parse;
pub mod metrics;
pub mod server_events;
pub mod server_states;
pub mod spans;
pub mod uptime_results;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::{DateTime, Utc};
use daroyan_protocol::log::LogBatch;
use daroyan_protocol::report::Report;
use duckdb::Connection;
use tokio::task::{JoinHandle, spawn_blocking};

use crate::analytics::archive::IS_ARCHIVED;
use crate::analytics::log_lines::{
    LogRow, OTLP_SERVER_ID, append_logs, append_volume, to_log_rows,
};
use crate::analytics::metrics::ingest::{MetricSample, SeriesRow};
use crate::analytics::metrics::series::{
    Admission, IDLE_MS, PointRow, SeriesCache, SeriesLimits,
};
use crate::analytics::metrics::store::{
    MetricPurge, append_metrics, list_known_series, purge_metrics, roll_up_late_hours,
    roll_up_metrics, to_late_hours,
};
use crate::analytics::server_events::{
    EventRow, append_events, to_event_row, to_event_rows,
};
use crate::analytics::server_states::{
    DiskRow, StateRow, append_disks, append_states, to_disk_rows,
};
use crate::analytics::spans::{ExceptionRow, SpanRow, append_spans};
use crate::config::Config;
use crate::error::{Error, ErrorName, Result};

const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const THREAD_SETTINGS: &str = "SET threads = 2;";
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);
const UPTIME_RETENTION_DAYS: i64 = 90;
const MAX_PENDING_ROWS: usize = 200_000;
const MAX_SERVER_ROWS: usize = MAX_PENDING_ROWS / 10;
const MAX_REPORT_EVENTS: usize = 1_000;
const KEPT_BACKUPS: usize = 8;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const SLOW_READ: Duration = Duration::from_millis(200);
const TRIM_INTERVAL: Duration = Duration::from_secs(5 * 60);
const TRIM_TARGET_PERCENT: u64 = 90;
const TRIM_KEPT_MS: i64 = 60 * 60 * 1000;

#[derive(Clone)]
pub struct Analytics {
    conn: Arc<Mutex<Connection>>,
    reader: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<Pending>>,
    metric_series: Arc<Mutex<SeriesCache>>,
    late_metric_hours: Arc<Mutex<BTreeSet<i64>>>,
    last_backup: Arc<Mutex<Option<Backup>>>,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trim {
    pub used_bytes: u64,
    pub deleted_rows: usize,
    pub logs_until: Option<i64>,
    pub spans_until: Option<i64>,
    pub unarchived_rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    pub days: u32,
    pub log_days: u32,
    pub archive_days: Option<u32>,
    pub metric_days: u32,
    pub metric_rollup_days: u32,
}

impl From<&Config> for Retention {
    fn from(config: &Config) -> Self {
        Self {
            days: config.retention_days,
            log_days: config.log_retention_days,
            archive_days: config.archive.as_ref().map(|archive| archive.retention_days),
            metric_days: config.metrics.retention_days,
            metric_rollup_days: config.metrics.rollup_retention_days,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Purge {
    pub now_ms: i64,
    pub cutoff_ms: i64,
    pub log_cutoff_ms: i64,
    pub volume_cutoff_ms: i64,
    pub point_cutoff_ms: i64,
    pub rollup_cutoff_ms: i64,
    pub keep_unarchived: bool,
}

impl Purge {
    pub fn at(now_ms: i64, retention: Retention) -> Self {
        let before = |days: u32| now_ms - i64::from(days) * DAY_MS;

        Self {
            now_ms,
            cutoff_ms: before(retention.days),
            log_cutoff_ms: before(retention.log_days),
            volume_cutoff_ms: before(
                retention.archive_days.unwrap_or(retention.log_days),
            ),
            point_cutoff_ms: before(retention.metric_days),
            rollup_cutoff_ms: before(retention.metric_rollup_days),
            keep_unarchived: retention.archive_days.is_some(),
        }
    }
}

#[derive(Default)]
struct ServerShare {
    logs: usize,
    report: usize,
}

#[derive(Default)]
struct Pending {
    states: Vec<StateRow>,
    disks: Vec<DiskRow>,
    events: Vec<EventRow>,
    logs: Vec<LogRow>,
    spans: Vec<SpanRow>,
    exceptions: Vec<ExceptionRow>,
    metric_series: Vec<SeriesRow>,
    metric_points: Vec<PointRow>,
    shares: HashMap<String, ServerShare>,
}

impl TryFrom<Connection> for Analytics {
    type Error = duckdb::Error;

    fn try_from(conn: Connection) -> duckdb::Result<Self> {
        conn.execute_batch(THREAD_SETTINGS)?;
        let reader = conn.try_clone()?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            reader: Arc::new(Mutex::new(reader)),
            pending: Arc::default(),
            metric_series: Arc::default(),
            late_metric_hours: Arc::default(),
            last_backup: Arc::default(),
        })
    }
}

impl Analytics {
    #[cfg(test)]
    pub fn open(path: &Path) -> duckdb::Result<Self> {
        Self::try_from(Connection::open(path)?)
    }

    pub fn limit_memory(&self, bytes: u64) -> duckdb::Result<()> {
        lock(&self.conn).execute_batch(&format!("SET memory_limit = '{bytes}B'"))
    }

    pub fn run<T>(
        &self,
        f: impl FnOnce(&Connection) -> duckdb::Result<T>,
    ) -> duckdb::Result<T> {
        f(&lock(&self.conn))
    }

    pub async fn read<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> duckdb::Result<T> + Send + 'static,
    {
        let reader = Arc::clone(&self.reader);
        let started = Instant::now();
        Ok(spawn_blocking(move || {
            let conn = lock(&reader).try_clone()?;
            let waited = started.elapsed();
            let result = f(&conn);
            let ran = started.elapsed() - waited;
            if waited + ran >= SLOW_READ {
                tracing::warn!(
                    waited_ms = waited.as_millis() as u64,
                    ran_ms = ran.as_millis() as u64,
                    "slow analytics read"
                );
            }
            result
        })
        .await??)
    }

    pub async fn write<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> duckdb::Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        Ok(spawn_blocking(move || f(&lock(&conn))).await??)
    }

    pub fn buffer_report(&self, server_id: &str, report: &Report) {
        let events = &report.events[..report.events.len().min(MAX_REPORT_EVENTS)];
        let disks = to_disk_rows(server_id, &report.state);
        let events = to_event_rows(server_id, events);
        let rows = 1 + disks.len() + events.len();

        let mut pending = lock(&self.pending);
        if pending.states.len() + pending.disks.len() + pending.events.len()
            >= MAX_PENDING_ROWS
        {
            tracing::warn!(server_id, "dropped a report, the write buffer is full");
            return;
        }

        let share = pending.shares.entry(server_id.to_string()).or_default();
        if share.report + rows > MAX_SERVER_ROWS {
            tracing::warn!(server_id, "dropped a report, the server used its share");
            return;
        }

        share.report += rows;
        pending.states.push(StateRow::from_state(server_id, &report.state));
        pending.disks.extend(disks);
        pending.events.extend(events);
    }

    pub fn buffer_server_event(&self, server_id: &str, kind: &str) {
        let mut pending = lock(&self.pending);
        if pending.states.len() + pending.disks.len() + pending.events.len()
            >= MAX_PENDING_ROWS
        {
            tracing::warn!(
                server_id,
                kind,
                "dropped a server event, the write buffer is full"
            );
            return;
        }
        let at = Utc::now().timestamp_millis();
        pending.events.push(to_event_row(server_id, at, kind));
    }

    pub fn buffer_logs(&self, server_id: &str, batch: LogBatch) {
        let rows = to_log_rows(server_id, batch, Utc::now().timestamp_millis());
        let mut pending = lock(&self.pending);
        if pending.logs.len() >= MAX_PENDING_ROWS {
            tracing::warn!(
                server_id,
                dropped = rows.len(),
                "dropped logs, the write buffer is full"
            );
            return;
        }

        let share = pending.shares.entry(server_id.to_string()).or_default();
        if share.logs + rows.len() > MAX_SERVER_ROWS {
            tracing::warn!(
                server_id,
                dropped = rows.len(),
                "dropped logs, the server used its share"
            );
            return;
        }

        share.logs += rows.len();
        pending.logs.extend(rows);
    }

    pub fn has_log_room(&self) -> bool {
        lock(&self.pending).logs.len() < MAX_PENDING_ROWS
    }

    pub fn buffer_log_rows(&self, rows: Vec<LogRow>) -> Result<()> {
        let mut pending = lock(&self.pending);
        if pending.logs.len() >= MAX_PENDING_ROWS {
            return Err(Error::new(
                ErrorName::RateLimitExceeded,
                "Too many log records at once. Retry in a few seconds",
            ));
        }
        pending.logs.extend(rows);
        Ok(())
    }

    pub fn buffer_spans(
        &self,
        spans: Vec<SpanRow>,
        exceptions: Vec<ExceptionRow>,
    ) -> Result<()> {
        let mut pending = lock(&self.pending);
        if pending.spans.len() >= MAX_PENDING_ROWS {
            return Err(Error::new(
                ErrorName::RateLimitExceeded,
                "Too many spans at once. Retry in a few seconds",
            ));
        }
        pending.spans.extend(spans);
        pending.exceptions.extend(exceptions);
        Ok(())
    }

    pub async fn accept_metrics(
        &self,
        samples: Vec<MetricSample>,
        limits: SeriesLimits,
    ) -> Result<Admission> {
        if lock(&self.pending).metric_points.len() >= MAX_PENDING_ROWS {
            return Err(Error::new(
                ErrorName::RateLimitExceeded,
                "Too many metric points at once. Retry in a few seconds",
            ));
        }

        let now = Utc::now().timestamp_millis();
        if !lock(&self.metric_series).is_loaded() {
            let since = now - IDLE_MS;
            let known = self.read(move |conn| list_known_series(conn, since)).await?;
            lock(&self.metric_series).load(known);
        }

        let admitted = lock(&self.metric_series).admit(samples, limits, now);
        let mut pending = lock(&self.pending);
        pending.metric_series.extend(admitted.series);
        pending.metric_points.extend(admitted.points);

        Ok(admitted.admission)
    }

    pub async fn flush(&self) -> Result<()> {
        let mut pending = std::mem::take(&mut *lock(&self.pending));
        let series = std::mem::take(&mut pending.metric_series);
        let points = std::mem::take(&mut pending.metric_points);
        let metrics = self.flush_metrics(series, points).await;

        if pending.states.is_empty()
            && pending.events.is_empty()
            && pending.logs.is_empty()
            && pending.spans.is_empty()
            && pending.exceptions.is_empty()
        {
            return metrics;
        }

        let others = self
            .write(move |conn| {
                append_states(conn, &pending.states)?;
                append_disks(conn, &pending.disks)?;
                append_events(conn, &pending.events)?;
                append_spans(conn, &pending.spans, &pending.exceptions)?;

                let transaction = conn.unchecked_transaction()?;
                access::remember_logins(conn, &mut pending.logs)?;
                append_logs(conn, &pending.logs, Utc::now().timestamp_millis())?;
                append_volume(conn, &pending.logs)?;
                transaction.commit()
            })
            .await;

        metrics.and(others)
    }

    async fn flush_metrics(
        &self,
        series: Vec<SeriesRow>,
        points: Vec<PointRow>,
    ) -> Result<()> {
        if series.is_empty() && points.is_empty() {
            return Ok(());
        }

        let series_ids: Vec<i64> = series.iter().map(|row| row.series_id).collect();
        let late_hours = to_late_hours(&points, Utc::now().timestamp_millis());
        let result = self.write(move |conn| append_metrics(conn, &series, &points)).await;
        match result {
            Ok(()) => lock(&self.late_metric_hours).extend(late_hours),
            Err(_) => lock(&self.metric_series).forget(&series_ids),
        }

        result
    }

    pub async fn purge(&self, purge: Purge) -> Result<()> {
        let Purge {
            now_ms,
            cutoff_ms,
            log_cutoff_ms,
            volume_cutoff_ms,
            point_cutoff_ms,
            rollup_cutoff_ms,
            keep_unarchived,
        } = purge;
        lock(&self.metric_series).forget_idle(now_ms);

        let pending_hours = Arc::clone(&self.late_metric_hours);
        let late_hours: BTreeSet<i64> = std::mem::take(&mut *lock(&pending_hours))
            .into_iter()
            .filter(|hour| *hour >= point_cutoff_ms)
            .collect();

        self.write(move |conn| {
            let rolled_up = roll_up_metrics(conn, now_ms)
                .and_then(|()| roll_up_late_hours(conn, now_ms, &late_hours));
            if let Err(error) = &rolled_up {
                tracing::error!(%error, "roll up metrics");
                lock(&pending_hours).extend(late_hours);
            }

            let metric_purge = MetricPurge {
                point_cutoff: point_cutoff_ms,
                rollup_cutoff: rollup_cutoff_ms,
                keep_points: rolled_up.is_err(),
            };
            if let Err(error) = purge_metrics(conn, metric_purge) {
                tracing::error!(%error, "purge metrics");
            }

            conn.execute("DELETE FROM server_states WHERE taken_at < ?", [cutoff_ms])?;
            conn.execute("DELETE FROM disk_states WHERE taken_at < ?", [cutoff_ms])?;
            conn.execute("DELETE FROM server_events WHERE occurred_at < ?", [cutoff_ms])?;

            let mut logs = String::from("DELETE FROM log_lines WHERE logged_at < ?");
            if keep_unarchived {
                logs.push_str(&format!(" AND {IS_ARCHIVED}"));
            }
            conn.execute(&logs, [log_cutoff_ms])?;
            conn.execute("DELETE FROM log_volume WHERE minute < ?", [volume_cutoff_ms])?;

            conn.execute("DELETE FROM spans WHERE started_at < ?", [cutoff_ms * 1_000])?;
            conn.execute(
                "DELETE FROM span_exceptions WHERE occurred_at < ?",
                [cutoff_ms * 1_000],
            )?;
            conn.execute(
                "DELETE FROM uptime_results WHERE checked_at < ?",
                [Utc::now().timestamp_millis() - UPTIME_RETENTION_DAYS * 86_400_000],
            )?;
            conn.execute(
                "DELETE FROM cron_runs WHERE finished_at < ?",
                [Utc::now().timestamp_millis() - UPTIME_RETENTION_DAYS * 86_400_000],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn trim_to(
        &self,
        max_bytes: u64,
        count_unarchived: bool,
    ) -> Result<Option<Trim>> {
        self.write(move |conn| {
            conn.execute_batch("CHECKPOINT")?;
            let used_bytes = read_used_bytes(conn)?;
            if used_bytes <= max_bytes {
                return Ok(None);
            }

            let target = max_bytes / 100 * TRIM_TARGET_PERCENT;
            let share = 1.0 - target as f64 / used_bytes as f64;
            let newest = Utc::now().timestamp_millis() - TRIM_KEPT_MS;
            let logs_until: Option<i64> = conn.query_row(
                &format!(
                    "SELECT least(approx_quantile(logged_at, {share:.4}::FLOAT), ?) FROM log_lines"
                ),
                [newest],
                |row| row.get(0),
            )?;
            let spans_until: Option<i64> = conn.query_row(
                &format!(
                    "SELECT least(approx_quantile(started_at, {share:.4}::FLOAT), ?) FROM spans"
                ),
                [newest * 1000],
                |row| row.get(0),
            )?;

            let mut deleted_rows = 0;
            let mut unarchived_rows = 0;
            if let Some(until) = logs_until {
                if count_unarchived {
                    unarchived_rows = conn.query_row(
                        &format!(
                            "SELECT count(*) FROM log_lines WHERE logged_at <= ? AND NOT {IS_ARCHIVED}"
                        ),
                        [until],
                        |row| row.get(0),
                    )?;
                }
                deleted_rows +=
                    conn.execute("DELETE FROM log_lines WHERE logged_at <= ?", [until])?;
            }
            if let Some(until) = spans_until {
                deleted_rows +=
                    conn.execute("DELETE FROM spans WHERE started_at <= ?", [until])?;
                conn.execute(
                    "DELETE FROM span_exceptions WHERE occurred_at <= ?",
                    [until],
                )?;
            }
            if deleted_rows > 0 {
                conn.execute_batch("CHECKPOINT")?;
            }

            Ok(Some(Trim {
                used_bytes,
                deleted_rows,
                logs_until,
                spans_until,
                unarchived_rows,
            }))
        })
        .await
    }

    pub fn read_last_backup(&self) -> Option<Backup> {
        lock(&self.last_backup).clone()
    }

    pub async fn back_up(&self, dir: PathBuf) -> Result<Backup> {
        fs::create_dir_all(&dir).context("create the backup folder")?;
        let name = format!("daroyan-{}.duckdb", Utc::now().format("%Y-%m-%dT%H%M%S"));
        let path = dir.join(&name);
        let partial = dir.join(format!("{name}.partial"));
        let target = partial.to_string_lossy().replace('\'', "''");
        self.read(move |conn| {
            let source: String =
                conn.query_row("SELECT current_database()", [], |row| row.get(0))?;
            let source = source.replace('"', "\"\"");
            let version = read_storage_version(conn)?;
            conn.execute_batch(&format!(
                "ATTACH '{target}' AS backup (STORAGE_VERSION '{version}'); COPY FROM DATABASE \"{source}\" TO backup; DETACH backup;"
            ))
        })
        .await?;
        fs::rename(&partial, &path).context("finish the backup file")?;
        prune_backups(&dir);
        let backup = Backup {
            name,
            size_bytes: fs::metadata(&path).context("read the backup size")?.len(),
            created_at: Utc::now(),
        };
        *lock(&self.last_backup) = Some(backup.clone());
        Ok(backup)
    }

    pub async fn delete_server_rows(&self, server_id: String) -> Result<()> {
        self.write(move |conn| {
            conn.execute("DELETE FROM server_states WHERE server_id = ?", [&server_id])?;
            conn.execute("DELETE FROM disk_states WHERE server_id = ?", [&server_id])?;
            conn.execute("DELETE FROM server_events WHERE server_id = ?", [&server_id])?;
            conn.execute(
                "UPDATE log_lines SET server_id = ? WHERE server_id = ? AND stream = 'otlp'",
                [OTLP_SERVER_ID, &server_id],
            )?;
            conn.execute("DELETE FROM log_lines WHERE server_id = ?", [&server_id])?;
            conn.execute("DELETE FROM access_sources WHERE server_id = ?", [&server_id])?;
            conn.execute(
                "UPDATE spans SET server_id = NULL WHERE server_id = ?",
                [&server_id],
            )?;
            conn.execute(
                "UPDATE metric_series SET server_id = NULL WHERE server_id = ?",
                [&server_id],
            )?;
            Ok(())
        })
        .await
    }
}

pub fn spawn_analytics_tasks(
    analytics: Analytics,
    retention: Retention,
    max_bytes: Option<u64>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut flush = tokio::time::interval(FLUSH_INTERVAL);
        let mut purge = tokio::time::interval(PURGE_INTERVAL);
        let mut trim = tokio::time::interval(TRIM_INTERVAL);
        loop {
            tokio::select! {
                _ = flush.tick() => {
                    if let Err(error) = analytics.flush().await {
                        tracing::error!(error = %error, "flush analytics");
                    }
                }
                _ = purge.tick() => {
                    let purge = Purge::at(Utc::now().timestamp_millis(), retention);
                    if let Err(error) = analytics.purge(purge).await {
                        tracing::error!(error = %error, "purge analytics");
                    }
                }
                _ = trim.tick(), if max_bytes.is_some() => {
                    let Some(max_bytes) = max_bytes else { continue };
                    match analytics.trim_to(max_bytes, retention.archive_days.is_some()).await {
                        Ok(Some(trim)) if trim.deleted_rows == 0 => tracing::error!(
                            used_mb = trim.used_bytes / 1_048_576,
                            max_mb = max_bytes / 1_048_576,
                            "DuckDB is over its size cap and has no logs or spans older than an hour to delete, raise DUCKDB_MAX_SIZE_GB or lower RETENTION_DAYS"
                        ),
                        Ok(Some(trim)) if trim.unarchived_rows > 0 => tracing::error!(
                            used_mb = trim.used_bytes / 1_048_576,
                            max_mb = max_bytes / 1_048_576,
                            deleted_rows = trim.deleted_rows,
                            lost_rows = trim.unarchived_rows,
                            "deleted logs that were not archived yet, DuckDB was over its size cap, check the archive export or raise DUCKDB_MAX_SIZE_GB"
                        ),
                        Ok(Some(trim)) => tracing::warn!(
                            used_mb = trim.used_bytes / 1_048_576,
                            max_mb = max_bytes / 1_048_576,
                            deleted_rows = trim.deleted_rows,
                            logs_until = ?trim.logs_until,
                            spans_until = ?trim.spans_until,
                            "deleted the oldest logs and spans, DuckDB was over its size cap"
                        ),
                        Ok(None) => {}
                        Err(error) => tracing::error!(error = %error, "trim analytics"),
                    }
                }
            }
        }
    })
}

fn read_used_bytes(conn: &Connection) -> duckdb::Result<u64> {
    conn.query_row(
        "SELECT used_blocks * block_size FROM pragma_database_size() \
         WHERE database_name = current_database()",
        [],
        |row| row.get(0),
    )
}

fn read_storage_version(conn: &Connection) -> duckdb::Result<String> {
    let version: String = conn.query_row(
        "SELECT tags['storage_version'] FROM duckdb_databases() \
         WHERE database_name = current_database()",
        [],
        |row| row.get(0),
    )?;

    Ok(version.trim_end_matches('+').to_string())
}

fn prune_backups(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
                name.starts_with("daroyan-") && name.ends_with(".duckdb")
            })
        })
        .collect();
    backups.sort_unstable();
    let excess = backups.len().saturating_sub(KEPT_BACKUPS);
    for path in &backups[..excess] {
        if let Err(error) = fs::remove_file(path) {
            tracing::warn!(path = %path.display(), %error, "delete an old backup");
        }
    }
}

pub fn clamp_future_ms(at_ms: i64, now_ms: i64) -> i64 {
    if at_ms > now_ms.saturating_add(DAY_MS) { now_ms } else { at_ms }
}

pub fn clamp_future_us(at_us: i64, now_us: i64) -> i64 {
    if at_us > now_us.saturating_add(DAY_MS * 1_000) { now_us } else { at_us }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use chrono::Utc;
    use daroyan_protocol::log::LogBatch;
    use duckdb::Connection;
    use serde_json::json;

    use super::{Analytics, MAX_SERVER_ROWS, Purge};
    use crate::analytics::metrics::ingest::{
        MetricKind, MetricSample, Reading, SeriesRow, Temporality,
    };
    use crate::analytics::metrics::series::SeriesLimits;

    fn batch(lines: usize) -> LogBatch {
        let line = json!({ "at": 1, "service": "app", "stream": "file", "severity": 9, "message": "hi" });
        let lines = (0..lines)
            .map(|_| serde_json::from_value(line.clone()).unwrap())
            .collect();

        LogBatch { lines, ..LogBatch::default() }
    }

    #[test]
    fn keeps_room_for_other_servers_when_one_floods() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-shares-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();

        for _ in 0..(MAX_SERVER_ROWS / 5_000 + 2) {
            analytics.buffer_logs("srv_noisy", batch(5_000));
        }
        analytics.buffer_logs("srv_quiet", batch(1));

        let pending = super::lock(&analytics.pending);
        assert_eq!(pending.shares["srv_noisy"].logs, MAX_SERVER_ROWS);
        assert_eq!(pending.shares["srv_quiet"].logs, 1);
        assert_eq!(pending.logs.len(), MAX_SERVER_ROWS + 1);
        drop(pending);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn slow_writes_do_not_block_reads() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-writers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics
            .run(|conn| conn.execute_batch("CREATE TABLE samples (value INTEGER);"))
            .unwrap();
        let writer = analytics.clone();
        let slow = tokio::spawn(async move {
            writer
                .write(|conn| {
                    std::thread::sleep(Duration::from_millis(800));
                    conn.execute_batch("INSERT INTO samples VALUES (1);")
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let started = Instant::now();
        analytics
            .read(|conn| {
                conn.query_row("SELECT count(*) FROM samples", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .await
            .unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "{:?}",
            started.elapsed()
        );

        slow.await.unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_reads_do_not_block_writes() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-readers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics
            .run(|conn| conn.execute_batch("CREATE TABLE samples (value INTEGER);"))
            .unwrap();
        let reader = analytics.clone();
        let slow = tokio::spawn(async move {
            reader
                .read(|conn| {
                    std::thread::sleep(Duration::from_millis(800));
                    conn.query_row("SELECT count(*) FROM samples", [], |row| {
                        row.get::<_, i64>(0)
                    })
                })
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let started = Instant::now();
        analytics
            .write(|conn| conn.execute_batch("INSERT INTO samples VALUES (1);"))
            .await
            .unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "{:?}",
            started.elapsed()
        );
        let seen = analytics
            .read(|conn| {
                conn.query_row("SELECT count(*) FROM samples", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .await
            .unwrap();
        assert_eq!(seen, 1);
        slow.await.unwrap();
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn trims_the_oldest_rows_over_the_size_cap() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-trim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch(
                    "CREATE TABLE log_lines (logged_at BIGINT, message VARCHAR); \
                     CREATE TABLE spans (started_at BIGINT, name VARCHAR); \
                     CREATE TABLE span_exceptions (occurred_at BIGINT); \
                     INSERT INTO log_lines SELECT i, md5(i::VARCHAR) || md5((i * 7)::VARCHAR) FROM range(400000) AS r(i); \
                     INSERT INTO spans SELECT i, md5(i::VARCHAR) FROM range(100000) AS r(i); \
                     INSERT INTO span_exceptions SELECT i FROM range(1000) AS r(i); \
                     CHECKPOINT;",
                )
            })
            .unwrap();
        let used = analytics.run(super::read_used_bytes).unwrap();
        assert_eq!(analytics.trim_to(used, false).await.unwrap(), None);

        let trim = analytics.trim_to(used / 2, false).await.unwrap().unwrap();
        assert_eq!(trim.used_bytes, used);
        let logs_until = trim.logs_until.unwrap();
        assert!((200_000..260_000).contains(&logs_until), "{logs_until}");

        let (oldest, kept): (i64, i64) = analytics
            .run(|conn| {
                conn.query_row(
                    "SELECT min(logged_at), count(*) FROM log_lines",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(oldest, logs_until + 1);
        assert_eq!(kept, 399_999 - logs_until);

        let exceptions: i64 = analytics
            .run(|conn| {
                conn.query_row("SELECT count(*) FROM span_exceptions", [], |row| {
                    row.get(0)
                })
            })
            .unwrap();
        assert_eq!(exceptions, 0);

        let mut passes = 1;
        while analytics.trim_to(used / 2, false).await.unwrap().is_some() {
            passes += 1;
            assert!(passes < 12, "still over the cap after {passes} passes");
        }
        assert!(analytics.run(super::read_used_bytes).unwrap() <= used / 2);

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn keeps_the_newest_hour_when_other_data_fills_the_cap() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-trim-floor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        let now = Utc::now().timestamp_millis();
        analytics
            .run(move |conn| {
                conn.execute_batch(&format!(
                    "CREATE TABLE log_lines (logged_at BIGINT, message VARCHAR); \
                     CREATE TABLE spans (started_at BIGINT, name VARCHAR); \
                     CREATE TABLE span_exceptions (occurred_at BIGINT); \
                     CREATE TABLE server_states (value VARCHAR); \
                     INSERT INTO log_lines SELECT {now} - i, md5(i::VARCHAR) FROM range(20000) AS r(i); \
                     INSERT INTO spans SELECT ({now} - i) * 1000, md5(i::VARCHAR) FROM range(20000) AS r(i); \
                     INSERT INTO server_states SELECT md5(i::VARCHAR) || md5((i * 7)::VARCHAR) FROM range(400000) AS r(i); \
                     CHECKPOINT;"
                ))
            })
            .unwrap();
        let used = analytics.run(super::read_used_bytes).unwrap();

        for _ in 0..3 {
            let trim = analytics.trim_to(used / 2, false).await.unwrap().unwrap();
            assert_eq!(trim.deleted_rows, 0);
        }

        let kept: (i64, i64) = analytics
            .run(|conn| {
                conn.query_row(
                    "SELECT (SELECT count(*) FROM log_lines), (SELECT count(*) FROM spans)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(kept, (20_000, 20_000));

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn keeps_logs_that_are_not_archived_yet() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-purge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch(include_str!(
                    "../../../daroyan-migration/duckdb/0015_metrics.sql"
                ))?;
                conn.execute_batch(
                    "CREATE TABLE server_states (taken_at BIGINT); \
                     CREATE TABLE disk_states (taken_at BIGINT); \
                     CREATE TABLE server_events (occurred_at BIGINT); \
                     CREATE TABLE spans (started_at BIGINT); \
                     CREATE TABLE span_exceptions (occurred_at BIGINT); \
                     CREATE TABLE uptime_results (checked_at BIGINT); \
                     CREATE TABLE cron_runs (finished_at BIGINT); \
                     CREATE TABLE log_lines (logged_at BIGINT, ingested_at BIGINT); \
                     CREATE TABLE log_volume (minute BIGINT); \
                     CREATE TABLE archive_state (name VARCHAR PRIMARY KEY, value BIGINT NOT NULL); \
                     INSERT INTO log_lines VALUES (1, 10), (2, 20), (3, 30), (500, 500); \
                     INSERT INTO archive_state VALUES ('log_watermark', 20);",
                )
            })
            .unwrap();
        let logged_at = || {
            analytics
                .run(|conn| {
                    conn.prepare("SELECT logged_at FROM log_lines ORDER BY logged_at")?
                        .query_map([], |row| row.get::<_, i64>(0))?
                        .collect::<duckdb::Result<Vec<_>>>()
                })
                .unwrap()
        };

        let purge = Purge {
            now_ms: 0,
            cutoff_ms: 0,
            log_cutoff_ms: 100,
            volume_cutoff_ms: 0,
            point_cutoff_ms: 0,
            rollup_cutoff_ms: 0,
            keep_unarchived: true,
        };
        analytics.purge(purge).await.unwrap();
        assert_eq!(logged_at(), [3, 500]);

        analytics
            .purge(Purge { keep_unarchived: false, ..purge })
            .await
            .unwrap();
        assert_eq!(logged_at(), [500]);

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn purges_other_data_when_metric_rollups_fail() {
        let analytics =
            Analytics::try_from(Connection::open_in_memory().unwrap()).unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch(
                    "CREATE TABLE metric_points (taken_at BIGINT); \
                     INSERT INTO metric_points VALUES (0); \
                     CREATE TABLE server_states (taken_at BIGINT); \
                     CREATE TABLE disk_states (taken_at BIGINT); \
                     CREATE TABLE server_events (occurred_at BIGINT); \
                     CREATE TABLE spans (started_at BIGINT); \
                     CREATE TABLE span_exceptions (occurred_at BIGINT); \
                     CREATE TABLE uptime_results (checked_at BIGINT); \
                     CREATE TABLE cron_runs (finished_at BIGINT); \
                     CREATE TABLE log_lines (logged_at BIGINT); \
                     CREATE TABLE log_volume (minute BIGINT); \
                     INSERT INTO server_states VALUES (1), (500);",
                )
            })
            .unwrap();

        let purge = Purge {
            now_ms: 10 * 60 * 60 * 1000,
            cutoff_ms: 100,
            log_cutoff_ms: 0,
            volume_cutoff_ms: 0,
            point_cutoff_ms: 0,
            rollup_cutoff_ms: 0,
            keep_unarchived: false,
        };
        analytics.purge(purge).await.unwrap();

        let states: i64 = analytics
            .run(|conn| {
                conn.query_row("SELECT count(*) FROM server_states", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(states, 1);
    }

    #[tokio::test]
    async fn writes_the_series_again_after_a_failed_metric_flush() {
        let analytics =
            Analytics::try_from(Connection::open_in_memory().unwrap()).unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch(include_str!(
                    "../../../daroyan-migration/duckdb/0015_metrics.sql"
                ))
            })
            .unwrap();
        let sample = || MetricSample {
            series: SeriesRow {
                series_id: 1,
                token_id: "ing_1".into(),
                name: "queue.depth".into(),
                unit: "".into(),
                description: "".into(),
                kind: MetricKind::Gauge,
                is_monotonic: false,
                temporality: Some(Temporality::Cumulative),
                service: "api".into(),
                server_id: None,
                instance: None,
                attributes: "{}".to_string(),
                resource: None,
                bounds: None,
                first_seen: Utc::now().timestamp_millis(),
            },
            at: Utc::now().timestamp_millis(),
            start_at: None,
            reading: Reading::Number(1.0),
            trace_id: None,
            span_id: None,
        };
        let limits = SeriesLimits { per_token: 10, per_metric: 10 };

        analytics.accept_metrics(vec![sample()], limits).await.unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch("ALTER TABLE metric_points_staging RENAME TO hidden")
            })
            .unwrap();
        assert!(analytics.flush().await.is_err());

        analytics
            .run(|conn| {
                conn.execute_batch("ALTER TABLE hidden RENAME TO metric_points_staging")
            })
            .unwrap();
        analytics.accept_metrics(vec![sample()], limits).await.unwrap();
        analytics.flush().await.unwrap();

        let rows: (i64, i64) = analytics
            .run(|conn| {
                conn.query_row(
                    "SELECT (SELECT count(*) FROM metric_series), (SELECT count(*) FROM metric_points)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(rows, (1, 1));
    }

    #[tokio::test]
    async fn backs_up_to_a_readable_file() {
        let dir =
            std::env::temp_dir().join(format!("daroyan-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics
            .run(|conn| conn.execute_batch("CREATE TABLE samples (value INTEGER); INSERT INTO samples VALUES (1), (2);"))
            .unwrap();
        let backup = analytics.back_up(dir.join("backups")).await.unwrap();
        let path = dir.join("backups").join(&backup.name);
        let copy = Connection::open(&path).unwrap();
        let count: i64 = copy
            .query_row("SELECT count(*) FROM samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert!(!path.with_extension("duckdb.partial").exists());
        assert_eq!(analytics.read_last_backup().unwrap().name, backup.name);
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn keeps_metric_points_when_their_rollup_fails() {
        let analytics =
            Analytics::try_from(Connection::open_in_memory().unwrap()).unwrap();
        analytics
            .run(|conn| {
                conn.execute_batch(include_str!(
                    "../../../daroyan-migration/duckdb/0015_metrics.sql"
                ))?;
                conn.execute_batch(
                    "DROP TABLE metric_rollups; \
                     INSERT INTO metric_points (series_id, taken_at) VALUES (1, 0); \
                     CREATE TABLE server_states (taken_at BIGINT); \
                     CREATE TABLE disk_states (taken_at BIGINT); \
                     CREATE TABLE server_events (occurred_at BIGINT); \
                     CREATE TABLE spans (started_at BIGINT); \
                     CREATE TABLE span_exceptions (occurred_at BIGINT); \
                     CREATE TABLE uptime_results (checked_at BIGINT); \
                     CREATE TABLE cron_runs (finished_at BIGINT); \
                     CREATE TABLE log_lines (logged_at BIGINT); \
                     CREATE TABLE log_volume (minute BIGINT); \
                     INSERT INTO server_states VALUES (1), (500);",
                )
            })
            .unwrap();

        let purge = Purge {
            now_ms: 10 * 60 * 60 * 1000,
            cutoff_ms: 100,
            log_cutoff_ms: 0,
            volume_cutoff_ms: 0,
            point_cutoff_ms: 100,
            rollup_cutoff_ms: 0,
            keep_unarchived: false,
        };
        analytics.purge(purge).await.unwrap();

        let rows: (i64, i64) = analytics
            .run(|conn| {
                conn.query_row(
                    "SELECT (SELECT count(*) FROM metric_points), (SELECT count(*) FROM server_states)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(rows, (1, 1));
    }
}
