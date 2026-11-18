mod upkeep;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use object_store::aws::AmazonS3Builder;
use object_store::buffered::BufWriter;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::analytics::Analytics;
use crate::analytics::archive::export::stage_logs;
use crate::analytics::archive::{
    ArchivedFile, FileKind, HOUR_MS, StagedFile, record_files,
};
use crate::config::ArchiveConfig;
use crate::error::Result;

pub use upkeep::Upkeep;

pub const TENANT: &str = "default";
const EXPORT_INTERVAL: Duration = Duration::from_secs(60 * 60);
const EXPORT_SETTLE_MS: i64 = 60_000;
const UPKEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const UPKEEP_DELAY: Duration = Duration::from_secs(10 * 60);
const CHECK_OBJECT: &str = ".daroyan-check";
const DEFAULT_RETENTION_DAYS: u32 = 90;

#[derive(Clone)]
pub struct Archive {
    store: Arc<dyn ObjectStore>,
    root: String,
    work_dir: PathBuf,
    retention_days: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Export {
    pub files: usize,
    pub rows: i64,
    pub bytes: u64,
}

impl TryFrom<&ArchiveConfig> for Archive {
    type Error = object_store::Error;

    fn try_from(config: &ArchiveConfig) -> object_store::Result<Self> {
        rustls::crypto::ring::default_provider().install_default().ok();

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&config.bucket)
            .with_region(&config.region);
        if let Some(endpoint) = &config.endpoint {
            builder = builder
                .with_endpoint(endpoint)
                .with_allow_http(endpoint.starts_with("http://"));
        }
        if let (Some(key_id), Some(secret)) =
            (&config.access_key_id, &config.secret_access_key)
        {
            builder = builder.with_access_key_id(key_id).with_secret_access_key(secret);
        }

        Ok(Self::new(
            Arc::new(builder.build()?),
            &config.prefix,
            config.cache_dir.clone(),
        )
        .with_retention_days(config.retention_days))
    }
}

