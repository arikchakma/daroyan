use std::path::Path;

use duckdb::{Config, Connection};

const STORAGE_VERSION: &str = "v1.2.0";
const EXTENSION_DIR: &str = "duckdb_extensions";

const MIGRATIONS: [(&str, &str); 14] = [
    ("0001_server_states.sql", include_str!("../duckdb/0001_server_states.sql")),
    ("0002_log_lines.sql", include_str!("../duckdb/0002_log_lines.sql")),
    ("0003_log_raw.sql", include_str!("../duckdb/0003_log_raw.sql")),
    ("0004_uptime_results.sql", include_str!("../duckdb/0004_uptime_results.sql")),
    ("0005_uptime_timings.sql", include_str!("../duckdb/0005_uptime_timings.sql")),
    ("0006_cron_runs.sql", include_str!("../duckdb/0006_cron_runs.sql")),
    ("0007_spans.sql", include_str!("../duckdb/0007_spans.sql")),
    ("0008_snake_case.sql", include_str!("../duckdb/0008_snake_case.sql")),
    ("0009_disk_states.sql", include_str!("../duckdb/0009_disk_states.sql")),
    ("0010_span_servers.sql", include_str!("../duckdb/0010_span_servers.sql")),
    ("0011_log_resource.sql", include_str!("../duckdb/0011_log_resource.sql")),
    ("0012_log_archive.sql", include_str!("../duckdb/0012_log_archive.sql")),
    ("0013_log_ingested_at.sql", include_str!("../duckdb/0013_log_ingested_at.sql")),
    ("0014_log_volume.sql", include_str!("../duckdb/0014_log_volume.sql")),
];

pub fn open_duckdb(path: &Path) -> duckdb::Result<Connection> {
    let extensions = path.with_file_name(EXTENSION_DIR);
    let config = Config::default()
        .with("storage_compatibility_version", STORAGE_VERSION)?
        .with("extension_directory", extensions.to_string_lossy())?;
    let conn = Connection::open_with_flags(path, config)?;
    run_migrations(&conn)?;

    if is_legacy(&conn)? {
        tracing::warn!(
            path = %path.display(),
            "the DuckDB file is in the old storage format, so its columns are not compressed with zstd, delete the file to start a compressed one"
        );
    }

    Ok(conn)
}

fn is_legacy(conn: &Connection) -> duckdb::Result<bool> {
    let version: String = conn.query_row(
        "SELECT tags['storage_version'] FROM duckdb_databases() WHERE database_name = current_database()",
        [],
        |row| row.get(0),
    )?;

    Ok(["v0.", "v1.0.", "v1.1."]
        .iter()
        .any(|prefix| version.starts_with(prefix)))
}

fn run_migrations(conn: &Connection) -> duckdb::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (name VARCHAR PRIMARY KEY, applied_at TIMESTAMP DEFAULT current_timestamp)",
    )?;
    for (name, sql) in MIGRATIONS {
        let applied: i64 = conn.query_row(
            "SELECT count(*) FROM _migrations WHERE name = ?",
            [name],
            |row| row.get(0),
        )?;
        if applied > 0 {
            continue;
        }
        conn.execute_batch(&format!(
            "BEGIN; {sql}; INSERT INTO _migrations (name) VALUES ('{name}'); COMMIT;"
        ))?;
        tracing::info!("applied {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use duckdb::Connection;

    use super::{is_legacy, open_duckdb, run_migrations};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-storage-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();

        dir
    }

    fn list_codecs(conn: &Connection, table: &str, column: &str) -> Vec<String> {
        let mut statement = conn
            .prepare(&format!(
                "SELECT DISTINCT compression FROM pragma_storage_info('{table}') \
                 WHERE column_name = ? AND segment_type <> 'VALIDITY'"
            ))
            .unwrap();

        statement
            .query_map([column], |row| row.get(0))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap()
    }

    fn fill(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO log_lines BY NAME \
             SELECT 'srv_1' AS server_id, 1794000000000 + i AS logged_at, 'api' AS service, 'stdout' AS stream, \
                    9::UTINYINT AS severity, 'GET /health' AS message, \
                    json_object('uri', '/v1/links/' || md5(i::VARCHAR), 'size', i) AS attributes, \
                    '2026-11-13T09:00:00Z INFO GET /health ' || md5(i::VARCHAR) AS raw \
             FROM range(40000) AS r(i); \
             INSERT INTO spans BY NAME \
             SELECT md5(i::VARCHAR) AS trace_id, 'span' AS span_id, 'api' AS service, 'GET /health' AS name, \
                    'server' AS kind, 1794000000000000 + i AS started_at, 5 AS duration_us, 'ok' AS status, \
                    json_object('uri', '/v1/links/' || md5(i::VARCHAR)) AS attributes, \
                    json_array(json_object('name', md5(i::VARCHAR))) AS events, 1794000000000 AS received_at \
             FROM range(40000) AS r(i); \
             INSERT INTO span_exceptions BY NAME \
             SELECT md5(i::VARCHAR) AS trace_id, 'span' AS span_id, 'api' AS service, 'print' AS fingerprint, \
                    'Error' AS type, 'failed' AS message, repeat(md5(i::VARCHAR), 20) AS stacktrace, \
                    1794000000000000 + i AS occurred_at \
             FROM range(40000) AS r(i); \
             CHECKPOINT;",
        )
        .unwrap();
    }

    #[test]
    fn creates_new_files_with_zstd_columns() {
        let dir = scratch("fresh");
        let conn = open_duckdb(&dir.join("daroyan.duckdb")).unwrap();
        assert!(!is_legacy(&conn).unwrap());

        fill(&conn);
        for (table, column) in [
            ("log_lines", "attributes"),
            ("log_lines", "raw"),
            ("spans", "attributes"),
            ("spans", "events"),
            ("span_exceptions", "stacktrace"),
        ] {
            assert_eq!(list_codecs(&conn, table, column), ["ZSTD"], "{table}.{column}");
        }

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn opens_an_old_format_file_as_it_is() {
        let dir = scratch("legacy");
        let path = dir.join("daroyan.duckdb");
        let legacy = Connection::open(&path).unwrap();
        run_migrations(&legacy).unwrap();
        fill(&legacy);
        drop(legacy);

        let conn = open_duckdb(&path).unwrap();
        assert!(is_legacy(&conn).unwrap());
        let lines: i64 = conn
            .query_row("SELECT count(*) FROM log_lines", [], |row| row.get(0))
            .unwrap();
        assert_eq!(lines, 40_000);

        fs::remove_dir_all(dir).ok();
    }
}
