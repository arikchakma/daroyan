use std::path::PathBuf;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::analytics::{Analytics, Backup};
use crate::archive::Archive;
use crate::error::Result;

const BACKUP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone)]
pub struct Backups {
    dir: Option<PathBuf>,
    archive: Option<Archive>,
}

impl Backups {
    pub fn new(dir: Option<PathBuf>, archive: Option<Archive>) -> Self {
        Self { dir, archive }
    }

    pub fn is_enabled(&self) -> bool {
        self.dir.is_some() || self.archive.is_some()
    }

    pub async fn back_up(&self, analytics: &Analytics) -> Result<Option<Backup>> {
        let Some(dir) = self.local_dir() else { return Ok(None) };
        let backup = analytics.back_up(dir.clone()).await?;

        if let Some(archive) = &self.archive {
            let local = dir.join(&backup.name);
            archive.upload_backup(&local, &backup.name).await?;
            if self.dir.is_none() {
                tokio::fs::remove_file(&local).await.ok();
            }
        }

        Ok(Some(backup))
    }

    fn local_dir(&self) -> Option<PathBuf> {
        self.dir.clone().or_else(|| Some(self.archive.as_ref()?.backup_dir()))
    }
}

pub fn spawn_backup_task(backups: Backups, analytics: Analytics) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval_at(
            tokio::time::Instant::now() + BACKUP_INTERVAL,
            BACKUP_INTERVAL,
        );

        loop {
            tick.tick().await;
            match backups.back_up(&analytics).await {
                Ok(Some(backup)) => {
                    tracing::info!(name = %backup.name, "backed up analytics");
                }
                Ok(None) => {}
                Err(error) => tracing::error!(%error, "back up analytics"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::TryStreamExt;
    use object_store::memory::InMemory;
    use object_store::{ObjectStore, ObjectStoreExt};

    use super::Backups;
    use crate::analytics::Analytics;
    use crate::archive::Archive;

    #[tokio::test]
    async fn uploads_backups_to_the_archive_and_keeps_the_newest_two() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-backup-upload-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        let analytics = Analytics::open(&dir.join("daroyan.duckdb")).unwrap();
        let store = Arc::new(InMemory::new());
        for name in
            ["daroyan-2026-01-01T000000.duckdb", "daroyan-2026-01-02T000000.duckdb"]
        {
            store
                .put(&format!("prod/backups/{name}").into(), "old".into())
                .await
                .unwrap();
        }
        let archive = Archive::new(store.clone(), "prod", dir.join("work"));

        assert!(!Backups::new(None, None).is_enabled());
        assert!(Backups::new(None, None).back_up(&analytics).await.unwrap().is_none());

        let backups = Backups::new(None, Some(archive.clone()));
        let backup = backups.back_up(&analytics).await.unwrap().unwrap();

        let mut paths: Vec<String> = store
            .list(None)
            .map_ok(|object| object.location.to_string())
            .try_collect()
            .await
            .unwrap();
        paths.sort();
        assert_eq!(
            paths,
            [
                "prod/backups/daroyan-2026-01-02T000000.duckdb".to_string(),
                format!("prod/backups/{}", backup.name),
            ]
        );
        assert!(!archive.backup_dir().join(&backup.name).exists());

        let local = Backups::new(Some(dir.join("local")), Some(archive));
        let kept = local.back_up(&analytics).await.unwrap().unwrap();
        assert!(dir.join("local").join(&kept.name).exists());

        std::fs::remove_dir_all(dir).ok();
    }
}
