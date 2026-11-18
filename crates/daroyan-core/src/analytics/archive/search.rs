use std::collections::HashSet;

use duckdb::types::Value as SqlValue;
use duckdb::{Connection, Row, params_from_iter};
use serde_json::{Map, Value};

use crate::analytics::archive::{
    DAY_MS, REST_PREFIX, quote_identifier, quote_literal, read_json_list,
};
use crate::analytics::log_lines::{
    LogColumns, LogItem, LogSearch, push_search, read_item,
};

const FIXED_SELECT: &str = "logged_at, server_id, service, stream, severity, message, \
     NULL AS attributes, trace_id, span_id, raw, NULL AS resource";
const FIXED_COUNT: usize = 11;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColdFile {
    pub path: String,
    pub day: i64,
    pub bytes: u64,
    pub services: Vec<String>,
    pub columns: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ColdDay {
    pub start: i64,
    pub files: Vec<ColdFile>,
}

impl ColdDay {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }

    pub fn columns(&self) -> Vec<String> {
        let mut seen = HashSet::new();

        self.files
            .iter()
            .flat_map(|file| &file.columns)
            .filter(|name| seen.insert(name.to_lowercase()))
            .cloned()
            .collect()
    }
}

pub fn list_cold_days(
    conn: &Connection,
    since: Option<i64>,
    until: i64,
    services: &[String],
) -> duckdb::Result<Vec<ColdDay>> {
    let mut statement = conn.prepare(
        "SELECT path, min_logged_at, bytes, CAST(to_json(services) AS VARCHAR), CAST(to_json(columns) AS VARCHAR) \
         FROM archive_files \
         WHERE deleted_at IS NULL AND max_logged_at >= ? AND min_logged_at <= ? \
         ORDER BY min_logged_at DESC, path",
    )?;
    let files = statement
        .query_map([since.unwrap_or(i64::MIN), until], |row| {
            let min_logged_at: i64 = row.get(1)?;
            let services: String = row.get(3)?;
            let columns: String = row.get(4)?;

            Ok(ColdFile {
                path: row.get(0)?,
                day: min_logged_at.div_euclid(DAY_MS) * DAY_MS,
                bytes: row.get(2)?,
                services: read_json_list(&services),
                columns: read_json_list(&columns),
            })
        })?
        .collect::<duckdb::Result<Vec<_>>>()?;

    let mut days: Vec<ColdDay> = Vec::new();
    for file in files {
        let matches = services.is_empty()
            || file.services.iter().any(|service| services.contains(service));
        if !matches {
            continue;
        }

        match days.last_mut() {
            Some(day) if day.start == file.day => day.files.push(file),
            _ => days.push(ColdDay { start: file.day, files: vec![file] }),
        }
    }

    Ok(days)
}

pub fn list_cold_logs(
    conn: &Connection,
    sources: &[String],
    columns: &[String],
    search: &LogSearch,
    offset: u32,
    limit: u32,
) -> duckdb::Result<Vec<LogItem>> {
    let names: HashSet<String> = columns.iter().cloned().collect();
    let mut sql = select_cold(sources, columns);
    let mut values = Vec::new();
    push_search(&mut sql, &mut values, &LogColumns::Cold(names), search);
    sql.push_str(" ORDER BY logged_at DESC LIMIT ? OFFSET ?");
    values.push(SqlValue::UInt(limit));
    values.push(SqlValue::UInt(offset));

    let mut statement = conn.prepare(&sql)?;
    statement
        .query_map(params_from_iter(values), |row| read_cold_item(row, columns))?
        .collect()
}

pub fn count_cold_logs(
    conn: &Connection,
    sources: &[String],
    columns: &[String],
    search: &LogSearch,
) -> duckdb::Result<u64> {
    let names: HashSet<String> = columns.iter().cloned().collect();
    let mut sql = format!("SELECT count(*) FROM ({})", select_cold(sources, columns));
    let mut values = Vec::new();
    push_search(&mut sql, &mut values, &LogColumns::Cold(names), search);

    conn.query_row(&sql, params_from_iter(values), |row| row.get(0))
}

pub fn list_cold_context(
    conn: &Connection,
    sources: &[String],
    columns: &[String],
    server_id: &str,
    service: &str,
    at: i64,
    limit: u32,
) -> duckdb::Result<Vec<LogItem>> {
    let select = select_cold(sources, columns);
    let read = |order: &str, compare: &str| -> duckdb::Result<Vec<LogItem>> {
        let mut statement = conn.prepare(&format!(
            "{select} WHERE server_id = ? AND service = ? AND logged_at {compare} ? \
             ORDER BY logged_at {order} LIMIT ?"
        ))?;
        statement
            .query_map(
                params_from_iter([
                    SqlValue::Text(server_id.to_string()),
                    SqlValue::Text(service.to_string()),
                    SqlValue::BigInt(at),
                    SqlValue::UInt(limit),
                ]),
                |row| read_cold_item(row, columns),
            )?
            .collect()
    };

    let mut lines = read("DESC", "<=")?;
    lines.reverse();
    lines.extend(read("ASC", ">")?);

    Ok(lines)
}

