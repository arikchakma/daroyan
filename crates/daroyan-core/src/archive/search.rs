use std::fs::{self, File};
use std::path::{Path as LocalPath, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use object_store::path::Path;
use tokio::task::spawn_blocking;

use crate::analytics::Analytics;
use crate::analytics::archive::search::{
    ColdDay, ColdFile, count_cold_logs, list_cold_context, list_cold_days, list_cold_logs,
};
use crate::analytics::archive::{DAY_MS, HOUR_MS};
use crate::analytics::log_lines::{
    LogBucket, LogItem, LogPage, LogQuery, LogScan, LogSearch, MINUTE_MS,
    count_log_buckets, count_logs, count_rolled_buckets, list_log_context, list_logs,
    read_oldest_log,
};
use crate::archive::Archive;
use crate::error::Result;

const MAX_SCAN_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SCAN_TIME: Duration = Duration::from_secs(20);
const CACHE_IN_USE: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Default)]
struct ColdPage {
    lines: Vec<LogItem>,
    scan: LogScan,
    stopped_at: Option<i64>,
}

pub async fn search_logs(
    analytics: &Analytics,
    archive: Option<&Archive>,
    query: LogQuery,
) -> Result<LogPage> {
    let wanted = query.limit + 1;
    let Some(archive) = archive else {
        let lines =
            list_logs(analytics, LogQuery { limit: wanted, ..query.clone() }).await?;
        return Ok(LogPage::new(lines, &query, None, None));
    };

    let floor = archive.hot_floor(analytics).await?;
    let search = &query.search;
    let mut lines = Vec::new();
    let mut skip = query.offset;

    if search.until >= floor {
        let hot = LogSearch {
            since: Some(search.since.map_or(floor, |since| since.max(floor))),
            ..search.clone()
        };
        lines = list_logs(
            analytics,
            LogQuery { search: hot.clone(), offset: skip, limit: wanted },
        )
        .await?;

        let is_done = lines.len() >= wanted as usize
            || search.since.is_some_and(|since| since >= floor);
        if is_done {
            return Ok(LogPage::new(lines, &query, None, None));
        }
        skip = match (lines.is_empty(), skip) {
            (true, 1..) => {
                let matched = count_logs(analytics, hot).await?;
                skip.saturating_sub(u32::try_from(matched).unwrap_or(u32::MAX))
            }
            _ => 0,
        };
    }

    let cold = LogSearch {
        until: search.until.min(floor - 1),
        ..search.clone()
    };
    let page = archive
        .search_cold(analytics, cold, skip, wanted - lines.len() as u32)
        .await?;
    lines.extend(page.lines);

    Ok(LogPage::new(lines, &query, page.stopped_at, Some(page.scan)))
}

pub async fn count_any_log_buckets(
    analytics: &Analytics,
    archive: Option<&Archive>,
    search: LogSearch,
    bucket_ms: i64,
) -> Result<Vec<LogBucket>> {
    let Some(archive) = archive else {
        return count_log_buckets(analytics, search, bucket_ms).await;
    };

    let floor = archive.hot_floor(analytics).await?;
    let reaches_archive = search.since.is_none_or(|since| since < floor);
    if reaches_archive && search.can_use_rollup() && bucket_ms >= MINUTE_MS {
        return count_rolled_buckets(analytics, search, bucket_ms).await;
    }

    count_log_buckets(analytics, search, bucket_ms).await
}

pub async fn list_any_log_context(
    analytics: &Analytics,
    archive: Option<&Archive>,
    server_id: String,
    service: String,
    at: i64,
    limit: u32,
) -> Result<Vec<LogItem>> {
    let Some(archive) = archive else {
        return list_log_context(analytics, server_id, service, at, limit).await;
    };
    if at >= archive.hot_floor(analytics).await? {
        return list_log_context(analytics, server_id, service, at, limit).await;
    }

    let day = at.div_euclid(DAY_MS) * DAY_MS;
    let services = vec![service.clone()];
    let days = analytics
        .read(move |conn| list_cold_days(conn, Some(day), day + DAY_MS - 1, &services))
        .await?;
    let Some(day) = days.into_iter().find(|found| found.start == day) else {
        return Ok(Vec::new());
    };

    let sources = archive.open_files(&day.files).await?;
    let columns = day.columns();
    analytics
        .read(move |conn| {
            list_cold_context(conn, &sources, &columns, &server_id, &service, at, limit)
        })
        .await
}

