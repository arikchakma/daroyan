use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use duckdb::types::Value as SqlValue;
use duckdb::{Connection, params_from_iter};

use crate::analytics::archive::{
    ArchivedFile, DAY_MS, FileKind, HOUR_MS, MAX_COLUMNS, PARQUET_OPTIONS, REST_PREFIX,
    StagedFile, in_transaction, insert_files, quote_literal, read_json_list,
};

pub const MAX_DAY_BYTES: u64 = 512 * 1024 * 1024;
const LATE_FILES: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFile {
    pub path: String,
    pub kind: FileKind,
    pub hour: i64,
    pub bytes: u64,
    pub columns: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Merge {
    pub kind: FileKind,
    pub hour: i64,
    pub files: Vec<StoredFile>,
}

impl Merge {
    fn new(kind: FileKind, files: Vec<StoredFile>) -> Self {
        let hour = files.iter().map(|file| file.hour).min().unwrap_or_default();
        let hour = match kind {
            FileKind::Day => (hour * HOUR_MS).div_euclid(DAY_MS) * DAY_MS / HOUR_MS,
            FileKind::Hour => hour,
        };

        Self { kind, hour, files }
    }

    fn fits(files: &[StoredFile], max_columns: usize) -> bool {
        let names: HashSet<String> = files
            .iter()
            .flat_map(|file| &file.columns)
            .filter(|name| !name.starts_with(REST_PREFIX))
            .map(|name| name.to_lowercase())
            .collect();

        names.len() <= max_columns
    }

    fn is_worth_merging(files: &[StoredFile]) -> bool {
        let merged: u64 = files
            .iter()
            .filter(|file| file.kind == FileKind::Day)
            .map(|file| file.bytes)
            .sum();
        let (late_files, late_bytes) = files
            .iter()
            .filter(|file| file.kind == FileKind::Hour)
            .fold((0, 0), |(count, bytes), file| (count + 1, bytes + file.bytes));

        merged == 0 || late_files >= LATE_FILES || late_bytes * 10 >= merged
    }

    pub fn plan(files: Vec<StoredFile>, max_bytes: u64, max_columns: usize) -> Vec<Self> {
        if files.len() < 2 {
            return Vec::new();
        }

        let bytes: u64 = files.iter().map(|file| file.bytes).sum();
        if bytes <= max_bytes && Self::fits(&files, max_columns) {
            if !Self::is_worth_merging(&files) {
                return Vec::new();
            }
            return vec![Self::new(FileKind::Day, files)];
        }

        let mut hours: BTreeMap<i64, Vec<StoredFile>> = BTreeMap::new();
        for file in files.into_iter().filter(|file| file.kind == FileKind::Hour) {
            hours.entry(file.hour).or_default().push(file);
        }

        hours
            .into_values()
            .filter(|files| files.len() > 1 && Self::fits(files, max_columns))
            .map(|files| Self::new(FileKind::Hour, files))
            .collect()
    }

    pub fn paths(&self) -> Vec<String> {
        self.files.iter().map(|file| file.path.clone()).collect()
    }
}

pub fn plan_merges(conn: &Connection, closed_before: i64) -> duckdb::Result<Vec<Merge>> {
    plan_merges_with(conn, closed_before, MAX_DAY_BYTES, MAX_COLUMNS)
}

fn plan_merges_with(
    conn: &Connection,
    closed_before: i64,
    max_bytes: u64,
    max_columns: usize,
) -> duckdb::Result<Vec<Merge>> {
    let mut statement = conn.prepare(
        "SELECT path, kind, min_logged_at, bytes, CAST(to_json(columns) AS VARCHAR) \
         FROM archive_files \
         WHERE deleted_at IS NULL AND (min_logged_at // ? + 1) * ? <= ? \
         ORDER BY min_logged_at, path",
    )?;
    let files = statement
        .query_map([DAY_MS, DAY_MS, closed_before], |row| {
            let kind: String = row.get(1)?;
            let min_logged_at: i64 = row.get(2)?;
            let columns: String = row.get(4)?;

            Ok(StoredFile {
                path: row.get(0)?,
                kind: kind.parse().unwrap_or(FileKind::Hour),
                hour: min_logged_at.div_euclid(HOUR_MS),
                bytes: row.get(3)?,
                columns: read_json_list(&columns),
            })
        })?
        .collect::<duckdb::Result<Vec<_>>>()?;

    let mut days: BTreeMap<i64, Vec<StoredFile>> = BTreeMap::new();
    for file in files {
        days.entry((file.hour * HOUR_MS).div_euclid(DAY_MS))
            .or_default()
            .push(file);
    }

    Ok(days
        .into_values()
        .flat_map(|files| Merge::plan(files, max_bytes, max_columns))
        .collect())
}

pub fn merge_files(
    conn: &Connection,
    merge: &Merge,
    sources: &[PathBuf],
    target: &Path,
) -> duckdb::Result<Option<StagedFile>> {
    if !have_matching_columns(conn, sources)? {
        tracing::warn!(
            files = sources.len(),
            "skipped a merge, the files name or type a column differently"
        );
        return Ok(None);
    }

    let target_text = quote_literal(&target.to_string_lossy());
    let sources = sources
        .iter()
        .map(|source| format!("'{}'", quote_literal(&source.to_string_lossy())))
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute_batch(&format!(
        "COPY (SELECT * FROM read_parquet([{sources}], union_by_name = true) \
         ORDER BY service, logged_at) TO '{target_text}' ({PARQUET_OPTIONS})"
    ))?;

    let (min_logged_at, max_logged_at, rows, services) = conn.query_row(
        &format!(
            "SELECT min(logged_at), max(logged_at), count(*), \
             CAST(to_json(list_sort(list_distinct(list(service)))) AS VARCHAR) \
             FROM read_parquet('{target_text}')"
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, String>(3)?)),
    )?;
    let mut statement = conn.prepare(&format!(
        "SELECT column_name FROM (DESCRIBE SELECT * FROM read_parquet('{target_text}')) \
         WHERE column_name LIKE 'a.%' OR column_name LIKE 'r.%' OR column_name LIKE '{REST_PREFIX}%'"
    ))?;
    let columns = statement
        .query_map([], |row| row.get(0))?
        .collect::<duckdb::Result<_>>()?;

    Ok(Some(StagedFile {
        local: target.to_path_buf(),
        name: target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        kind: merge.kind,
        hour: merge.hour,
        min_logged_at,
        max_logged_at,
        rows,
        services: read_json_list(&services),
        columns,
    }))
}

