pub mod export;
pub mod search;
pub mod upkeep;

use std::path::PathBuf;
use std::str::FromStr;

use duckdb::{Connection, params};
use serde_json::Value;

pub const LOG_WATERMARK: &str = "log_watermark";
pub const IS_ARCHIVED: &str = "ingested_at <= coalesce((SELECT value FROM archive_state WHERE name = 'log_watermark'), 0)";
pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const DAY_MS: i64 = 24 * HOUR_MS;
pub const MAX_COLUMNS: usize = 1_000;
pub const REST_PREFIX: &str = "_rest_";
const PARQUET_OPTIONS: &str =
    "FORMAT parquet, COMPRESSION zstd, COMPRESSION_LEVEL 3, ROW_GROUP_SIZE 122880";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Hour,
    Day,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }
}

impl FromStr for FileKind {
    type Err = String;

    fn from_str(kind: &str) -> Result<Self, Self::Err> {
        match kind {
            "hour" => Ok(Self::Hour),
            "day" => Ok(Self::Day),
            _ => Err(format!("unknown archive file kind: {kind}")),
        }
    }
}

#[derive(Debug)]
pub struct StagedFile {
    pub local: PathBuf,
    pub name: String,
    pub kind: FileKind,
    pub hour: i64,
    pub min_logged_at: i64,
    pub max_logged_at: i64,
    pub rows: i64,
    pub services: Vec<String>,
    pub columns: Vec<String>,
}

#[derive(Debug)]
pub struct ArchivedFile {
    pub path: String,
    pub bytes: u64,
    pub file: StagedFile,
}

pub fn read_watermark(conn: &Connection) -> duckdb::Result<i64> {
    let value: Option<i64> = conn.query_row(
        "SELECT max(value) FROM archive_state WHERE name = ?",
        [LOG_WATERMARK],
        |row| row.get(0),
    )?;

    Ok(value.unwrap_or_default())
}

pub fn record_files(
    conn: &Connection,
    files: &[ArchivedFile],
    tenant: &str,
    until: i64,
    now_ms: i64,
) -> duckdb::Result<()> {
    in_transaction(conn, || {
        insert_files(conn, files, tenant, now_ms)?;
        conn.execute(
            "INSERT OR REPLACE INTO archive_state (name, value) VALUES (?, ?)",
            params![LOG_WATERMARK, until],
        )?;

        Ok(())
    })
}

fn in_transaction(
    conn: &Connection,
    work: impl FnOnce() -> duckdb::Result<()>,
) -> duckdb::Result<()> {
    conn.execute_batch("BEGIN TRANSACTION")?;

    if let Err(error) = work() {
        conn.execute_batch("ROLLBACK").ok();
        return Err(error);
    }

    conn.execute_batch("COMMIT")
}

fn insert_files(
    conn: &Connection,
    files: &[ArchivedFile],
    tenant: &str,
    now_ms: i64,
) -> duckdb::Result<()> {
    let mut statement = conn.prepare(
        "INSERT INTO archive_files \
         (path, tenant, kind, min_logged_at, max_logged_at, rows, bytes, services, columns, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, CAST(CAST(? AS JSON) AS VARCHAR[]), CAST(CAST(? AS JSON) AS VARCHAR[]), ?)",
    )?;

    for archived in files {
        let file = &archived.file;
        statement.execute(params![
            archived.path,
            tenant,
            file.kind.as_str(),
            file.min_logged_at,
            file.max_logged_at,
            file.rows,
            archived.bytes,
            Value::from(file.services.clone()).to_string(),
            Value::from(file.columns.clone()).to_string(),
            now_ms,
        ])?;
    }

    Ok(())
}

fn read_json_list(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

fn quote_literal(text: &str) -> String {
    text.replace('\'', "''")
}

pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
pub fn create_log_tables(conn: &Connection) -> duckdb::Result<()> {
    for sql in [
        include_str!("../../../../daroyan-migration/duckdb/0002_log_lines.sql"),
        include_str!("../../../../daroyan-migration/duckdb/0003_log_raw.sql"),
        include_str!("../../../../daroyan-migration/duckdb/0011_log_resource.sql"),
        include_str!("../../../../daroyan-migration/duckdb/0012_log_archive.sql"),
        include_str!("../../../../daroyan-migration/duckdb/0014_log_volume.sql"),
    ] {
        conn.execute_batch(sql)?;
    }

    Ok(())
}