impl Archive {
    pub fn new(store: Arc<dyn ObjectStore>, prefix: &str, work_dir: PathBuf) -> Self {
        Self {
            store,
            root: prefix.trim_matches('/').to_string(),
            work_dir,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }

    pub fn with_retention_days(mut self, retention_days: u32) -> Self {
        self.retention_days = retention_days;
        self
    }

    fn object_path(&self, relative: &str) -> Path {
        if self.root.is_empty() {
            return Path::from(relative);
        }

        Path::from(format!("{}/{relative}", self.root))
    }

    fn log_path(&self, file: &StagedFile) -> Path {
        let start = DateTime::<Utc>::from_timestamp_millis(file.hour * HOUR_MS)
            .unwrap_or_default();

        let day = start.format("%Y-%m-%d");

        match file.kind {
            FileKind::Hour => self.object_path(&format!(
                "logs/v1/tenant={TENANT}/dt={day}/hour={}/{}",
                start.format("%H"),
                file.name
            )),
            FileKind::Day => self
                .object_path(&format!("logs/v1/tenant={TENANT}/dt={day}/{}", file.name)),
        }
    }

    fn logs_root(&self) -> Path {
        self.object_path(&format!("logs/v1/tenant={TENANT}"))
    }

    pub async fn check(&self) -> Result<()> {
        let path = self.object_path(CHECK_OBJECT);
        self.store.put(&path, "ok".into()).await?;
        self.store.delete(&path).await?;

        Ok(())
    }

    pub async fn export_logs(&self, analytics: &Analytics) -> Result<Export> {
        let staging = reset_dir(self.work_dir.join("export")).await?;
        let dir = staging.clone();
        let until = Utc::now().timestamp_millis() - EXPORT_SETTLE_MS;
        let staged = analytics.read(move |conn| stage_logs(conn, &dir, until)).await?;

        let mut archived = Vec::with_capacity(staged.files.len());
        let mut uploaded = Ok(());
        for file in staged.files {
            if uploaded.is_ok() {
                match self.upload(&file).await {
                    Ok((path, bytes)) => archived.push(ArchivedFile {
                        path: path.to_string(),
                        bytes,
                        file,
                    }),
                    Err(error) => uploaded = Err(error),
                }
            }
        }
        tokio::fs::remove_dir_all(&staging).await.ok();
        uploaded?;

        let export = Export {
            files: archived.len(),
            rows: archived.iter().map(|archived| archived.file.rows).sum(),
            bytes: archived.iter().map(|archived| archived.bytes).sum(),
        };
        let until = staged.until;
        analytics
            .write(move |conn| {
                record_files(
                    conn,
                    &archived,
                    TENANT,
                    until,
                    Utc::now().timestamp_millis(),
                )
            })
            .await?;

        Ok(export)
    }

    async fn upload(&self, file: &StagedFile) -> Result<(Path, u64)> {
        let path = self.log_path(file);
        let mut source = tokio::fs::File::open(&file.local).await?;
        let bytes = source.metadata().await?.len();

        let mut writer = BufWriter::new(Arc::clone(&self.store), path.clone());
        let copied = tokio::io::copy(&mut source, &mut writer).await;
        let finished = match copied {
            Ok(_) => writer.shutdown().await,
            Err(error) => Err(error),
        };
        if let Err(error) = finished {
            writer.abort().await.ok();
            return Err(error.into());
        }

        Ok((path, bytes))
    }

    async fn download(&self, path: &Path, local: &std::path::Path) -> Result<()> {
        let mut stream = self.store.get(path).await?.into_stream();
        let mut file = tokio::fs::File::create(local).await?;
        while let Some(chunk) = stream.try_next().await? {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;

        Ok(())
    }
}

async fn reset_dir(dir: PathBuf) -> Result<PathBuf> {
    tokio::fs::remove_dir_all(&dir).await.ok();
    tokio::fs::create_dir_all(&dir).await?;

    Ok(dir)
}

pub fn spawn_archive_task(archive: Archive, analytics: Analytics) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = archive.check().await {
            tracing::error!(
                %error,
                "cannot write to the archive bucket, check ARCHIVE_URL and its keys"
            );
        }

        let mut export = tokio::time::interval(EXPORT_INTERVAL);
        export.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut upkeep = tokio::time::interval_at(
            tokio::time::Instant::now() + UPKEEP_DELAY,
            UPKEEP_INTERVAL,
        );
        upkeep.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = export.tick() => match archive.export_logs(&analytics).await {
                    Ok(done) => tracing::info!(
                        files = done.files,
                        rows = done.rows,
                        bytes = done.bytes,
                        "archived logs"
                    ),
                    Err(error) => tracing::error!(%error, "archive logs"),
                },
                _ = upkeep.tick() => match archive.tidy(&analytics).await {
                    Ok(done) => tracing::info!(
                        merged = done.merged,
                        expired = done.expired,
                        deleted = done.deleted,
                        orphans = done.orphans,
                        "tidied the log archive"
                    ),
                    Err(error) => tracing::error!(%error, "tidy the log archive"),
                },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::TryStreamExt;
    use object_store::ObjectStore;
    use object_store::memory::InMemory;

    use super::Archive;
    use crate::analytics::Analytics;
    use crate::analytics::archive::create_log_tables;

    #[tokio::test]
    async fn uploads_hourly_files_and_records_them() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-upload-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        analytics
            .run(|conn| {
                create_log_tables(conn)?;
                conn.execute_batch(
                    "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, ingested_at) \
                     VALUES ('srv_1', 1794002400000, 'api', 'otlp', 9, 'done', '{\"user.id\":\"u_1\"}', 1), \
                            ('srv_1', 1794006000000, 'api', 'otlp', 9, 'done', NULL, 1);",
                )
            })
            .unwrap();

        let store = Arc::new(InMemory::new());
        let archive = Archive::new(store.clone(), "/prod/", dir.join("work"));
        archive.check().await.unwrap();

        let export = archive.export_logs(&analytics).await.unwrap();
        assert_eq!((export.files, export.rows), (2, 2));
        assert!(export.bytes > 0);
        assert!(!dir.join("work").join("export").exists());

        let mut paths: Vec<String> = store
            .list(None)
            .map_ok(|meta| meta.location.to_string())
            .try_collect()
            .await
            .unwrap();
        paths.sort();
        assert_eq!(paths.len(), 2);
        assert!(
            paths[0].starts_with("prod/logs/v1/tenant=default/dt=2026-11-06/hour=22/"),
            "{paths:?}"
        );
        assert!(
            paths[1].starts_with("prod/logs/v1/tenant=default/dt=2026-11-06/hour=23/")
        );

        let recorded: Vec<String> = analytics
            .run(|conn| {
                conn.prepare("SELECT path FROM archive_files ORDER BY path")?
                    .query_map([], |row| row.get(0))?
                    .collect()
            })
            .unwrap();
        assert_eq!(recorded, paths);

        let again = archive.export_logs(&analytics).await.unwrap();
        assert_eq!(again.files, 0);

        std::fs::remove_dir_all(dir).ok();
    }
}
