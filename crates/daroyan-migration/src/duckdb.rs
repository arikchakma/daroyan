use std::path::Path;

use duckdb::Connection;

const MIGRATIONS: [(&str, &str); 8] = [
    ("0001_server_states.sql", include_str!("../duckdb/0001_server_states.sql")),
    ("0002_log_lines.sql", include_str!("../duckdb/0002_log_lines.sql")),
    ("0003_log_raw.sql", include_str!("../duckdb/0003_log_raw.sql")),
    ("0004_uptime_results.sql", include_str!("../duckdb/0004_uptime_results.sql")),
    ("0005_uptime_timings.sql", include_str!("../duckdb/0005_uptime_timings.sql")),
    ("0006_cron_runs.sql", include_str!("../duckdb/0006_cron_runs.sql")),
    ("0007_spans.sql", include_str!("../duckdb/0007_spans.sql")),
    ("0008_snake_case.sql", include_str!("../duckdb/0008_snake_case.sql")),
];

pub fn migrate_duckdb(path: &Path) -> duckdb::Result<()> {
    let conn = Connection::open(path)?;
    run_duckdb_migrations(&conn)
}

pub fn run_duckdb_migrations(conn: &Connection) -> duckdb::Result<()> {
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
