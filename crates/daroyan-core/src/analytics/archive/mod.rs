pub mod export;
pub mod recover;
pub mod search;
pub mod upkeep;

use std::path::PathBuf;
use std::str::FromStr;

use duckdb::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LOG_WATERMARK: &str = "log_watermark";
pub const IS_ARCHIVED: &str = "ingested_at <= coalesce((SELECT value FROM archive_state WHERE name = 'log_watermark'), 0)";
pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const DAY_MS: i64 = 24 * HOUR_MS;
pub const MAX_COLUMNS: usize = 1_000;
pub const REST_PREFIX: &str = "_rest_";
const PARQUET_OPTIONS: &str =
    "FORMAT parquet, COMPRESSION zstd, COMPRESSION_LEVEL 3, ROW_GROUP_SIZE 30720";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    pub exported_until: i64,
    pub replaces: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub kind: FileKind,
    pub min_logged_at: i64,
    pub max_logged_at: i64,
    pub rows: i64,
    pub bytes: u64,
    pub services: Vec<String>,
    pub columns: Vec<String>,
    pub exported_until: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
    pub created_at: i64,
}

impl FileMeta {
    pub fn new(file: StagedFile, bytes: u64, created_at: i64) -> Self {
        Self {
            kind: file.kind,
            min_logged_at: file.min_logged_at,
            max_logged_at: file.max_logged_at,
            rows: file.rows,
            bytes,
            services: file.services,
            columns: file.columns,
            exported_until: file.exported_until,
            replaces: file.replaces,
            created_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedFile {
    pub path: String,
    pub meta: FileMeta,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveSummary {
    pub files: u64,
    pub bytes: u64,
    pub oldest_at: Option<i64>,
    pub exported_at: Option<i64>,
}

pub fn summarize_archive(conn: &Connection) -> duckdb::Result<ArchiveSummary> {
    let (files, bytes, oldest_at) = conn.query_row(
        "SELECT count(*), coalesce(sum(bytes), 0), min(min_logged_at) \
         FROM archive_files WHERE deleted_at IS NULL",
        [],
        |row| Ok((row.get(0)?, row.get::<_, i128>(1)?, row.get(2)?)),
    )?;
    let watermark = read_watermark(conn)?;

    Ok(ArchiveSummary {
        files,
        bytes: u64::try_from(bytes).unwrap_or_default(),
        oldest_at,
        exported_at: (watermark > 0).then_some(watermark),
    })
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
) -> duckdb::Result<()> {
    in_transaction(conn, || {
        insert_files(conn, files, tenant)?;
        write_watermark(conn, until)
    })
}

fn write_watermark(conn: &Connection, until: i64) -> duckdb::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO archive_state (name, value) VALUES (?, ?)",
        params![LOG_WATERMARK, until],
    )?;

    Ok(())
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
) -> duckdb::Result<()> {
    let mut statement = conn.prepare(
        "INSERT INTO archive_files \
         (path, tenant, kind, min_logged_at, max_logged_at, rows, bytes, services, columns, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, CAST(CAST(? AS JSON) AS VARCHAR[]), CAST(CAST(? AS JSON) AS VARCHAR[]), ?)",
    )?;

    for archived in files {
        let meta = &archived.meta;
        statement.execute(params![
            archived.path,
            tenant,
            meta.kind.as_str(),
            meta.min_logged_at,
            meta.max_logged_at,
            meta.rows,
            meta.bytes,
            Value::from(meta.services.clone()).to_string(),
            Value::from(meta.columns.clone()).to_string(),
            meta.created_at,
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
