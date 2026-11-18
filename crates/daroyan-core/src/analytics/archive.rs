use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use duckdb::{Connection, params};
use serde_json::{Map, Value};

pub const LOG_WATERMARK: &str = "log_watermark";
pub const IS_ARCHIVED: &str = "ingested_at <= coalesce((SELECT value FROM archive_state WHERE name = 'log_watermark'), 0)";
pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const MAX_COLUMNS: usize = 1_000;
const FIXED_COLUMNS: &str = "server_id, logged_at, ingested_at, service, stream, severity, message, \
     trace_id, span_id, raw, http_status, http_method, url_path, client_ip";
const PARQUET_OPTIONS: &str =
    "FORMAT parquet, COMPRESSION zstd, COMPRESSION_LEVEL 3, ROW_GROUP_SIZE 122880";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Source {
    Attributes,
    Resource,
}

impl Source {
    fn column(self) -> &'static str {
        match self {
            Self::Attributes => "attributes",
            Self::Resource => "resource",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Attributes => "a",
            Self::Resource => "r",
        }
    }

    fn rest_column(self) -> &'static str {
        match self {
            Self::Attributes => "_rest_attributes",
            Self::Resource => "_rest_resource",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnKind {
    Integer,
    Float,
    Boolean,
    Text,
}

impl ColumnKind {
    fn from_types(types: &[String]) -> Self {
        let seen: Vec<&str> = types
            .iter()
            .map(String::as_str)
            .filter(|kind| *kind != "NULL")
            .collect();
        let only = |allowed: &[&str]| seen.iter().all(|kind| allowed.contains(kind));

        if seen.is_empty() {
            return Self::Text;
        }
        if only(&["BIGINT", "UBIGINT"]) {
            return Self::Integer;
        }
        if only(&["BIGINT", "UBIGINT", "DOUBLE"]) {
            return Self::Float;
        }
        if only(&["BOOLEAN"]) {
            return Self::Boolean;
        }

        Self::Text
    }

    fn sql_type(self) -> Option<&'static str> {
        match self {
            Self::Integer => Some("BIGINT"),
            Self::Float => Some("DOUBLE"),
            Self::Boolean => Some("BOOLEAN"),
            Self::Text => None,
        }
    }
}

#[derive(Debug)]
struct Field {
    source: Source,
    key: String,
    found: u64,
    kind: ColumnKind,
}

impl Field {
    fn name(&self) -> String {
        format!("{}.{}", self.source.prefix(), self.key)
    }

    fn select(&self) -> String {
        let path = quote_literal(&format!("$.\"{}\"", self.key));
        let value = format!("json_extract_string({}, '{path}')", self.source.column());
        let value = match self.kind.sql_type() {
            Some(sql_type) => format!("TRY_CAST({value} AS {sql_type})"),
            None => value,
        };

        format!("{value} AS {}", quote_identifier(&self.name()))
    }
}

#[derive(Debug, Default)]
struct Fields {
    chosen: Vec<Field>,
    overflowing: Vec<Source>,
}

impl Fields {
    fn choose(found: Vec<Field>, truncated: &[Source], max_columns: usize) -> Self {
        let mut found = found;
        found.sort_by_key(|field| Reverse(field.found));

        let mut fields = Self {
            chosen: Vec::new(),
            overflowing: truncated.to_vec(),
        };
        let mut names = HashSet::new();
        for field in found {
            let fits = fields.chosen.len() < max_columns
                && !field.key.contains('"')
                && names.insert(field.name().to_lowercase());
            if fits {
                fields.chosen.push(field);
            } else if !fields.overflowing.contains(&field.source) {
                fields.overflowing.push(field.source);
            }
        }

        fields
    }

    fn select(&self) -> String {
        let mut select = String::from(FIXED_COLUMNS);
        for field in &self.chosen {
            select.push_str(", ");
            select.push_str(&field.select());
        }
        for source in &self.overflowing {
            select.push_str(", ");
            select.push_str(&self.select_rest(*source));
        }

        select
    }