impl Archive {
    pub async fn hot_floor(&self, analytics: &Analytics) -> Result<i64> {
        let kept = Utc::now().timestamp_millis()
            - i64::from(self.log_retention_days) * DAY_MS
            + HOUR_MS;
        let oldest = read_oldest_log(analytics).await?;

        Ok(oldest.map_or(kept, |oldest| oldest.max(kept)))
    }

    async fn search_cold(
        &self,
        analytics: &Analytics,
        search: LogSearch,
        skip: u32,
        wanted: u32,
    ) -> Result<ColdPage> {
        let services = if search.services.negate {
            Vec::new()
        } else {
            search.services.values.clone()
        };
        let (since, until) = (search.since, search.until);
        let days = analytics
            .read(move |conn| list_cold_days(conn, since, until, &services))
            .await?;

        let started = Instant::now();
        let mut page = ColdPage::default();
        let mut skip = skip;
        for day in days {
            let needed = wanted.saturating_sub(page.lines.len() as u32);
            if needed == 0 {
                break;
            }

            let day_end = (day.start + DAY_MS - 1).min(search.until);
            let is_over_budget =
                page.scan.bytes >= MAX_SCAN_BYTES || started.elapsed() >= MAX_SCAN_TIME;
            if is_over_budget {
                page.stopped_at = Some(day_end);
                break;
            }

            let found = self.search_day(analytics, &search, &day, skip, needed).await?;
            page.scan.files += day.files.len();
            page.scan.bytes += day.bytes();
            skip = match (found.lines.is_empty(), skip) {
                (true, 1..) => skip.saturating_sub(found.matched),
                _ => 0,
            };
            page.lines.extend(found.lines);
        }

        Ok(page)
    }

    async fn search_day(
        &self,
        analytics: &Analytics,
        search: &LogSearch,
        day: &ColdDay,
        skip: u32,
        needed: u32,
    ) -> Result<DayMatches> {
        let window = LogSearch {
            since: Some(search.since.map_or(day.start, |since| since.max(day.start))),
            until: search.until.min(day.start + DAY_MS - 1),
            ..search.clone()
        };
        let sources = self.open_files(&day.files).await?;
        let columns = day.columns();

        analytics
            .read(move |conn| {
                let lines =
                    list_cold_logs(conn, &sources, &columns, &window, skip, needed)?;
                let matched = match (lines.is_empty(), skip) {
                    (true, 1..) => count_cold_logs(conn, &sources, &columns, &window)?,
                    _ => 0,
                };

                Ok(DayMatches {
                    lines,
                    matched: u32::try_from(matched).unwrap_or(u32::MAX),
                })
            })
            .await
    }

    async fn open_files(&self, files: &[ColdFile]) -> Result<Vec<String>> {
        if let Some(bucket) = &self.range_bucket {
            return Ok(files
                .iter()
                .map(|file| format!("s3://{bucket}/{}", file.path))
                .collect());
        }

        let mut sources = Vec::with_capacity(files.len());
        for file in files {
            sources
                .push(self.cached_file(&file.path).await?.to_string_lossy().to_string());
        }

        let dir = self.cache_dir();
        let max_bytes = self.cache_bytes;
        spawn_blocking(move || trim_cache(&dir, max_bytes)).await?;

        Ok(sources)
    }

    fn cache_dir(&self) -> PathBuf {
        self.work_dir.join("cache")
    }

    async fn cached_file(&self, path: &str) -> Result<PathBuf> {
        let local = self.cache_dir().join(path);
        if let Ok(file) = File::options().write(true).open(&local) {
            file.set_modified(SystemTime::now()).ok();
            return Ok(local);
        }

        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let partial = local.with_extension("partial");
        self.download(&Path::parse(path)?, &partial).await?;
        tokio::fs::rename(&partial, &local).await?;

        Ok(local)
    }
}

struct DayMatches {
    lines: Vec<LogItem>,
    matched: u32,
}