fn select_cold(sources: &[String], columns: &[String]) -> String {
    let sources = sources
        .iter()
        .map(|source| format!("'{}'", quote_literal(source)))
        .collect::<Vec<_>>()
        .join(", ");
    let extra: String = columns
        .iter()
        .map(|name| format!(", {}", quote_identifier(name)))
        .collect();

    format!(
        "SELECT {FIXED_SELECT}{extra} FROM read_parquet([{sources}], union_by_name = true)"
    )
}

fn read_cold_item(row: &Row<'_>, columns: &[String]) -> duckdb::Result<LogItem> {
    let mut item = read_item(row)?;
    let mut attributes = Map::new();
    let mut resource = Map::new();

    for (index, name) in columns.iter().enumerate() {
        let value: SqlValue = row.get(FIXED_COUNT + index)?;
        let Some(value) = to_json(value) else { continue };

        if let Some(source) = name.strip_prefix(REST_PREFIX) {
            let Value::Object(rest) = value else { continue };
            let target =
                if source == "resource" { &mut resource } else { &mut attributes };
            target.extend(rest);
        } else if let Some(key) = name.strip_prefix("a.") {
            attributes.insert(key.to_string(), value);
        } else if let Some(key) = name.strip_prefix("r.") {
            resource.insert(key.to_string(), value);
        }
    }

    item.attributes = (!attributes.is_empty()).then_some(Value::Object(attributes));
    item.resource = (!resource.is_empty()).then_some(Value::Object(resource));

    Ok(item)
}

