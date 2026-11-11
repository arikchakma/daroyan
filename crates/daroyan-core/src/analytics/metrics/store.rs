use duckdb::{Connection, params};

use crate::analytics::metrics::ingest::{
    HistogramReading, Reading, SeriesRow, Temporality,
};
use crate::analytics::metrics::series::{KnownSeries, LastPoint, PointRow};

pub const HOUR_MS: i64 = 60 * 60 * 1000;
const LOAD_WINDOW_MS: i64 = 24 * HOUR_MS;

pub fn append_metrics(
    conn: &Connection,
    series: &[SeriesRow],
    points: &[PointRow],
) -> duckdb::Result<()> {
    if series.is_empty() && points.is_empty() {
        return Ok(());
    }

    let mut appender = conn.appender("metric_series_staging")?;
    for row in series {
        appender.append_row(params![
            row.series_id,
            &*row.token_id,
            &*row.name,
            &*row.unit,
            &*row.description,
            row.kind.as_str(),
            row.is_monotonic,
            row.temporality.map(Temporality::as_str),
            &*row.service,
            row.server_id.as_deref(),
            row.instance.as_deref(),
            row.attributes,
            row.resource.as_deref(),
            row.bounds.as_deref().map(to_list_text),
            row.first_seen,
            row.first_seen,
        ])?;
    }
    appender.flush()?;
    drop(appender);

    let mut appender = conn.appender("metric_points_staging")?;
    for row in points {
        appender.append_row(params![
            row.series_id,
            row.at,
            row.start_at,
            row.value,
            row.delta,
            row.count,
            row.sum,
            row.min,
            row.max,
            row.buckets.as_deref().map(to_list_text),
            row.count_delta,
            row.sum_delta,
            row.bucket_deltas.as_deref().map(to_list_text),
            row.trace_id,
            row.span_id,
        ])?;
    }
    appender.flush()?;
    drop(appender);

    conn.execute_batch(
        "INSERT INTO metric_series
            SELECT * REPLACE (CAST(bounds AS DOUBLE[]) AS bounds) FROM metric_series_staging;
         DELETE FROM metric_series_staging;
         INSERT INTO metric_points
            SELECT * REPLACE (
                CAST(buckets AS UBIGINT[]) AS buckets,
                CAST(bucket_deltas AS UBIGINT[]) AS bucket_deltas
            ) FROM metric_points_staging;
         DELETE FROM metric_points_staging;",
    )
}

fn to_list_text<T: ToString>(values: &[T]) -> String {
    let values: Vec<String> = values.iter().map(ToString::to_string).collect();

    format!("[{}]", values.join(","))
}

pub fn list_known_series(
    conn: &Connection,
    now: i64,
) -> duckdb::Result<Vec<KnownSeries>> {
    let mut statement = conn.prepare(
        "SELECT s.series_id, s.token_id, s.name, greatest(s.last_seen, coalesce(p.taken_at, 0)),
                p.taken_at, p.start_at, p.value, p.count, p.sum, CAST(p.buckets AS VARCHAR)
         FROM metric_series s
         LEFT JOIN (
            SELECT series_id, max(taken_at) AS taken_at,
                   arg_max(start_at, taken_at) AS start_at,
                   arg_max(value, taken_at) AS value,
                   arg_max(count, taken_at) AS count,
                   arg_max(sum, taken_at) AS sum,
                   arg_max(buckets, taken_at) AS buckets
            FROM metric_points
            WHERE taken_at >= ?
            GROUP BY series_id
         ) p USING (series_id)",
    )?;

    let rows = statement.query_map([now - LOAD_WINDOW_MS], |row| {
        let at: Option<i64> = row.get(4)?;
        let value: Option<f64> = row.get(6)?;
        let count: Option<u64> = row.get(7)?;
        let buckets: Option<String> = row.get(9)?;
        let reading = match (value, count) {
            (Some(value), _) => Some(Reading::Number(value)),
            (None, Some(count)) => Some(Reading::Histogram(HistogramReading {
                count,
                sum: row.get(8)?,
                min: None,
                max: None,
                buckets: buckets.as_deref().map(read_list).unwrap_or_default(),
            })),
            (None, None) => None,
        };

        Ok(KnownSeries {
            series_id: row.get(0)?,
            token_id: row.get::<_, String>(1)?.into(),
            name: row.get::<_, String>(2)?.into(),
            last_seen: row.get(3)?,
            last: at.zip(reading).map(|(at, reading)| LastPoint {
                at,
                start_at: row.get(5).ok().flatten(),
                reading,
            }),
        })
    })?;

    rows.collect()
}

fn read_list(text: &str) -> Vec<u64> {
    text.trim_matches(['[', ']'])
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect()
}

