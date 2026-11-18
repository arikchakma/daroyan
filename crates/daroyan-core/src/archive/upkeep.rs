use std::path::Path as LocalPath;

use chrono::{DateTime, Utc};
use object_store::ObjectStoreExt;
use object_store::path::Path;

use crate::analytics::Analytics;
use crate::analytics::archive::upkeep::{
    Merge, expire_files, forget_files, list_deleted, merge_files, plan_merges,
    replace_files,
};
use crate::analytics::archive::{DAY_MS, FileKind, HOUR_MS};
use crate::archive::{Archive, TENANT, meta_path, reset_dir};
use crate::error::Result;

const SETTLE_MS: i64 = 6 * HOUR_MS;
const DELETE_DELAY_MS: i64 = 2 * HOUR_MS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Upkeep {
    pub merged: usize,
    pub expired: usize,
    pub deleted: usize,
}

impl Archive {
    pub async fn tidy(&self, analytics: &Analytics) -> Result<Upkeep> {
        self.tidy_at(analytics, Utc::now()).await
    }

    async fn tidy_at(&self, analytics: &Analytics, now: DateTime<Utc>) -> Result<Upkeep> {
        let now_ms = now.timestamp_millis();
        let before = now_ms - i64::from(self.retention_days) * DAY_MS;

        let expired = analytics
            .write(move |conn| expire_files(conn, before, now_ms))
            .await?;
        let merged = self.merge_days(analytics, now_ms).await?;
        let deleted = self.delete_expired(analytics, now_ms).await?;

        Ok(Upkeep { merged, expired, deleted })
    }

    async fn merge_days(&self, analytics: &Analytics, now_ms: i64) -> Result<usize> {
        let closed_before = now_ms - SETTLE_MS;
        let merges = analytics.read(move |conn| plan_merges(conn, closed_before)).await?;

        let mut merged = 0;
        for merge in merges {
            let dir = reset_dir(self.work_dir.join("merge")).await?;
            match self.merge(analytics, merge, &dir, now_ms).await {
                Ok(true) => merged += 1,
                Ok(false) => {}
                Err(error) => tracing::warn!(%error, "merge archived log files"),
            }
            tokio::fs::remove_dir_all(&dir).await.ok();
        }

        Ok(merged)
    }

    async fn merge(
        &self,
        analytics: &Analytics,
        merge: Merge,
        dir: &LocalPath,
        now_ms: i64,
    ) -> Result<bool> {
        let mut sources = Vec::with_capacity(merge.files.len());
        for (index, file) in merge.files.iter().enumerate() {
            let local = dir.join(format!("{index}.parquet"));
            self.download(&Path::parse(&file.path)?, &local).await?;
            sources.push(local);
        }

        let prefix = match merge.kind {
            FileKind::Day => "day-",
            FileKind::Hour => "",
        };
        let target =
            dir.join(format!("{prefix}{now_ms}-{:08x}.parquet", rand::random::<u32>()));
        let file = analytics
            .read(move |conn| merge_files(conn, &merge, &sources, &target))
            .await?;
        let Some(file) = file else { return Ok(false) };

        let merged = self.publish(file, now_ms).await?;
        analytics
            .write(move |conn| replace_files(conn, &merged, TENANT, now_ms))
            .await?;

        Ok(true)
    }

    async fn delete_expired(&self, analytics: &Analytics, now_ms: i64) -> Result<usize> {
        let deleted_before = now_ms - DELETE_DELAY_MS;
        let paths =
            analytics.read(move |conn| list_deleted(conn, deleted_before)).await?;

        let mut gone = Vec::with_capacity(paths.len());
        for path in paths {
            match self.delete_file(&path).await {
                Ok(()) => gone.push(path),
                Err(error) => tracing::warn!(%error, path, "delete an archived log file"),
            }
        }

        let count = gone.len();
        analytics.write(move |conn| forget_files(conn, &gone)).await?;

        Ok(count)
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        for target in [meta_path(path), path.to_string()] {
            match self.store.delete(&Path::parse(target)?).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeDelta, Utc};
    use futures_util::TryStreamExt;
    use object_store::memory::InMemory;
    use object_store::path::Path;
    use object_store::{ObjectStore, ObjectStoreExt};

    use super::Upkeep;
    use crate::analytics::Analytics;
    use crate::analytics::archive::{DAY_MS, HOUR_MS, create_log_tables};
    use crate::archive::Archive;

    async fn list(store: &InMemory) -> Vec<String> {
        let mut paths: Vec<String> = store
            .list(None)
            .map_ok(|meta| meta.location.to_string())
            .try_filter(|path| std::future::ready(!path.ends_with(".json")))
            .try_collect()
            .await
            .unwrap();
        paths.sort();

        paths
    }

    #[tokio::test]
    async fn merges_old_days_then_deletes_what_expired() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-upkeep-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        let now = Utc::now();
        let day = (now.timestamp_millis() - 3 * DAY_MS).div_euclid(DAY_MS) * DAY_MS;
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics
            .run(move |conn| {
                create_log_tables(conn)?;
                conn.execute(
                    "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, ingested_at) \
                     VALUES ('srv_1', ?, 'api', 'otlp', 9, 'one', '{\"user.id\":\"u_1\"}', 1), \
                            ('srv_1', ?, 'web', 'otlp', 9, 'two', '{\"took\":1.5}', 1)",
                    [day + HOUR_MS, day + 5 * HOUR_MS],
                )?;

                Ok(())
            })
            .unwrap();

        let store = Arc::new(InMemory::new());
        let archive = Archive::new(store.clone(), "prod", dir.join("work"));
        archive.export_logs(&analytics).await.unwrap();
        assert_eq!(list(&store).await.len(), 2);

        let stray = Path::from("prod/logs/v1/tenant=default/dt=2020-01-01/stray.parquet");
        store.put(&stray, "x".into()).await.unwrap();

        let first = archive.tidy_at(&analytics, now).await.unwrap();
        assert_eq!(first, Upkeep { merged: 1, ..Upkeep::default() });
        let paths = list(&store).await;
        assert_eq!(paths.len(), 4);
        let day_file = paths.iter().find(|path| path.contains("/day-")).unwrap();

        let rows: (i64, String) = analytics
            .run(|conn| {
                conn.query_row(
                    "SELECT rows, CAST(columns AS VARCHAR) FROM archive_files WHERE deleted_at IS NULL",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(rows, (2, "[a.user.id, a.took]".to_string()));

        let later = archive.tidy_at(&analytics, now + TimeDelta::days(2)).await.unwrap();
        assert_eq!(later, Upkeep { deleted: 2, ..Upkeep::default() });
        assert_eq!(list(&store).await, [stray.to_string(), day_file.clone()]);
        let metas: Vec<String> = store
            .list(None)
            .map_ok(|meta| meta.location.to_string())
            .try_filter(|path| std::future::ready(path.ends_with(".json")))
            .try_collect()
            .await
            .unwrap();
        assert_eq!(metas, [format!("{day_file}.json")]);

        let short = archive.clone().with_retention_days(1);
        let expired = short.tidy_at(&analytics, now).await.unwrap();
        assert_eq!(expired, Upkeep { expired: 1, ..Upkeep::default() });
        let gone = short.tidy_at(&analytics, now + TimeDelta::hours(3)).await.unwrap();
        assert_eq!(gone, Upkeep { deleted: 1, ..Upkeep::default() });
        assert_eq!(list(&store).await, [stray.to_string()]);

        std::fs::remove_dir_all(dir).ok();
    }
}
