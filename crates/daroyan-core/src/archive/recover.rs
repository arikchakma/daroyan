use std::collections::HashSet;

use chrono::Utc;
use futures_util::TryStreamExt;
use object_store::ObjectStoreExt;
use object_store::path::Path;

use crate::analytics::Analytics;
use crate::analytics::archive::recover::{Recovery, apply_recovery, list_known_files};
use crate::analytics::archive::{ArchivedFile, FileMeta};
use crate::archive::{Archive, TENANT, meta_path};
use crate::error::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reconcile {
    pub adopted: usize,
    pub superseded: usize,
    pub described: usize,
}

impl Reconcile {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Archive {
    pub async fn reconcile(&self, analytics: &Analytics) -> Result<Reconcile> {
        let logs = self.object_path("logs/v1");
        let listed: HashSet<String> = self
            .store
            .list(Some(&logs))
            .map_ok(|object| object.location.to_string())
            .try_collect()
            .await?;
        let known = analytics.read(list_known_files).await?;
        let known_paths = known.paths();

        let mut found = Vec::new();
        for path in &listed {
            let Some(data) = path.strip_suffix(".json") else { continue };
            if !listed.contains(data) || known_paths.contains(data) {
                continue;
            }

            match self.read_meta(path).await {
                Ok(meta) => found.push(ArchivedFile { path: data.to_string(), meta }),
                Err(error) => {
                    tracing::warn!(%error, path, "read an archived file's metadata");
                }
            }
        }

        let mut described = 0;
        for file in &known.live {
            if listed.contains(&file.path) && !listed.contains(&meta_path(&file.path)) {
                self.write_meta(&file.path, &file.meta).await?;
                described += 1;
            }
        }

        let recovery = Recovery::plan(found, &known);
        let done = Reconcile {
            adopted: recovery.adopted.len(),
            superseded: recovery.superseded.len(),
            described,
        };
        if !recovery.is_empty() || recovery.watermark != known.watermark {
            let now_ms = Utc::now().timestamp_millis();
            analytics
                .write(move |conn| apply_recovery(conn, &recovery, TENANT, now_ms))
                .await?;
        }

        Ok(done)
    }

    async fn read_meta(&self, path: &str) -> Result<FileMeta> {
        let bytes = self.store.get(&Path::parse(path)?).await?.bytes().await?;

        Ok(serde_json::from_slice(&bytes)?)
    }

    pub(super) async fn write_meta(&self, path: &str, meta: &FileMeta) -> Result<()> {
        let body = serde_json::to_vec(meta)?;
        self.store.put(&Path::parse(meta_path(path))?, body.into()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::TryStreamExt;
    use object_store::memory::InMemory;
    use object_store::{ObjectStore, ObjectStoreExt};

    use super::Reconcile;
    use crate::analytics::Analytics;
    use crate::analytics::archive::{create_log_tables, summarize_archive};
    use crate::archive::Archive;

    fn open(dir: &std::path::Path, name: &str) -> Analytics {
        let analytics = Analytics::open(&dir.join(name)).unwrap();
        analytics.run(create_log_tables).unwrap();

        analytics
    }

    fn insert(analytics: &Analytics, ingested_at: i64) {
        analytics
            .run(move |conn| {
                conn.execute(
                    "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, ingested_at) \
                     VALUES ('srv_1', 1794002400000, 'api', 'otlp', 9, 'one', '{\"user.id\":\"u_1\"}', ?), \
                            ('srv_1', 1794006000000, 'web', 'otlp', 9, 'two', NULL, ?)",
                    [ingested_at, ingested_at],
                )
            })
            .unwrap();
    }

    async fn list(store: &InMemory) -> Vec<String> {
        let mut paths: Vec<String> = store
            .list(None)
            .map_ok(|object| object.location.to_string())
            .try_collect()
            .await
            .unwrap();
        paths.sort();

        paths
    }

    #[tokio::test]
    async fn rebuilds_the_file_list_after_losing_duckdb() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-recover-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        let analytics = open(&dir, "lost.duckdb");
        insert(&analytics, 1);
        let store = Arc::new(InMemory::new());
        let archive = Archive::new(store.clone(), "prod", dir.join("work"));
        archive.export_logs(&analytics).await.unwrap();
        let paths = list(&store).await;
        assert_eq!(paths.len(), 4);
        assert!(paths[1].ends_with(".parquet.json"), "{paths:?}");
        let before = analytics.run(summarize_archive).unwrap();

        let fresh = open(&dir, "fresh.duckdb");
        let done = archive.reconcile(&fresh).await.unwrap();
        assert_eq!(done, Reconcile { adopted: 2, ..Reconcile::default() });
        assert_eq!(fresh.run(summarize_archive).unwrap(), before);

        insert(&fresh, 1);
        assert_eq!(archive.export_logs(&fresh).await.unwrap().files, 0);
        assert!(archive.reconcile(&fresh).await.unwrap().is_empty());

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn describes_files_exported_before_metadata_existed() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-describe-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        let analytics = open(&dir, "daroyan.duckdb");
        insert(&analytics, 1);
        let store = Arc::new(InMemory::new());
        let archive = Archive::new(store.clone(), "prod", dir.join("work"));
        archive.export_logs(&analytics).await.unwrap();
        for path in list(&store).await.iter().filter(|path| path.ends_with(".json")) {
            store.delete(&path.as_str().into()).await.unwrap();
        }
        assert_eq!(list(&store).await.len(), 2);

        let done = archive.reconcile(&analytics).await.unwrap();
        assert_eq!(done, Reconcile { described: 2, ..Reconcile::default() });
        assert_eq!(list(&store).await.len(), 4);

        let fresh = open(&dir, "fresh.duckdb");
        let rebuilt = archive.reconcile(&fresh).await.unwrap();
        assert_eq!(rebuilt.adopted, 2);
        assert_eq!(
            fresh.run(summarize_archive).unwrap(),
            analytics.run(summarize_archive).unwrap()
        );

        std::fs::remove_dir_all(dir).ok();
    }
}