fn to_json(value: SqlValue) -> Option<Value> {
    match value {
        SqlValue::Null => None,
        SqlValue::Boolean(flag) => Some(Value::Bool(flag)),
        SqlValue::TinyInt(number) => Some(number.into()),
        SqlValue::SmallInt(number) => Some(number.into()),
        SqlValue::Int(number) => Some(number.into()),
        SqlValue::BigInt(number) => Some(number.into()),
        SqlValue::UTinyInt(number) => Some(number.into()),
        SqlValue::USmallInt(number) => Some(number.into()),
        SqlValue::UInt(number) => Some(number.into()),
        SqlValue::UBigInt(number) => Some(number.into()),
        SqlValue::Float(number) => Some(f64::from(number).into()),
        SqlValue::Double(number) => Some(number.into()),
        SqlValue::Text(text) => {
            let looks_like_json = text.starts_with('{') || text.starts_with('[');
            let parsed =
                looks_like_json.then(|| serde_json::from_str(&text).ok()).flatten();
            Some(parsed.unwrap_or(Value::String(text)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use duckdb::Connection;
    use serde_json::json;

    use super::{
        ColdFile, count_cold_logs, list_cold_context, list_cold_days, list_cold_logs,
    };
    use crate::analytics::archive::{DAY_MS, create_log_tables};
    use crate::analytics::log_lines::{LogFilter, LogSearch, SearchTerm};

    const DAY: i64 = 1_794_009_600_000 / DAY_MS * DAY_MS;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-cold-{name}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();

        dir
    }

    fn write_files(conn: &Connection, dir: &std::path::Path) -> Vec<String> {
        let first = dir.join("first.parquet").to_string_lossy().to_string();
        let second = dir.join("second.parquet").to_string_lossy().to_string();
        conn.execute_batch(&format!(
            "COPY (SELECT 'srv_1' AS server_id, {DAY} + 10 AS logged_at, 1 AS ingested_at, 'api' AS service, 'otlp' AS stream, 17::UTINYINT AS severity, \
                     '' AS message, NULL AS trace_id, NULL AS span_id, NULL AS raw, 404::USMALLINT AS http_status, 'GET' AS http_method, '/v1/links' AS url_path, NULL AS client_ip, \
                     'u_1' AS \"a.user.id\", 3 AS \"a.attempt\", '{{\"deep\":{{\"x\":1}}}}' AS _rest_attributes, 'api' AS \"r.service.name\" \
                   UNION ALL SELECT 'srv_1', {DAY} + 20, 1, 'api', 'otlp', 9, 'GET /health', NULL, NULL, NULL, 200, 'GET', '/health', NULL, 'u_2', 1, NULL, 'api') TO '{first}'; \
             COPY (SELECT 'srv_2' AS server_id, {DAY} + 30 AS logged_at, 1 AS ingested_at, 'web' AS service, 'stdout' AS stream, 13::UTINYINT AS severity, \
                     'slow page' AS message, NULL AS trace_id, NULL AS span_id, NULL AS raw, NULL::USMALLINT AS http_status, NULL AS http_method, NULL AS url_path, NULL AS client_ip, \
                     true AS \"a.cached\") TO '{second}';"
        ))
        .unwrap();

        vec![first, second]
    }

    fn columns() -> Vec<String> {
        ["a.user.id", "a.attempt", "_rest_attributes", "r.service.name", "a.cached"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn groups_live_files_by_day_newest_first() {
        let dir = scratch("days");
        let conn = Connection::open(dir.join("daroyan.duckdb")).unwrap();
        create_log_tables(&conn).unwrap();
        for (path, at, services, deleted_at) in [
            ("a", DAY + 5, "['api']", None),
            ("b", DAY + 9, "['web']", None),
            ("c", DAY + DAY_MS + 5, "['api']", None),
            ("d", DAY + DAY_MS + 7, "['api']", Some(1)),
            ("e", DAY - DAY_MS, "['api']", None),
        ] {
            conn.execute(
                &format!("INSERT INTO archive_files VALUES (?, 'default', 'hour', ?, ?, 1, 7, {services}, ['a.x'], 0, ?)"),
                duckdb::params![path, at, at + 1, deleted_at],
            )
            .unwrap();
        }

        let days = list_cold_days(&conn, Some(DAY), DAY + 2 * DAY_MS, &[]).unwrap();
        let paths: Vec<Vec<&str>> = days
            .iter()
            .map(|day| day.files.iter().map(|file| file.path.as_str()).collect())
            .collect();
        assert_eq!(paths, [vec!["c"], vec!["b", "a"]]);
        assert_eq!(days[1].start, DAY);
        assert_eq!(days[1].bytes(), 14);
        assert_eq!(days[1].columns(), ["a.x"]);

        let api = list_cold_days(&conn, None, DAY + 2 * DAY_MS, &["api".into()]).unwrap();
        let first: Vec<&ColdFile> = api.iter().flat_map(|day| &day.files).collect();
        assert_eq!(
            first.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
            ["c", "a", "e"]
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn searches_flat_files_like_the_hot_table() {
        let dir = scratch("search");
        let conn = Connection::open(dir.join("daroyan.duckdb")).unwrap();
        let sources = write_files(&conn, &dir);
        let columns = columns();
        let find = |search: LogSearch| -> Vec<i64> {
            list_cold_logs(&conn, &sources, &columns, &search, 0, 50)
                .unwrap()
                .iter()
                .map(|item| item.at - DAY)
                .collect()
        };
        let until = DAY + DAY_MS;

        assert_eq!(find(LogSearch { until, ..Default::default() }), [30, 20, 10]);
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::key("user.id", "U_1", false)],
                ..Default::default()
            }),
            [10]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::key("deep", "*", false)],
                ..Default::default()
            }),
            [10]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::message("get /v1/links", false)],
                ..Default::default()
            }),
            [10]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::key("http.status", "404", false)],
                ..Default::default()
            }),
            [10]
        );
        assert_eq!(
            find(LogSearch {
                until,
                status_classes: LogFilter { values: vec!["2".into()], negate: false },
                ..Default::default()
            }),
            [20]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm {
                    field: crate::analytics::log_lines::SearchField::Any,
                    matches: crate::analytics::log_lines::SearchMatch::Text("u_2".into()),
                    negate: false,
                }],
                ..Default::default()
            }),
            [20]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::key("cached", "true", false)],
                ..Default::default()
            }),
            [30]
        );
        assert_eq!(
            find(LogSearch {
                until,
                terms: vec![SearchTerm::key("missing", "*", false)],
                ..Default::default()
            }),
            [0; 0]
        );

        let count = count_cold_logs(
            &conn,
            &sources,
            &columns,
            &LogSearch { until, ..Default::default() },
        )
        .unwrap();
        assert_eq!(count, 3);

        let items = list_cold_logs(
            &conn,
            &sources,
            &columns,
            &LogSearch { until, ..Default::default() },
            2,
            5,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].attributes,
            Some(json!({ "user.id": "u_1", "attempt": 3, "deep": { "x": 1 } }))
        );
        assert_eq!(items[0].resource, Some(json!({ "service.name": "api" })));
        assert_eq!(items[0].level, "error");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reads_the_lines_around_a_cold_line() {
        let dir = scratch("context");
        let conn = Connection::open(dir.join("daroyan.duckdb")).unwrap();
        let sources = write_files(&conn, &dir);

        let lines =
            list_cold_context(&conn, &sources, &columns(), "srv_1", "api", DAY + 10, 5)
                .unwrap();
        assert_eq!(lines.iter().map(|item| item.at - DAY).collect::<Vec<_>>(), [10, 20]);

        fs::remove_dir_all(dir).ok();
    }
}