fn have_matching_columns(conn: &Connection, sources: &[PathBuf]) -> duckdb::Result<bool> {
    let mut seen: HashMap<String, (String, String)> = HashMap::new();
    for source in sources {
        let mut statement = conn.prepare(&format!(
            "SELECT column_name, column_type FROM (DESCRIBE SELECT * FROM read_parquet('{}'))",
            quote_literal(&source.to_string_lossy())
        ))?;
        let columns = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<duckdb::Result<Vec<_>>>()?;

        for (name, kind) in columns {
            let column = (name.clone(), kind);
            match seen.get(&name.to_lowercase()) {
                Some(known) if *known != column => return Ok(false),
                Some(_) => {}
                None => {
                    seen.insert(name.to_lowercase(), column);
                }
            }
        }
    }

    Ok(true)
}

pub fn replace_files(
    conn: &Connection,
    merged: &ArchivedFile,
    old: &[String],
    tenant: &str,
    now_ms: i64,
) -> duckdb::Result<()> {
    in_transaction(conn, || {
        insert_files(conn, std::slice::from_ref(merged), tenant, now_ms)?;
        mark_deleted(conn, old, now_ms)
    })
}

fn mark_deleted(conn: &Connection, paths: &[String], now_ms: i64) -> duckdb::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let marks = vec!["?"; paths.len()].join(", ");
    let values = std::iter::once(SqlValue::BigInt(now_ms))
        .chain(paths.iter().cloned().map(SqlValue::Text));
    conn.execute(
        &format!("UPDATE archive_files SET deleted_at = ? WHERE path IN ({marks})"),
        params_from_iter(values),
    )?;

    Ok(())
}

pub fn expire_files(
    conn: &Connection,
    before_ms: i64,
    now_ms: i64,
) -> duckdb::Result<usize> {
    conn.execute(
        "UPDATE archive_files SET deleted_at = ? WHERE deleted_at IS NULL AND max_logged_at < ?",
        [now_ms, before_ms],
    )
}

