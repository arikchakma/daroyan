pub mod cron_runs;
pub mod log_lines;
pub mod log_parse;
pub mod server_events;
pub mod server_states;
pub mod spans;
pub mod uptime_results;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use daroyan_protocol::log::LogBatch;
use daroyan_protocol::report::Report;
use duckdb::Connection;
use tokio::task::{JoinHandle, spawn_blocking};

use crate::analytics::log_lines::{LogRow, append_logs, to_log_rows};
use crate::analytics::server_events::{
    EventRow, append_events, to_event_row, to_event_rows,
};
use crate::analytics::server_states::{StateRow, append_states};
use crate::analytics::spans::{ExceptionRow, SpanRow, append_spans};
use crate::error::{Error, ErrorName, Result};

const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const MEMORY_SETTINGS: &str = "SET memory_limit = '512MB'; SET threads = 2;";
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);
const UPTIME_RETENTION_DAYS: i64 = 90;
const MAX_PENDING_ROWS: usize = 200_000;
const KEPT_BACKUPS: usize = 8;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const BACKUP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone)]
pub struct Analytics {
    conn: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<Pending>>,
    last_backup: Arc<Mutex<Option<Backup>>>,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Default)]
struct Pending {
    states: Vec<StateRow>,
    events: Vec<EventRow>,
    logs: Vec<LogRow>,
    spans: Vec<SpanRow>,
    exceptions: Vec<ExceptionRow>,
}

impl Analytics {
    pub fn open(path: &Path) -> duckdb::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MEMORY_SETTINGS)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            pending: Arc::default(),
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
        let reader = lock(&self.conn).try_clone()?;
        Ok(spawn_blocking(move || f(&reader)).await??)
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
        let mut pending = lock(&self.pending);
        if pending.states.len() + pending.events.len() >= MAX_PENDING_ROWS {
            tracing::warn!(server_id, "dropped a report, the write buffer is full");
            return;
        }
        pending.states.push(StateRow::from_state(server_id, &report.state));
        pending.events.extend(to_event_rows(server_id, &report.events));
    }

    pub fn buffer_server_event(&self, server_id: &str, kind: &str) {
        let mut pending = lock(&self.pending);
        if pending.states.len() + pending.events.len() >= MAX_PENDING_ROWS {
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
        pending.logs.extend(rows);
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

    pub async fn flush(&self) -> Result<()> {
        let pending = std::mem::take(&mut *lock(&self.pending));
        if pending.states.is_empty()
            && pending.events.is_empty()
            && pending.logs.is_empty()
            && pending.spans.is_empty()
            && pending.exceptions.is_empty()
        {
            return Ok(());
        }
        self.write(move |conn| {
            append_states(conn, &pending.states)?;
            append_events(conn, &pending.events)?;
            append_logs(conn, &pending.logs)?;
            append_spans(conn, &pending.spans, &pending.exceptions)
        })
        .await
    }

    pub async fn purge_before(&self, cutoff_ms: i64, log_cutoff_ms: i64) -> Result<()> {
        self.write(move |conn| {
            conn.execute("DELETE FROM server_states WHERE taken_at < ?", [cutoff_ms])?;
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
            conn.execute("DELETE FROM server_events WHERE server_id = ?", [&server_id])?;
            conn.execute("DELETE FROM log_lines WHERE server_id = ?", [&server_id])?;
            Ok(())
        })
        .await
    }
}

pub fn spawn_analytics_tasks(
    analytics: Analytics,
    retention_days: u32,
    log_retention_days: u32,
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
                    let cutoff = now - i64::from(retention_days) * DAY_MS;
                    let log_cutoff = now - i64::from(log_retention_days) * DAY_MS;
                    if let Err(error) = analytics.purge_before(cutoff, log_cutoff).await {
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

    use super::Analytics;

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
