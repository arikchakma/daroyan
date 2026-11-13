use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use duckdb::{Config, Connection};

const STORAGE_VERSION: &str = "v1.2.0";
const LEGACY_MIGRATIONS: usize = 11;
const KEPT_RAW: &str =
    "CASE WHEN json_valid(raw) AND starts_with(ltrim(raw), '{') THEN NULL ELSE raw END";

const MIGRATIONS: [(&str, &str); 12] = [
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
    ("0012_zstd_columns.sql", include_str!("../duckdb/0012_zstd_columns.sql")),
];

pub fn open_duckdb(path: &Path) -> anyhow::Result<Connection> {
    let conn = open(path)?;
    if !is_legacy(&conn)? {
        run_migrations(&conn, &MIGRATIONS)?;
        return Ok(conn);
    }

    run_migrations(&conn, &MIGRATIONS[..LEGACY_MIGRATIONS])?;
    conn.execute_batch("CHECKPOINT")?;
    conn.close().map_err(|(_, error)| error)?;
    upgrade_storage(path)?;

    let conn = open(path)?;
    run_migrations(&conn, &MIGRATIONS)?;
    Ok(conn)
}

fn open(path: &Path) -> duckdb::Result<Connection> {
    let config =
        Config::default().with("storage_compatibility_version", STORAGE_VERSION)?;

    Connection::open_with_flags(path, config)
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

fn upgrade_storage(path: &Path) -> anyhow::Result<()> {
    let packed = suffixed(path, ".upgrade");
    let packed_wal = suffixed(&packed, ".wal");
    for stale in [&packed, &packed_wal] {
        if stale.exists() {
            fs::remove_file(stale).context("remove an unfinished storage upgrade")?;
        }
    }

    let before = fs::metadata(path).context("read the DuckDB file size")?.len();
    tracing::info!(
        size_mb = before / 1_048_576,
        "upgrading the DuckDB file to compressed storage, this runs once"
    );

    let conn = open(&packed)?;
    run_migrations(&conn, &MIGRATIONS)?;
    let source = path.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("ATTACH '{source}' AS legacy (READ_ONLY)"))?;
    for table in list_legacy_tables(&conn)? {
        let rows = conn.execute(&copy_sql(&table), [])?;
        tracing::info!(table, rows, "copied a table");
    }
    conn.execute_batch("DETACH legacy; CHECKPOINT")?;
    conn.close().map_err(|(_, error)| error)?;

    let wal = suffixed(path, ".wal");
    if wal.exists() {
        fs::remove_file(&wal).context("remove the old DuckDB write-ahead log")?;
    }
    fs::rename(&packed, path).context("replace the DuckDB file")?;

    let after = fs::metadata(path).context("read the DuckDB file size")?.len();
    tracing::info!(
        before_mb = before / 1_048_576,
        after_mb = after / 1_048_576,
        "upgraded the DuckDB file"
    );
    Ok(())
}

fn list_legacy_tables(conn: &Connection) -> duckdb::Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT table_name FROM duckdb_tables() \
         WHERE database_name = 'legacy' AND table_name <> '_migrations' ORDER BY table_name",
    )?;

    statement.query_map([], |row| row.get(0))?.collect()
}

fn copy_sql(table: &str) -> String {
    if table == "log_lines" {
        return format!(
            "INSERT INTO log_lines BY NAME SELECT * REPLACE ({KEPT_RAW} AS raw) FROM legacy.log_lines"
        );
    }

    format!("INSERT INTO \"{table}\" BY NAME SELECT * FROM legacy.\"{table}\"")
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);

    PathBuf::from(name)
}

fn run_migrations(conn: &Connection, migrations: &[(&str, &str)]) -> duckdb::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (name VARCHAR PRIMARY KEY, applied_at TIMESTAMP DEFAULT current_timestamp)",
    )?;
    for (name, sql) in migrations {
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

    use super::{LEGACY_MIGRATIONS, MIGRATIONS, is_legacy, open_duckdb, run_migrations};

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

    fn fill_logs(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO log_lines \
             SELECT 'srv_1', 1794000000000 + i, 'Caddy', 'file', 9, 'handled request', \
                    json_object('uri', '/v1/links/' || md5(i::VARCHAR), 'status', 200, 'size', i), \
                    NULL, NULL, \
                    '{\"msg\":\"handled request\",\"uri\":\"/v1/links/' || md5(i::VARCHAR) || '\",\"size\":' || i || '}', \
                    NULL \
             FROM range(40000) AS r(i); \
             INSERT INTO log_lines \
             SELECT 'srv_1', 1794000100000 + i, 'api', 'stdout', 9, 'GET /health', \
                    json_object('http.method', 'GET'), NULL, NULL, \
                    '2026-11-13T09:00:00Z INFO GET /health ' || i, NULL \
             FROM range(100) AS r(i); \
             CHECKPOINT;",
        )
        .unwrap();
    }

    #[test]
    fn creates_new_files_with_zstd_columns() {
        let dir = scratch("fresh");
        let conn = open_duckdb(&dir.join("daroyan.duckdb")).unwrap();
        assert!(!is_legacy(&conn).unwrap());

        fill_logs(&conn);
        assert_eq!(list_codecs(&conn, "log_lines", "attributes"), ["ZSTD"]);
        assert_eq!(list_codecs(&conn, "log_lines", "raw"), ["ZSTD"]);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn upgrades_a_legacy_file_once_and_keeps_its_rows() {
        let dir = scratch("legacy");
        let path = dir.join("daroyan.duckdb");
        let legacy = Connection::open(&path).unwrap();
        assert!(is_legacy(&legacy).unwrap());
        run_migrations(&legacy, &MIGRATIONS[..LEGACY_MIGRATIONS - 1]).unwrap();
        legacy
            .execute_batch(
                "INSERT INTO server_events VALUES ('srv_1', 5, 'agent_started', '{}'); \
                 INSERT INTO log_lines SELECT * FROM (SELECT 'srv_1', 1, 'api', 'stdout', 9, 'old', NULL, NULL, NULL, 'kept text'); \
                 ALTER TABLE log_lines ADD COLUMN resource JSON; \
                 INSERT INTO _migrations (name) VALUES ('0011_log_resource.sql');",
            )
            .unwrap();
        fill_logs(&legacy);
        drop(legacy);
        let before = fs::metadata(&path).unwrap().len();

        let conn = open_duckdb(&path).unwrap();
        assert!(!is_legacy(&conn).unwrap());
        let after = fs::metadata(&path).unwrap().len();
        assert!(after < before, "{after} >= {before}");
        assert!(!dir.join("daroyan.duckdb.upgrade").exists());

        let (lines, with_raw): (i64, i64) = conn
            .query_row("SELECT count(*), count(raw) FROM log_lines", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(lines, 40_101);
        assert_eq!(with_raw, 101);

        let events: i64 = conn
            .query_row("SELECT count(*) FROM server_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(events, 1);
        assert_eq!(list_codecs(&conn, "log_lines", "attributes"), ["ZSTD"]);

        let applied: i64 = conn
            .query_row("SELECT count(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(applied, MIGRATIONS.len() as i64);
        drop(conn);

        let size = fs::metadata(&path).unwrap().len();
        let again = open_duckdb(&path).unwrap();
        let lines: i64 = again
            .query_row("SELECT count(*) FROM log_lines", [], |row| row.get(0))
            .unwrap();
        assert_eq!(lines, 40_101);
        assert_eq!(fs::metadata(&path).unwrap().len(), size);

        fs::remove_dir_all(dir).ok();
    }
}
