use std::collections::{HashMap, HashSet};

use duckdb::Connection;

use crate::analytics::archive::upkeep::mark_deleted;
use crate::analytics::archive::{
    ArchivedFile, FileKind, FileMeta, in_transaction, insert_files, read_json_list,
    read_watermark, write_watermark,
};

#[derive(Debug, Default)]
pub struct KnownFiles {
    pub live: Vec<ArchivedFile>,
    pub deleted: HashSet<String>,
    pub watermark: i64,
}

impl KnownFiles {
    pub fn paths(&self) -> HashSet<&str> {
        self.live
            .iter()
            .map(|file| file.path.as_str())
            .chain(self.deleted.iter().map(String::as_str))
            .collect()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recovery {
    pub adopted: Vec<ArchivedFile>,
    pub superseded: Vec<ArchivedFile>,
    pub replaced: Vec<String>,
    pub watermark: i64,
}

impl Recovery {
    pub fn plan(mut found: Vec<ArchivedFile>, known: &KnownFiles) -> Self {
        found.sort_by(|a, b| {
            (a.meta.created_at, &a.path).cmp(&(b.meta.created_at, &b.path))
        });

        let mut live: HashSet<String> =
            known.live.iter().map(|file| file.path.clone()).collect();
        let mut deleted = known.deleted.clone();
        let mut adopted: HashMap<String, ArchivedFile> = HashMap::new();
        let mut recovery = Self { watermark: known.watermark, ..Self::default() };

        for file in found {
            let replaces = &file.meta.replaces;
            let is_stale = if replaces.is_empty() {
                file.meta.exported_until <= known.watermark
            } else {
                replaces.iter().any(|path| deleted.contains(path))
            };
            if is_stale {
                deleted.insert(file.path.clone());
                recovery.superseded.push(file);
                continue;
            }

            for path in replaces {
                deleted.insert(path.clone());
                if live.remove(path) {
                    recovery.replaced.push(path.clone());
                }
                if let Some(old) = adopted.remove(path) {
                    recovery.superseded.push(old);
                }
            }

            recovery.watermark = recovery.watermark.max(file.meta.exported_until);
            adopted.insert(file.path.clone(), file);
        }

        recovery.adopted = adopted.into_values().collect();
        recovery.adopted.sort_by(|a, b| a.path.cmp(&b.path));

        recovery
    }

    pub fn is_empty(&self) -> bool {
        self.adopted.is_empty() && self.superseded.is_empty() && self.replaced.is_empty()
    }
}

pub fn list_known_files(conn: &Connection) -> duckdb::Result<KnownFiles> {
    let watermark = read_watermark(conn)?;
    let mut statement = conn.prepare(
        "SELECT path, kind, min_logged_at, max_logged_at, rows, bytes, \
         CAST(to_json(services) AS VARCHAR), CAST(to_json(columns) AS VARCHAR), created_at, deleted_at \
         FROM archive_files ORDER BY path",
    )?;
    let mut rows = statement.query([])?;

    let mut known = KnownFiles { watermark, ..KnownFiles::default() };
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        if row.get::<_, Option<i64>>(9)?.is_some() {
            known.deleted.insert(path);
            continue;
        }

        let kind: String = row.get(1)?;
        known.live.push(ArchivedFile {
            path,
            meta: FileMeta {
                kind: kind.parse().unwrap_or(FileKind::Hour),
                min_logged_at: row.get(2)?,
                max_logged_at: row.get(3)?,
                rows: row.get(4)?,
                bytes: row.get(5)?,
                services: read_json_list(&row.get::<_, String>(6)?),
                columns: read_json_list(&row.get::<_, String>(7)?),
                exported_until: watermark,
                replaces: Vec::new(),
                created_at: row.get(8)?,
            },
        });
    }

    Ok(known)
}

pub fn apply_recovery(
    conn: &Connection,
    recovery: &Recovery,
    tenant: &str,
    now_ms: i64,
) -> duckdb::Result<()> {
    let superseded: Vec<String> =
        recovery.superseded.iter().map(|file| file.path.clone()).collect();

    in_transaction(conn, || {
        insert_files(conn, &recovery.adopted, tenant)?;
        insert_files(conn, &recovery.superseded, tenant)?;
        mark_deleted(conn, &superseded, now_ms)?;
        mark_deleted(conn, &recovery.replaced, now_ms)?;
        write_watermark(conn, recovery.watermark)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use duckdb::Connection;

    use super::{KnownFiles, Recovery, apply_recovery, list_known_files};
    use crate::analytics::archive::{
        ArchivedFile, FileKind, FileMeta, create_log_tables, read_watermark, record_files,
    };

    fn file(
        path: &str,
        exported_until: i64,
        replaces: &[&str],
        created_at: i64,
    ) -> ArchivedFile {
        ArchivedFile {
            path: path.to_string(),
            meta: FileMeta {
                kind: if replaces.is_empty() { FileKind::Hour } else { FileKind::Day },
                min_logged_at: 10,
                max_logged_at: 20,
                rows: 1,
                bytes: 100,
                services: vec!["api".to_string()],
                columns: vec!["a.x".to_string()],
                exported_until,
                replaces: replaces.iter().map(|path| path.to_string()).collect(),
                created_at,
            },
        }
    }

    fn paths(files: &[ArchivedFile]) -> Vec<&str> {
        files.iter().map(|file| file.path.as_str()).collect()
    }

    #[test]
    fn rebuilds_an_empty_list_from_the_bucket() {
        let found = vec![
            file("h1", 100, &[], 1),
            file("h2", 100, &[], 1),
            file("d1", 100, &["h1", "h2"], 2),
            file("h3", 200, &[], 3),
            file("d2", 200, &["d1", "h3"], 4),
            file("h4", 300, &[], 5),
        ];

        let recovery = Recovery::plan(found, &KnownFiles::default());

        assert_eq!(paths(&recovery.adopted), ["d2", "h4"]);
        assert_eq!(paths(&recovery.superseded), ["h1", "h2", "d1", "h3"]);
        assert!(recovery.replaced.is_empty());
        assert_eq!(recovery.watermark, 300);
    }

    #[test]
    fn catches_up_a_list_restored_from_a_backup() {
        let known = KnownFiles {
            live: vec![file("h1", 100, &[], 1), file("h2", 100, &[], 1)],
            deleted: HashSet::new(),
            watermark: 100,
        };
        let found =
            vec![file("h3", 200, &[], 3), file("d1", 200, &["h1", "h2", "h3"], 4)];

        let recovery = Recovery::plan(found, &known);

        assert_eq!(paths(&recovery.adopted), ["d1"]);
        assert_eq!(paths(&recovery.superseded), ["h3"]);
        assert_eq!(recovery.replaced, ["h1", "h2"]);
        assert_eq!(recovery.watermark, 200);
    }

    #[test]
    fn drops_files_that_a_recorded_export_or_merge_already_covers() {
        let known = KnownFiles {
            live: vec![file("d2", 200, &["h1", "h2"], 5)],
            deleted: HashSet::from(["h1".to_string(), "h2".to_string()]),
            watermark: 200,
        };
        let found = vec![file("retry", 150, &[], 2), file("d1", 150, &["h1", "h2"], 3)];

        let recovery = Recovery::plan(found, &known);

        assert!(recovery.adopted.is_empty());
        assert_eq!(paths(&recovery.superseded), ["retry", "d1"]);
        assert_eq!(recovery.watermark, 200);
    }

    #[test]
    fn records_adopted_files_and_raises_the_watermark() {
        let conn = Connection::open_in_memory().unwrap();
        create_log_tables(&conn).unwrap();
        record_files(&conn, &[file("h1", 100, &[], 1)], "default", 100).unwrap();

        let known = list_known_files(&conn).unwrap();
        assert_eq!(paths(&known.live), ["h1"]);
        assert_eq!(known.live[0].meta.exported_until, 100);

        let found = vec![file("old", 50, &[], 0), file("d1", 300, &["h1"], 2)];
        let recovery = Recovery::plan(found, &known);
        apply_recovery(&conn, &recovery, "default", 900).unwrap();

        let rows: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT path, deleted_at FROM archive_files ORDER BY path")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                ("d1".to_string(), None),
                ("h1".to_string(), Some(900)),
                ("old".to_string(), Some(900)),
            ]
        );
        assert_eq!(read_watermark(&conn).unwrap(), 300);
    }
}