pub fn roll_up_metrics(conn: &Connection, now: i64) -> duckdb::Result<()> {
    let end = now / HOUR_MS * HOUR_MS;
    let start: Option<i64> = conn.query_row(
        "SELECT coalesce(
            (SELECT max(hour) + ? FROM metric_rollups),
            (SELECT min(taken_at) // ? * ? FROM metric_points)
         )",
        params![HOUR_MS, HOUR_MS, HOUR_MS],
        |row| row.get(0),
    )?;
    let Some(start) = start.filter(|start| *start < end) else { return Ok(()) };

    conn.execute(
        "INSERT INTO metric_rollups
         WITH window_points AS (
            SELECT *, taken_at // ?3 * ?3 AS hour FROM metric_points WHERE taken_at >= ?1 AND taken_at < ?2
         ),
         buckets AS (
            SELECT series_id, hour, list(total ORDER BY slot) AS bucket_deltas
            FROM (
                SELECT series_id, hour, slot, sum(count) AS total
                FROM (
                    SELECT series_id, hour,
                           generate_subscripts(bucket_deltas, 1) AS slot,
                           unnest(bucket_deltas) AS count
                    FROM window_points
                    WHERE bucket_deltas IS NOT NULL
                )
                GROUP BY series_id, hour, slot
            )
            GROUP BY series_id, hour
         )
         SELECT series_id, hour, count(*),
                arg_max(value, taken_at), min(coalesce(min, value)), max(coalesce(max, value)),
                avg(value), sum(delta), sum(count_delta), sum(sum_delta),
                any_value(buckets.bucket_deltas)
         FROM window_points
         LEFT JOIN buckets USING (series_id, hour)
         GROUP BY series_id, hour",
        params![start, end, HOUR_MS],
    )?;

    Ok(())
}

pub fn purge_metrics(
    conn: &Connection,
    point_cutoff: i64,
    rollup_cutoff: i64,
) -> duckdb::Result<()> {
    conn.execute(
        "UPDATE metric_series SET last_seen = recent.taken_at
         FROM (
            SELECT series_id, max(taken_at) AS taken_at FROM metric_points
            WHERE taken_at >= ? GROUP BY series_id
         ) recent
         WHERE metric_series.series_id = recent.series_id
           AND recent.taken_at > metric_series.last_seen",
        [point_cutoff],
    )?;
    conn.execute("DELETE FROM metric_points WHERE taken_at < ?", [point_cutoff])?;
    conn.execute("DELETE FROM metric_rollups WHERE hour < ?", [rollup_cutoff])?;
    conn.execute("DELETE FROM metric_series WHERE last_seen < ?", [rollup_cutoff])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use daroyan_migration::duckdb::run_duckdb_migrations;

    use super::*;
    use crate::analytics::metrics::ingest::MetricKind;

    fn open() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_duckdb_migrations(&conn).unwrap();
        conn
    }

    fn series(series_id: i64) -> SeriesRow {
        SeriesRow {
            series_id,
            token_id: "ing_1".into(),
            name: "job.duration".into(),
            unit: "s".into(),
            description: "".into(),
            kind: MetricKind::Histogram,
            is_monotonic: false,
            temporality: Some(Temporality::Delta),
            service: "api".into(),
            server_id: None,
            instance: None,
            attributes: "{}".to_string(),
            resource: None,
            bounds: Some(vec![1.0, 2.0].into()),
            first_seen: 0,
        }
    }

    fn point(at: i64, buckets: Vec<u64>) -> PointRow {
        PointRow {
            series_id: 1,
            at,
            count: Some(buckets.iter().sum()),
            sum: Some(1.5),
            count_delta: Some(buckets.iter().sum()),
            sum_delta: Some(1.5),
            buckets: Some(buckets.clone()),
            bucket_deltas: Some(buckets),
            ..PointRow::default()
        }
    }

    #[test]
    fn stores_lists_and_rolls_up_by_hour() {
        let conn = open();
        append_metrics(
            &conn,
            &[series(1)],
            &[
                point(1_000, vec![1, 0, 2]),
                point(2_000, vec![0, 3, 1]),
                point(HOUR_MS + 1, vec![5, 0, 0]),
            ],
        )
        .unwrap();

        let bounds: String = conn
            .query_row("SELECT CAST(bounds AS VARCHAR) FROM metric_series", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(bounds, "[1.0, 2.0]");

        roll_up_metrics(&conn, 2 * HOUR_MS + 5).unwrap();
        roll_up_metrics(&conn, 2 * HOUR_MS + 5).unwrap();

        let rows: Vec<(i64, i64, String)> = conn
            .prepare(
                "SELECT hour, points, CAST(bucket_deltas AS VARCHAR)
                 FROM metric_rollups ORDER BY hour",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<duckdb::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            [(0, 2, "[1, 3, 3]".to_string()), (HOUR_MS, 1, "[5, 0, 0]".to_string())]
        );
    }

    #[test]
    fn loads_the_last_point_of_each_series() {
        let conn = open();
        append_metrics(
            &conn,
            &[series(1)],
            &[point(1_000, vec![1, 0, 2]), point(2_000, vec![0, 3, 1])],
        )
        .unwrap();

        let known = list_known_series(&conn, 3_000).unwrap();

        assert_eq!(known.len(), 1);
        assert_eq!(known[0].last_seen, 2_000);
        let last = known[0].last.as_ref().unwrap();
        assert_eq!(last.at, 2_000);
        let Reading::Histogram(histogram) = &last.reading else { panic!() };
        assert_eq!(histogram.buckets, [0, 3, 1]);
    }

    #[test]
    fn purges_old_points_and_quiet_series() {
        let conn = open();
        append_metrics(
            &conn,
            &[series(1)],
            &[point(1_000, vec![1, 0, 0]), point(9_000, vec![1, 0, 0])],
        )
        .unwrap();

        purge_metrics(&conn, 5_000, 0).unwrap();
        let (points, last_seen): (i64, i64) = conn
            .query_row(
                "SELECT (SELECT count(*) FROM metric_points), (SELECT last_seen FROM metric_series)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((points, last_seen), (1, 9_000));

        purge_metrics(&conn, 20_000, 10_000).unwrap();
        let series: i64 = conn
            .query_row("SELECT count(*) FROM metric_series", [], |row| row.get(0))
            .unwrap();
        assert_eq!(series, 0);
    }
}