fn trim_cache(dir: &LocalPath, max_bytes: u64) {
    let mut files = Vec::new();
    collect_files(dir, &mut files);

    let mut total: u64 = files.iter().map(|(_, bytes, _)| bytes).sum();
    if total <= max_bytes {
        return;
    }

    let in_use = SystemTime::now() - CACHE_IN_USE;
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, bytes, modified) in files {
        if total <= max_bytes || modified > in_use {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= bytes;
        }
    }
}

fn collect_files(dir: &LocalPath, files: &mut Vec<(PathBuf, u64, SystemTime)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            collect_files(&entry.path(), files);
        } else {
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            files.push((entry.path(), meta.len(), modified));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use object_store::memory::InMemory;

    use super::{list_any_log_context, search_logs};
    use crate::analytics::Analytics;
    use crate::analytics::archive::{DAY_MS, create_log_tables};
    use crate::analytics::log_lines::{LogCursor, LogQuery, LogSearch, SearchTerm};
    use crate::archive::Archive;

    fn query(until: i64, offset: u32, limit: u32) -> LogQuery {
        LogQuery {
            search: LogSearch { until, ..LogSearch::default() },
            offset,
            limit,
        }
    }

    #[tokio::test]
    async fn pages_from_the_hot_table_into_the_archive() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-search-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        let now = Utc::now().timestamp_millis();
        let (hot, cold_new, cold_old) =
            (now - 60_000, now - 2 * DAY_MS, now - 3 * DAY_MS);
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics
            .run(move |conn| {
                create_log_tables(conn)?;
                conn.execute(
                    "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, ingested_at) VALUES \
                     ('srv_1', ?, 'api', 'otlp', 9, 'hot one', NULL, 1), \
                     ('srv_1', ?, 'api', 'otlp', 9, 'hot two', NULL, 1), \
                     ('srv_1', ?, 'api', 'otlp', 17, 'cold new', '{\"user.id\":\"u_9\"}', 1), \
                     ('srv_1', ?, 'api', 'otlp', 9, 'cold old', NULL, 1)",
                    [hot, hot - 1, cold_new, cold_old],
                )?;

                Ok(())
            })
            .unwrap();

        let archive = Archive::new(Arc::new(InMemory::new()), "", dir.join("work"))
            .with_log_retention_days(1);
        archive.export_logs(&analytics).await.unwrap();
        analytics
            .run(move |conn| {
                conn.execute("DELETE FROM log_lines WHERE logged_at < ?", [now - DAY_MS])
            })
            .unwrap();

        let first = search_logs(&analytics, Some(&archive), query(now, 0, 2))
            .await
            .unwrap();
        let messages = |lines: &[crate::analytics::log_lines::LogItem]| {
            lines.iter().map(|line| line.message.clone()).collect::<Vec<_>>()
        };
        assert_eq!(messages(&first.lines), ["hot one", "hot two"]);
        assert!(first.has_more);
        assert_eq!(first.scan.map(|scan| scan.files), Some(1));
        let next = first.next.unwrap();
        assert_eq!(next, LogCursor { at: hot - 1, skip: 1 });

        let second =
            search_logs(&analytics, Some(&archive), query(next.at, next.skip, 2))
                .await
                .unwrap();
        assert_eq!(messages(&second.lines), ["cold new", "cold old"]);
        assert!(!second.has_more);
        assert_eq!(second.scan.unwrap().files, 2);
        assert_eq!(
            second.lines[0].attributes,
            Some(serde_json::json!({ "user.id": "u_9" }))
        );

        let legacy = search_logs(&analytics, Some(&archive), query(now, 3, 1))
            .await
            .unwrap();
        assert_eq!(messages(&legacy.lines), ["cold old"]);

        let found = search_logs(
            &analytics,
            Some(&archive),
            LogQuery {
                search: LogSearch {
                    until: now,
                    terms: vec![SearchTerm::key("user.id", "u_9", false)],
                    ..LogSearch::default()
                },
                offset: 0,
                limit: 10,
            },
        )
        .await
        .unwrap();
        assert_eq!(messages(&found.lines), ["cold new"]);

        let context = list_any_log_context(
            &analytics,
            Some(&archive),
            "srv_1".into(),
            "api".into(),
            cold_new,
            5,
        )
        .await
        .unwrap();
        assert_eq!(messages(&context), ["cold new"]);

        std::fs::remove_dir_all(dir).ok();
    }
}