    fn select_rest(&self, source: Source) -> String {
        let patch: Map<String, Value> = self
            .chosen
            .iter()
            .filter(|field| field.source == source)
            .map(|field| (field.key.clone(), Value::Null))
            .collect();
        let patch = quote_literal(&Value::Object(patch).to_string());

        format!(
            "NULLIF(CAST(json_merge_patch({}, '{patch}') AS VARCHAR), '{{}}') AS {}",
            source.column(),
            source.rest_column()
        )
    }

    fn names(&self) -> Vec<String> {
        self.chosen
            .iter()
            .map(Field::name)
            .chain(self.overflowing.iter().map(|source| source.rest_column().to_string()))
            .collect()
    }
}

#[derive(Debug)]
pub struct StagedFile {
    pub local: PathBuf,
    pub name: String,
    pub hour: i64,
    pub min_logged_at: i64,
    pub max_logged_at: i64,
    pub rows: i64,
    pub services: Vec<String>,
    pub columns: Vec<String>,
}

#[derive(Debug)]
pub struct StagedExport {
    pub files: Vec<StagedFile>,
    pub until: i64,
}

#[derive(Debug)]
pub struct ArchivedFile {
    pub path: String,
    pub bytes: u64,
    pub file: StagedFile,
}

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

pub fn read_watermark(conn: &Connection) -> duckdb::Result<i64> {
    let value: Option<i64> = conn.query_row(
        "SELECT max(value) FROM archive_state WHERE name = ?",
        [LOG_WATERMARK],
        |row| row.get(0),
    )?;

    Ok(value.unwrap_or_default())
}

pub fn stage_logs(
    conn: &Connection,
    staging: &Path,
    now_ms: i64,
) -> duckdb::Result<StagedExport> {
    stage_logs_with(conn, staging, now_ms, MAX_COLUMNS)
}

fn stage_logs_with(
    conn: &Connection,
    staging: &Path,
    now_ms: i64,
    max_columns: usize,
) -> duckdb::Result<StagedExport> {
    let since = read_watermark(conn)?;
    let until = now_ms.max(since);
    let scope = format!("ingested_at > {since} AND ingested_at <= {until}");

    let mut statement = conn.prepare(&format!(
        "SELECT DISTINCT logged_at // {HOUR_MS} AS hour FROM log_lines WHERE {scope} ORDER BY hour"
    ))?;
    let hours: Vec<i64> = statement
        .query_map([], |row| row.get(0))?
        .collect::<duckdb::Result<_>>()?;

    let mut files = Vec::with_capacity(hours.len());
    for hour in hours {
        let scope = format!("{scope} AND logged_at // {HOUR_MS} = {hour}");
        files.push(stage_hour(conn, staging, &scope, hour, until, max_columns)?);
    }

    Ok(StagedExport { files, until })
}

fn stage_hour(
    conn: &Connection,
    staging: &Path,
    scope: &str,
    hour: i64,
    until: i64,
    max_columns: usize,
) -> duckdb::Result<StagedFile> {
    let mut found = Vec::new();
    let mut truncated = Vec::new();
    for source in [Source::Attributes, Source::Resource] {
        let fields = read_fields(conn, scope, source, max_columns + 1)?;
        if fields.len() > max_columns {
            truncated.push(source);
        }
        found.extend(fields);
    }
    let fields = Fields::choose(found, &truncated, max_columns);

    let name = format!("{until}-{:08x}.parquet", rand::random::<u32>());
    let local = staging.join(&name);
    conn.execute_batch(&format!(
        "COPY (SELECT {} FROM log_lines WHERE {scope} ORDER BY service, logged_at) TO '{}' ({PARQUET_OPTIONS})",
        fields.select(),
        quote_literal(&local.to_string_lossy()),
    ))?;

    let (min_logged_at, max_logged_at, rows, services) = conn.query_row(
        &format!(
            "SELECT min(logged_at), max(logged_at), count(*), \
             CAST(to_json(list_sort(list_distinct(list(service)))) AS VARCHAR) \
             FROM log_lines WHERE {scope}"
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, String>(3)?)),
    )?;

    Ok(StagedFile {
        local,
        name,
        hour,
        min_logged_at,
        max_logged_at,
        rows,
        services: serde_json::from_str(&services).unwrap_or_default(),
        columns: fields.names(),
    })
}