pub fn list_deleted(
    conn: &Connection,
    deleted_before: i64,
) -> duckdb::Result<Vec<String>> {
    let mut statement = conn
        .prepare("SELECT path FROM archive_files WHERE deleted_at <= ? ORDER BY path")?;

    statement.query_map([deleted_before], |row| row.get(0))?.collect()
}

pub fn forget_files(conn: &Connection, paths: &[String]) -> duckdb::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let marks = vec!["?"; paths.len()].join(", ");
    conn.execute(
        &format!("DELETE FROM archive_files WHERE path IN ({marks})"),
        params_from_iter(paths),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use duckdb::Connection;

    use super::{
        Merge, StoredFile, expire_files, forget_files, list_deleted, merge_files,
        plan_merges_with, replace_files,
    };
    use crate::analytics::archive::{
        ArchivedFile, DAY_MS, FileKind, HOUR_MS, create_log_tables,
    };

    const DAY: i64 = 1_794_009_600_000 / DAY_MS * DAY_MS;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-upkeep-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();

        dir
    }

    fn open(dir: &std::path::Path) -> Connection {
        let conn = Connection::open(dir.join("daroyan.duckdb")).unwrap();
        create_log_tables(&conn).unwrap();

        conn
    }

    fn stored(
        path: &str,
        kind: FileKind,
        hour: i64,
        bytes: u64,
        columns: &[&str],
    ) -> StoredFile {
        StoredFile {
            path: path.to_string(),
            kind,
            hour: DAY / HOUR_MS + hour,
            bytes,
            columns: columns.iter().map(ToString::to_string).collect(),
        }
    }

    fn record(
        conn: &Connection,
        path: &str,
        kind: &str,
        at: i64,
        bytes: u64,
        deleted_at: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO archive_files VALUES (?, 'default', ?, ?, ?, 1, ?, ['api'], ['a.x'], 0, ?)",
            duckdb::params![path, kind, at, at + 10, bytes, deleted_at],
        )
        .unwrap();
    }

    #[test]
    fn merges_a_small_day_into_one_file() {
        let files = vec![
            stored("h00", FileKind::Hour, 0, 10, &["a.x"]),
            stored("h05", FileKind::Hour, 5, 10, &["a.y", "_rest_attributes"]),
        ];
        let merges = Merge::plan(files, 100, 10);

        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].kind, FileKind::Day);
        assert_eq!(merges[0].hour, DAY / HOUR_MS);
        assert_eq!(merges[0].paths(), ["h00", "h05"]);
    }

    #[test]
    fn leaves_a_merged_day_alone_until_enough_late_lines_arrive() {
        let day = || stored("day", FileKind::Day, 0, 1_000, &["a.x"]);
        let late = |bytes| stored("late", FileKind::Hour, 9, bytes, &["a.x"]);

        assert_eq!(Merge::plan(vec![day(), late(10)], 10_000, 10), []);
        assert_eq!(Merge::plan(vec![day(), late(100)], 10_000, 10).len(), 1);
    }

    #[test]
    fn merges_by_hour_when_the_day_is_too_big() {
        let files = vec![
            stored("day", FileKind::Day, 0, 80, &["a.x"]),
            stored("h03a", FileKind::Hour, 3, 20, &["a.x"]),
            stored("h03b", FileKind::Hour, 3, 20, &["a.x"]),
            stored("h04", FileKind::Hour, 4, 20, &["a.x"]),
        ];
        let merges = Merge::plan(files, 100, 10);

        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].kind, FileKind::Hour);
        assert_eq!(merges[0].hour, DAY / HOUR_MS + 3);
        assert_eq!(merges[0].paths(), ["h03a", "h03b"]);
    }

    #[test]
    fn skips_files_whose_columns_would_pass_the_cap() {
        let files = vec![
            stored("h01a", FileKind::Hour, 1, 1, &["a.x", "a.y"]),
            stored("h01b", FileKind::Hour, 1, 1, &["a.z", "a.X"]),
        ];

        assert_eq!(Merge::plan(files, 100, 2), []);
        assert_eq!(
            Merge::plan(vec![stored("one", FileKind::Hour, 1, 1, &[])], 100, 2),
            []
        );
    }

    #[test]
    fn plans_only_days_that_ended_before_the_cutoff() {
        let dir = scratch("plan");
        let conn = open(&dir);
        record(&conn, "d1-a", "hour", DAY + HOUR_MS, 10, None);
        record(&conn, "d1-b", "hour", DAY + 2 * HOUR_MS, 10, None);
        record(&conn, "d1-gone", "hour", DAY + 3 * HOUR_MS, 10, Some(5));
        record(&conn, "d2-a", "hour", DAY + DAY_MS + HOUR_MS, 10, None);
        record(&conn, "d2-b", "hour", DAY + DAY_MS + 2 * HOUR_MS, 10, None);

        let merges =
            plan_merges_with(&conn, DAY + DAY_MS + 6 * HOUR_MS, 100, 10).unwrap();
        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].paths(), ["d1-a", "d1-b"]);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn merges_files_and_replaces_them() {
        let dir = scratch("merge");
        let conn = open(&dir);
        let first = dir.join("first.parquet");
        let second = dir.join("second.parquet");
        conn.execute_batch(&format!(
            "COPY (SELECT 'srv_1' AS server_id, {DAY} + 5 AS logged_at, 'web' AS service, 1::BIGINT AS \"a.x\") TO '{}'; \
             COPY (SELECT 'srv_1' AS server_id, {DAY} + 1 AS logged_at, 'api' AS service, 2::BIGINT AS \"a.x\", '{{}}' AS _rest_attributes) TO '{}';",
            first.display(),
            second.display()
        ))
        .unwrap();
        record(&conn, "first", "hour", DAY + 5, 10, None);
        record(&conn, "second", "hour", DAY + 1, 10, None);

        let merge = Merge {
            kind: FileKind::Day,
            hour: DAY / HOUR_MS,
            files: Vec::new(),
        };
        let target = dir.join("day-1.parquet");
        let clash = dir.join("clash.parquet");
        conn.execute_batch(&format!(
            "COPY (SELECT 'srv_1' AS server_id, {DAY} + 7 AS logged_at, 'api' AS service, 'one' AS \"a.x\") TO '{}';",
            clash.display()
        ))
        .unwrap();
        let skipped = merge_files(
            &conn,
            &merge,
            &[first.clone(), clash],
            &dir.join("skip.parquet"),
        )
        .unwrap();
        assert!(skipped.is_none());

        let file = merge_files(&conn, &merge, &[first, second], &target)
            .unwrap()
            .unwrap();
        assert_eq!(
            (file.min_logged_at, file.max_logged_at, file.rows),
            (DAY + 1, DAY + 5, 2)
        );
        assert_eq!(file.services, ["api", "web"]);
        assert_eq!(file.columns, ["a.x", "_rest_attributes"]);
        assert_eq!(file.name, "day-1.parquet");

        let order: String = conn
            .query_row(
                &format!(
                    "SELECT string_agg(service, ',') FROM read_parquet('{}')",
                    target.display()
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(order, "api,web");

        let merged = ArchivedFile { path: "day".to_string(), bytes: 15, file };
        replace_files(&conn, &merged, &["first".into(), "second".into()], "default", 700)
            .unwrap();
        let rows: Vec<(String, String, Option<i64>)> = conn
            .prepare("SELECT path, kind, deleted_at FROM archive_files ORDER BY path")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                ("day".to_string(), "day".to_string(), None),
                ("first".to_string(), "hour".to_string(), Some(700)),
                ("second".to_string(), "hour".to_string(), Some(700)),
            ]
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn expires_old_files_and_lists_them_after_the_delay() {
        let dir = scratch("expire");
        let conn = open(&dir);
        record(&conn, "old", "hour", 100, 10, None);
        record(&conn, "new", "hour", 900, 10, None);

        assert_eq!(expire_files(&conn, 500, 1_000).unwrap(), 1);
        assert_eq!(list_deleted(&conn, 999).unwrap(), Vec::<String>::new());
        assert_eq!(list_deleted(&conn, 1_000).unwrap(), ["old"]);

        forget_files(&conn, &["old".to_string()]).unwrap();
        assert_eq!(list_deleted(&conn, 2_000).unwrap(), Vec::<String>::new());

        fs::remove_dir_all(dir).ok();
    }
}
