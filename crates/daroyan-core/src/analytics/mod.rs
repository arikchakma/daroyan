pub mod cron_runs;
pub mod log_lines;
pub mod log_parse;
pub mod metrics;
pub mod server_events;
pub mod server_states;
pub mod spans;
pub mod uptime_results;

use std::collections::HashMap;
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

use crate::analytics::log_lines::{LogRow, OTLP_SERVER_ID, append_logs, to_log_rows};
use crate::analytics::metrics::ingest::{MetricSample, SeriesRow};
use crate::analytics::metrics::series::{Admission, PointRow, SeriesCache, SeriesLimits};
use crate::analytics::metrics::store::{
    append_metrics, list_known_series, purge_metrics, roll_up_metrics,
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
const MEMORY_SETTINGS: &str = "SET memory_limit = '512MB'; SET threads = 2;";
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);
const UPTIME_RETENTION_DAYS: i64 = 90;
const MAX_PENDING_ROWS: usize = 200_000;
const MAX_SERVER_ROWS: usize = MAX_PENDING_ROWS / 10;
const MAX_REPORT_EVENTS: usize = 1_000;
const KEPT_BACKUPS: usize = 8;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const BACKUP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const SLOW_READ: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Analytics {
    conn: Arc<Mutex<Connection>>,
    reader: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<Pending>>,
    metric_series: Arc<Mutex<SeriesCache>>,
    last_backup: Arc<Mutex<Option<Backup>>>,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
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

impl Analytics {
    pub fn open(path: &Path) -> duckdb::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MEMORY_SETTINGS)?;
        let reader = conn.try_clone()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            reader: Arc::new(Mutex::new(reader)),
            pending: Arc::default(),
            metric_series: Arc::default(),
            last_backup: Arc::default(),
        })
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
            let known = self.read(move |conn| list_known_series(conn, now)).await?;
            lock(&self.metric_series).load(known);
        }

        let mut admission = lock(&self.metric_series).admit(samples, limits, now);
        let mut pending = lock(&self.pending);
        pending.metric_series.append(&mut admission.series);
        pending.metric_points.append(&mut admission.points);

        Ok(admission)
    }

    pub async fn flush(&self) -> Result<()> {
        let pending = std::mem::take(&mut *lock(&self.pending));
        if pending.states.is_empty()
            && pending.events.is_empty()
            && pending.logs.is_empty()
            && pending.spans.is_empty()
            && pending.exceptions.is_empty()
            && pending.metric_series.is_empty()
            && pending.metric_points.is_empty()
        {
            return Ok(());
        }
        self.write(move |conn| {
            append_states(conn, &pending.states)?;
            append_disks(conn, &pending.disks)?;
            append_events(conn, &pending.events)?;
            append_logs(conn, &pending.logs)?;
            append_spans(conn, &pending.spans, &pending.exceptions)?;
            append_metrics(conn, &pending.metric_series, &pending.metric_points)
        })
        .await
    }

    pub async fn purge_before(&self, now: i64, retention: Retention) -> Result<()> {
        let cutoff_ms = now - i64::from(retention.days) * DAY_MS;
        let log_cutoff_ms = now - i64::from(retention.log_days) * DAY_MS;
        let point_cutoff = now - i64::from(retention.metric_days) * DAY_MS;
        let rollup_cutoff = now - i64::from(retention.metric_rollup_days) * DAY_MS;
        lock(&self.metric_series).forget_before(rollup_cutoff);

        self.write(move |conn| {
            roll_up_metrics(conn, now)?;
            purge_metrics(conn, point_cutoff, rollup_cutoff)?;
            conn.execute("DELETE FROM server_states WHERE taken_at < ?", [cutoff_ms])?;
            conn.execute("DELETE FROM disk_states WHERE taken_at < ?", [cutoff_ms])?;
            conn.execute("DELETE FROM server_events WHERE occurred_at < ?", [cutoff_ms])?;
            conn.execute("DELETE FROM log_lines WHERE logged_at < ?", [log_cutoff_ms])?;
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

    pub fn read_last_backup(&self) -> Option<Backup> {
        lock(&self.last_backup).clone()
    }

    pub async fn back_up(&self, dir: PathBuf) -> Result<Backup> {
        fs::create_dir_all(&dir).context("create the backup folder")?;
        let name = format!("daroyan-{}.duckdb", Utc::now().format("%Y-%m-%dT%H%M%S"));
        let path = dir.join(&name);
        let partial = dir.join(format!("{name}.partial"));
        let target = partial.to_string_lossy().replace('\'', "''");
        self.write(move |conn| {
            let source: String =
                conn.query_row("SELECT current_database()", [], |row| row.get(0))?;
            let source = source.replace('"', "\"\"");
            conn.execute_batch(&format!(
                "ATTACH '{target}' AS backup; COPY FROM DATABASE \"{source}\" TO backup; DETACH backup;"
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

#[derive(Clone, Copy, Debug)]
pub struct Retention {
    pub days: u32,
    pub log_days: u32,
    pub metric_days: u32,
    pub metric_rollup_days: u32,
}

impl From<&Config> for Retention {
    fn from(config: &Config) -> Self {
        Self {
            days: config.retention_days,
            log_days: config.log_retention_days,
            metric_days: config.metrics.retention_days,
            metric_rollup_days: config.metrics.rollup_retention_days,
        }
    }
}

pub fn spawn_analytics_tasks(
    analytics: Analytics,
    retention: Retention,
    backup_dir: Option<PathBuf>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut flush = tokio::time::interval(FLUSH_INTERVAL);
        let mut purge = tokio::time::interval(PURGE_INTERVAL);
        let mut backup = tokio::time::interval_at(
            tokio::time::Instant::now() + BACKUP_INTERVAL,
            BACKUP_INTERVAL,
        );
        loop {
            tokio::select! {
                _ = flush.tick() => {
                    if let Err(error) = analytics.flush().await {
                        tracing::error!(error = %error, "flush analytics");
                    }
                }
                _ = purge.tick() => {
                    let now = Utc::now().timestamp_millis();
                    if let Err(error) = analytics.purge_before(now, retention).await {
                        tracing::error!(error = %error, "purge analytics");
                    }
                }
                _ = backup.tick(), if backup_dir.is_some() => {
                    let Some(dir) = backup_dir.clone() else { continue };
                    match analytics.back_up(dir).await {
                        Ok(backup) => tracing::info!(name = %backup.name, "backed up analytics"),
                        Err(error) => tracing::error!(error = %error, "back up analytics"),
                    }
                }
            }
        }
    })
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

    use daroyan_protocol::log::LogBatch;
    use serde_json::json;

    use super::{Analytics, MAX_SERVER_ROWS};

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
        let copy = duckdb::Connection::open(&path).unwrap();
        let count: i64 = copy
            .query_row("SELECT count(*) FROM samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert!(!path.with_extension("duckdb.partial").exists());
        assert_eq!(analytics.read_last_backup().unwrap().name, backup.name);
        std::fs::remove_dir_all(dir).ok();
    }
}
