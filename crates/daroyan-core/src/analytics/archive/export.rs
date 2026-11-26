use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::Path;

use duckdb::Connection;
use serde_json::{Map, Value};

use crate::analytics::archive::{
    FileKind, HOUR_MS, MAX_COLUMNS, PARQUET_OPTIONS, REST_PREFIX, StagedFile,
    quote_identifier, quote_literal, read_json_list, read_watermark,
};

const FIXED_COLUMNS: &str = "server_id, logged_at, ingested_at, service, stream, severity, message, \
     trace_id, span_id, raw, http_status, http_method, url_path, client_ip";

#[derive(Debug)]
pub struct StagedExport {
    pub files: Vec<StagedFile>,
    pub until: i64,
}

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

    fn rest_column(self) -> String {
        format!("{REST_PREFIX}{}", self.column())
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
            .chain(self.overflowing.iter().map(|source| source.rest_column()))
            .collect()
    }
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
             (SELECT CAST(to_json(list(service ORDER BY service)) AS VARCHAR) \
              FROM (SELECT DISTINCT service FROM log_lines WHERE {scope})) \
             FROM log_lines WHERE {scope}"
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, String>(3)?)),
    )?;

    Ok(StagedFile {
        local,
        name,
        kind: FileKind::Hour,
        hour,
        min_logged_at,
        max_logged_at,
        rows,
        services: read_json_list(&services),
        columns: fields.names(),
        exported_until: until,
        replaces: Vec::new(),
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
        "SELECT key, CAST(sum(found) AS BIGINT) AS found, \
         CAST(to_json(list(kind ORDER BY kind) FILTER (WHERE kind IS NOT NULL)) AS VARCHAR) \
         FROM (SELECT key, kind, count(*) AS found \
               FROM (SELECT key, CASE WHEN contains(key, '\"') THEN 'VARCHAR' \
                         WHEN json_type({column}, '$.\"' || key || '\"') = 'UBIGINT' \
                              AND TRY_CAST(json_extract_string({column}, '$.\"' || key || '\"') AS BIGINT) IS NULL THEN 'VARCHAR' \
                         ELSE json_type({column}, '$.\"' || key || '\"') END AS kind \
                     FROM (SELECT {column}, unnest(json_keys({column})) AS key FROM log_lines \
                           WHERE {scope} AND {column} IS NOT NULL)) \
               GROUP BY key, kind) \
         GROUP BY key ORDER BY found DESC, key LIMIT {limit}"
    ))?;

    statement
        .query_map([], |row| {
            let types: String = row.get(2)?;
            let types = read_json_list(&types);

            Ok(Field {
                source,
                key: row.get(0)?,
                found: row.get(1)?,
                kind: ColumnKind::from_types(&types),
            })
        })?
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use duckdb::Connection;

    use super::{ColumnKind, stage_logs_with};
    use crate::analytics::archive::{
        ArchivedFile, FileMeta, HOUR_MS, create_log_tables, read_watermark, record_files,
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
        create_log_tables(&conn).unwrap();

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
        let hour = 1_794_000_000_000 / HOUR_MS * HOUR_MS;
        insert(&conn, hour + 5, 100, r#"{"http.status":200,"user.id":"u_1","took":1.5}"#);
        insert(&conn, hour + 9, 100, r#"{"http.status":"500","ok":true}"#);
        insert(&conn, hour + HOUR_MS + 1, 100, r#"{"ok":false}"#);
        insert(&conn, hour + 2, 900, r#"{"late":1}"#);

        let export = stage_logs_with(&conn, &dir, 500, 1_000).unwrap();
        assert_eq!(export.until, 500);
        assert_eq!(export.files.len(), 2);

        let first = &export.files[0];
        assert_eq!(first.hour, hour / HOUR_MS);
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
    fn keeps_integers_too_big_for_bigint_as_text() {
        let dir = scratch("big");
        let conn = open(&dir);
        insert(&conn, 1_000, 100, r#"{"hash":18446744073709551615,"small":7}"#);
        insert(&conn, 1_001, 100, r#"{"hash":3,"small":8}"#);

        let export = stage_logs_with(&conn, &dir, 500, 10).unwrap();
        let local = export.files[0].local.to_string_lossy().to_string();
        let (hash, small): (String, i64) = conn
            .query_row(
                &format!(
                    "SELECT \"a.hash\", \"a.small\" FROM read_parquet('{local}') ORDER BY logged_at LIMIT 1"
                ),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((hash.as_str(), small), ("18446744073709551615", 7));

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
                meta: FileMeta::new(file, 10, 600),
            })
            .collect();
        record_files(&conn, &files, "default", until).unwrap();
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

    #[test]
    fn stages_a_busy_hour_in_little_memory() {
        let dir = scratch("busy");
        let conn = open(&dir);
        conn.execute_batch(&format!(
            "INSERT INTO log_lines (server_id, logged_at, service, stream, severity, message, attributes, resource, ingested_at) \
             SELECT 'srv_1', {HOUR_MS} * 10 + i % {HOUR_MS}, 'api' || (i % 20), 'otlp', 9, 'done', \
                    json_object('user.id', 'u_' || i, 'request.id', md5(i::VARCHAR), 'route', '/r/' || (i % 50), \
                                'status', 200 + i % 5, 'took', i * 0.37, 'path', '/a/b/' || i), \
                    json_object('service.name', 'api' || (i % 20), 'host.name', 'web-' || (i % 4)), 1 \
             FROM range(600000) t(i); \
             SET memory_limit = '112MB'; SET threads = 2;"
        ))
        .unwrap();

        let staging = dir.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let export = stage_logs_with(&conn, &staging, 2, 64).unwrap();

        assert_eq!(export.files.len(), 1);
        assert_eq!(export.files[0].rows, 600_000);
        assert_eq!(export.files[0].services.len(), 20);
        assert_eq!(export.files[0].columns.len(), 8);

        fs::remove_dir_all(dir).ok();
    }
}