fn read_fields(
    conn: &Connection,
    scope: &str,
    source: Source,
    limit: usize,
) -> duckdb::Result<Vec<Field>> {
    let column = source.column();
    let mut statement = conn.prepare(&format!(
        "SELECT key, count(*) AS found, \
         CAST(to_json(list_distinct(list(CASE WHEN contains(key, '\"') THEN 'VARCHAR' \
              ELSE json_type({column}, '$.\"' || key || '\"') END))) AS VARCHAR) \
         FROM (SELECT {column}, unnest(json_keys({column})) AS key FROM log_lines \
               WHERE {scope} AND {column} IS NOT NULL) \
         GROUP BY key ORDER BY found DESC, key LIMIT {limit}"
    ))?;

    statement
        .query_map([], |row| {
            let types: String = row.get(2)?;
            let types: Vec<String> = serde_json::from_str(&types).unwrap_or_default();

            Ok(Field {
                source,
                key: row.get(0)?,
                found: row.get(1)?,
                kind: ColumnKind::from_types(&types),
            })
        })?
        .collect()
}

pub fn record_files(
    conn: &Connection,
    files: &[ArchivedFile],
    tenant: &str,
    until: i64,
    now_ms: i64,
) -> duckdb::Result<()> {
    conn.execute_batch("BEGIN TRANSACTION")?;

    let recorded = insert_files(conn, files, tenant, now_ms).and_then(|()| {
        conn.execute(
            "INSERT OR REPLACE INTO archive_state (name, value) VALUES (?, ?)",
            params![LOG_WATERMARK, until],
        )
    });
    if let Err(error) = recorded {
        conn.execute_batch("ROLLBACK")?;
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
            FileKind::Hour.as_str(),
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

fn quote_literal(text: &str) -> String {
    text.replace('\'', "''")
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use duckdb::Connection;

    use super::{
        ArchivedFile, ColumnKind, read_watermark, record_files, stage_logs_with,
    };

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-archive-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();

        dir
    }

    fn open(dir: &std::path::Path) -> Connection {
        let conn = Connection::open(dir.join("daroyan.duckdb")).unwrap();
        for sql in [
            include_str!("../../../daroyan-migration/duckdb/0002_log_lines.sql"),
            include_str!("../../../daroyan-migration/duckdb/0003_log_raw.sql"),
            include_str!("../../../daroyan-migration/duckdb/0011_log_resource.sql"),
            include_str!("../../../daroyan-migration/duckdb/0012_log_archive.sql"),
        ] {
            conn.execute_batch(sql).unwrap();
        }

        conn
    }

    fn insert(conn: &Connection, logged_at: i64, ingested_at: i64, attributes: &str) {
        conn.execute(
            "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, resource, ingested_at) \
             VALUES ('srv_1', ?, 'api', 'otlp', 9, 'done', ?, '{\"service.name\":\"api\"}', ?)",
            duckdb::params![logged_at, attributes, ingested_at],
        )
        .unwrap();
    }

    #[test]
    fn picks_a_column_type_from_the_json_types() {
        let kinds = |types: &[&str]| {
            ColumnKind::from_types(
                &types.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )
        };

        assert_eq!(kinds(&["UBIGINT", "BIGINT", "NULL"]), ColumnKind::Integer);
        assert_eq!(kinds(&["UBIGINT", "DOUBLE"]), ColumnKind::Float);
        assert_eq!(kinds(&["BOOLEAN"]), ColumnKind::Boolean);
        assert_eq!(kinds(&["UBIGINT", "VARCHAR"]), ColumnKind::Text);
        assert_eq!(kinds(&["OBJECT"]), ColumnKind::Text);
        assert_eq!(kinds(&["NULL"]), ColumnKind::Text);
    }

    #[test]
    fn writes_one_flat_parquet_file_per_hour() {
        let dir = scratch("hours");
        let conn = open(&dir);
        let hour = 1_794_000_000_000 / super::HOUR_MS * super::HOUR_MS;
        insert(&conn, hour + 5, 100, r#"{"http.status":200,"user.id":"u_1","took":1.5}"#);
        insert(&conn, hour + 9, 100, r#"{"http.status":"500","ok":true}"#);
        insert(&conn, hour + super::HOUR_MS + 1, 100, r#"{"ok":false}"#);
        insert(&conn, hour + 2, 900, r#"{"late":1}"#);

        let export = stage_logs_with(&conn, &dir, 500, 1_000).unwrap();
        assert_eq!(export.until, 500);
        assert_eq!(export.files.len(), 2);

        let first = &export.files[0];
        assert_eq!(first.hour, hour / super::HOUR_MS);
        assert_eq!(
            (first.min_logged_at, first.max_logged_at, first.rows),
            (hour + 5, hour + 9, 2)
        );
        assert_eq!(first.services, ["api"]);
        assert!(first.columns.contains(&"a.user.id".to_string()));
        assert!(first.columns.contains(&"r.service.name".to_string()));
        assert!(!first.columns.iter().any(|name| name.starts_with("_rest")));

        let local = first.local.to_string_lossy().to_string();
        let types: String = conn
            .query_row(
                &format!(
                    "SELECT string_agg(column_name || ':' || column_type, ',' ORDER BY column_name) \
                     FROM (DESCRIBE SELECT * FROM read_parquet('{local}')) \
                     WHERE column_name LIKE 'a.%' OR column_name LIKE 'r.%'"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            types,
            "a.http.status:VARCHAR,a.ok:BOOLEAN,a.took:DOUBLE,a.user.id:VARCHAR,r.service.name:VARCHAR"
        );

        let statuses: String = conn
            .query_row(
                "SELECT string_agg(\"a.http.status\", ',' ORDER BY logged_at) FROM read_parquet(?)",
                [&local],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(statuses, "200,500");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn keeps_keys_over_the_cap_in_a_rest_column() {
        let dir = scratch("cap");
        let conn = open(&dir);
        insert(&conn, 1_000, 100, r#"{"a":1,"b":2,"C":3,"c":4,"say \"hi\"":5}"#);
        insert(&conn, 1_001, 100, r#"{"a":1,"b":2}"#);

        let export = stage_logs_with(&conn, &dir, 500, 3).unwrap();
        let file = &export.files[0];
        assert_eq!(file.columns, ["a.a", "a.b", "r.service.name", "_rest_attributes"]);

        let local = file.local.to_string_lossy().to_string();
        let rest: Vec<Option<String>> = conn
            .prepare("SELECT _rest_attributes FROM read_parquet(?) ORDER BY logged_at")
            .unwrap()
            .query_map([&local], |row| row.get(0))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap();
        let first: serde_json::Value =
            serde_json::from_str(rest[0].as_deref().unwrap()).unwrap();
        assert_eq!(first, serde_json::json!({ "C": 3, "c": 4, "say \"hi\"": 5 }));
        assert_eq!(rest[1], None);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn moves_the_watermark_only_when_files_are_recorded() {
        let dir = scratch("record");
        let conn = open(&dir);
        insert(&conn, 1_000, 100, r#"{"a":1}"#);
        assert_eq!(read_watermark(&conn).unwrap(), 0);

        let export = stage_logs_with(&conn, &dir, 500, 10).unwrap();
        let until = export.until;
        let files: Vec<ArchivedFile> = export
            .files
            .into_iter()
            .map(|file| ArchivedFile {
                path: format!("logs/{}", file.name),
                bytes: 10,
                file,
            })
            .collect();
        record_files(&conn, &files, "default", until, 600).unwrap();
        assert_eq!(read_watermark(&conn).unwrap(), 500);

        let again = stage_logs_with(&conn, &dir, 700, 10).unwrap();
        assert!(again.files.is_empty());

        let (services, columns): (String, String) = conn
            .query_row(
                "SELECT CAST(services AS VARCHAR), CAST(columns AS VARCHAR) FROM archive_files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (services.as_str(), columns.as_str()),
            ("[api]", "[a.a, r.service.name]")
        );

        fs::remove_dir_all(dir).ok();
    }
}
