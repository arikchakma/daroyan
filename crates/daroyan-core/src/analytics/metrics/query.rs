use std::collections::{BTreeMap, HashMap};

use duckdb::types::Value as SqlValue;
use duckdb::{Connection, OptionalExt, params_from_iter};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::analytics::metrics::histogram::quantile_of;
use crate::analytics::metrics::ingest::MetricKind;
use crate::analytics::metrics::store::{HOUR_MS, to_rollup_select};
use crate::error::{Error, ErrorName, Result};

const RAW_RANGE_MS: i64 = 24 * HOUR_MS;
const MINUTE_MS: i64 = 60_000;
const TARGET_BUCKETS: i64 = 120;
const MAX_BUCKETS: i64 = 1_500;
const MAX_GROUPS: usize = 50;
const MAX_EXEMPLARS: i64 = 200;
const MAX_ATTRIBUTE_VALUES: i64 = 20;
const STEPS_MS: [i64; 8] = [
    MINUTE_MS,
    5 * MINUTE_MS,
    15 * MINUTE_MS,
    30 * MINUTE_MS,
    HOUR_MS,
    3 * HOUR_MS,
    6 * HOUR_MS,
    24 * HOUR_MS,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = MetricAggregation)]
pub enum Aggregation {
    Rate,
    RatePerMinute,
    Increase,
    Last,
    Avg,
    Min,
    Max,
    P50,
    P90,
    P95,
    P99,
    CountRate,
}

impl Aggregation {
    pub fn default_for(kind: MetricKind, is_monotonic: bool) -> Self {
        match kind {
            MetricKind::Sum if is_monotonic => Self::RatePerMinute,
            MetricKind::Histogram => Self::P95,
            MetricKind::Sum | MetricKind::Gauge => Self::Last,
        }
    }

    pub fn fits(self, kind: MetricKind, is_monotonic: bool) -> bool {
        match kind {
            MetricKind::Sum if is_monotonic => {
                matches!(self, Self::Rate | Self::RatePerMinute | Self::Increase)
            }
            MetricKind::Histogram => matches!(
                self,
                Self::P50
                    | Self::P90
                    | Self::P95
                    | Self::P99
                    | Self::Avg
                    | Self::CountRate
            ),
            MetricKind::Sum | MetricKind::Gauge => {
                matches!(self, Self::Last | Self::Avg | Self::Min | Self::Max)
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rate => "rate",
            Self::RatePerMinute => "rate_per_minute",
            Self::Increase => "increase",
            Self::Last => "last",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::P50 => "p50",
            Self::P90 => "p90",
            Self::P95 => "p95",
            Self::P99 => "p99",
            Self::CountRate => "count_rate",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rate => "rate per second",
            Self::RatePerMinute => "rate per minute",
            Self::Increase => "increase",
            Self::Last => "value",
            Self::Avg => "average",
            Self::Min => "minimum",
            Self::Max => "maximum",
            Self::P50 => "p50",
            Self::P90 => "p90",
            Self::P95 => "p95",
            Self::P99 => "p99",
            Self::CountRate => "count per second",
        }
    }

    fn quantile(self) -> Option<f64> {
        match self {
            Self::P50 => Some(0.5),
            Self::P90 => Some(0.9),
            Self::P95 => Some(0.95),
            Self::P99 => Some(0.99),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupBy {
    Service,
    Server,
    Attribute(String),
}

impl GroupBy {
    pub fn read(text: &str) -> Option<Self> {
        match text.trim() {
            "" => None,
            "service" => Some(Self::Service),
            "server" | "server_id" => Some(Self::Server),
            key => Some(Self::Attribute(key.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricFilter {
    pub key: String,
    pub value: String,
    pub is_negated: bool,
}

impl MetricFilter {
    pub fn read_all(text: &str) -> Vec<Self> {
        text.split(',').filter_map(Self::read).collect()
    }

    fn read(text: &str) -> Option<Self> {
        if let Some((key, value)) = text.split_once("!=") {
            return Self::new(key, value, true);
        }

        let (key, value) = text.split_once('=')?;

        Self::new(key, value, false)
    }

    fn new(key: &str, value: &str, is_negated: bool) -> Option<Self> {
        let key = key.trim();
        (!key.is_empty()).then(|| Self {
            key: key.to_string(),
            value: value.trim().to_string(),
            is_negated,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct MetricQuery {
    pub name: String,
    pub aggregation: Option<Aggregation>,
    pub service: Option<String>,
    pub server_id: Option<String>,
    pub filters: Vec<MetricFilter>,
    pub group_by: Option<GroupBy>,
    pub since: i64,
    pub until: i64,
    pub step_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MetricSummary {
    pub name: String,
    pub kind: MetricKind,
    pub is_monotonic: bool,
    pub unit: String,
    pub description: String,
    pub services: Vec<String>,
    pub series: u64,
    pub last_seen_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct AttributeValueCount {
    pub value: String,
    pub series: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MetricAttribute {
    pub key: String,
    pub values: Vec<AttributeValueCount>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MetricLine {
    pub group: Option<String>,
    pub values: Vec<Option<f64>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MetricExemplar {
    pub at: i64,
    pub group: Option<String>,
    pub trace_id: String,
    pub span_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct MetricResult {
    pub name: String,
    pub kind: MetricKind,
    pub is_monotonic: bool,
    pub unit: String,
    pub aggregation: Aggregation,
    pub step_ms: i64,
    pub times: Vec<i64>,
    pub series: Vec<MetricLine>,
    pub is_truncated: bool,
    pub exemplars: Vec<MetricExemplar>,
}

pub async fn list_metrics(
    analytics: &Analytics,
    since: i64,
) -> Result<Vec<MetricSummary>> {
    analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "SELECT name, arg_max(kind, last_seen), bool_or(is_monotonic),
                        arg_max(unit, last_seen), arg_max(description, last_seen),
                        CAST(to_json(list_sort(list(DISTINCT service))) AS VARCHAR),
                        count(*), max(last_seen)
                 FROM metric_series
                 WHERE last_seen >= ?
                 GROUP BY name
                 ORDER BY name",
            )?;
            let rows = statement.query_map([since], |row| {
                let kind: String = row.get(1)?;
                let services: String = row.get(5)?;

                Ok(MetricSummary {
                    name: row.get(0)?,
                    kind: MetricKind::read(&kind).unwrap_or(MetricKind::Gauge),
                    is_monotonic: row.get(2)?,
                    unit: row.get(3)?,
                    description: row.get(4)?,
                    services: serde_json::from_str(&services).unwrap_or_default(),
                    series: row.get(6)?,
                    last_seen_at: row.get(7)?,
                })
            })?;

            rows.collect()
        })
        .await
}

pub async fn list_metric_attributes(
    analytics: &Analytics,
    name: String,
) -> Result<Vec<MetricAttribute>> {
    let rows: Vec<(String, String, u64)> = analytics
        .read(move |conn| {
            let mut statement = conn.prepare(
                "WITH pairs AS (
                    SELECT key, attributes ->> key AS value
                    FROM (
                        SELECT unnest(json_keys(attributes)) AS key, attributes
                        FROM metric_series WHERE name = ?
                    )
                 ),
                 counted AS (
                    SELECT key, value, count(*) AS series,
                           row_number() OVER (PARTITION BY key ORDER BY count(*) DESC, value) AS rank
                    FROM pairs WHERE value IS NOT NULL
                    GROUP BY key, value
                 )
                 SELECT key, value, series FROM counted WHERE rank <= ? ORDER BY key, rank",
            )?;
            let rows = statement.query_map(duckdb::params![name, MAX_ATTRIBUTE_VALUES], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;

            rows.collect()
        })
        .await?;

    let mut attributes: Vec<MetricAttribute> = Vec::new();
    for (key, value, series) in rows {
        let value = AttributeValueCount { value, series };
        match attributes.last_mut() {
            Some(attribute) if attribute.key == key => attribute.values.push(value),
            _ => attributes.push(MetricAttribute { key, values: vec![value] }),
        }
    }

    Ok(attributes)
}

struct Shape {
    kind: MetricKind,
    is_monotonic: bool,
    unit: String,
    bounds: Option<String>,
}

struct Selection {
    sql: String,
    params: Vec<SqlValue>,
}

impl Selection {
    fn new(query: &MetricQuery, bounds: Option<&str>) -> Self {
        let mut params = Vec::new();
        let group = match &query.group_by {
            None => "NULL".to_string(),
            Some(GroupBy::Service) => "s.service".to_string(),
            Some(GroupBy::Server) => "s.server_id".to_string(),
            Some(GroupBy::Attribute(key)) => {
                params.push(SqlValue::Text(key.clone()));
                "s.attributes ->> ?".to_string()
            }
        };

        let mut conditions = vec!["s.name = ?".to_string()];
        params.push(SqlValue::Text(query.name.clone()));

        if let Some(service) = &query.service {
            conditions.push("s.service = ?".to_string());
            params.push(SqlValue::Text(service.clone()));
        }
        if let Some(server_id) = &query.server_id {
            conditions.push("s.server_id = ?".to_string());
            params.push(SqlValue::Text(server_id.clone()));
        }
        if let Some(bounds) = bounds {
            conditions.push("CAST(s.bounds AS VARCHAR) = ?".to_string());
            params.push(SqlValue::Text(bounds.to_string()));
        }
        for filter in &query.filters {
            let operator = if filter.is_negated { "IS DISTINCT FROM" } else { "=" };
            conditions.push(format!("(s.attributes ->> ?) {operator} ?"));
            params.push(SqlValue::Text(filter.key.clone()));
            params.push(SqlValue::Text(filter.value.clone()));
        }

        Self {
            sql: format!(
                "sel AS (SELECT s.series_id, {group} AS g FROM metric_series s WHERE {})",
                conditions.join(" AND ")
            ),
            params,
        }
    }
}

struct Source {
    table: String,
    time: &'static str,
    last: &'static str,
    avg: &'static str,
    min: &'static str,
    max: &'static str,
}

impl Source {
    fn points() -> Self {
        Self {
            table: "metric_points".to_string(),
            time: "taken_at",
            last: "arg_max(value, taken_at)",
            avg: "avg(value)",
            min: "min(coalesce(min, value))",
            max: "max(coalesce(max, value))",
        }
    }

    fn rollups() -> Self {
        let fresh = to_rollup_select(
            &format!("(SELECT coalesce(max(hour) + {HOUR_MS}, 0) FROM metric_rollups)"),
            &i64::MAX.to_string(),
        );

        Self {
            table: format!(
                "(SELECT * FROM metric_rollups UNION ALL BY NAME ({fresh})) AS rolled"
            ),
            time: "hour",
            last: "arg_max(last, hour)",
            avg: "avg(avg)",
            min: "min(min)",
            max: "max(max)",
        }
    }
}

struct Window<'a> {
    source: &'a Source,
    origin: i64,
    step_ms: i64,
    since: i64,
    until: i64,
}

impl Window<'_> {
    fn params(&self) -> [SqlValue; 6] {
        [
            SqlValue::BigInt(self.origin),
            SqlValue::BigInt(self.step_ms),
            SqlValue::BigInt(self.step_ms),
            SqlValue::BigInt(self.origin),
            SqlValue::BigInt(self.since),
            SqlValue::BigInt(self.until),
        ]
    }
}

pub async fn query_metric(
    analytics: &Analytics,
    query: MetricQuery,
) -> Result<MetricResult> {
    analytics.read(move |conn| run_query(conn, &query)).await?
}

fn resolve(
    conn: &Connection,
    query: &MetricQuery,
) -> duckdb::Result<Result<(Shape, Aggregation)>> {
    let Some(shape) = read_shape(conn, query)? else {
        return Ok(Err(Error::new(
            ErrorName::NotFound,
            format!("No metric is called {}", query.name),
        )));
    };

    let aggregation = query
        .aggregation
        .unwrap_or_else(|| Aggregation::default_for(shape.kind, shape.is_monotonic));
    if !aggregation.fits(shape.kind, shape.is_monotonic) {
        return Ok(Err(Error::new(
            ErrorName::ValidationError,
            format!(
                "{} is a {} metric, so it cannot be read with {aggregation:?}",
                query.name,
                shape.kind.as_str()
            ),
        )));
    }

    Ok(Ok((shape, aggregation)))
}

fn read_cells(
    conn: &Connection,
    query: &MetricQuery,
    shape: &Shape,
    aggregation: Aggregation,
    window: &Window,
) -> duckdb::Result<Vec<Cell>> {
    match aggregation.quantile() {
        Some(quantile) => read_quantiles(conn, query, shape, window, quantile),
        None => read_values(conn, query, shape, aggregation, window),
    }
}

fn run_query(
    conn: &Connection,
    query: &MetricQuery,
) -> duckdb::Result<Result<MetricResult>> {
    let (shape, aggregation) = match resolve(conn, query)? {
        Ok(resolved) => resolved,
        Err(error) => return Ok(Err(error)),
    };

    let range = (query.until - query.since).max(MINUTE_MS);
    let source = if range > RAW_RANGE_MS { Source::rollups() } else { Source::points() };
    let step_ms = pick_step(range, query.step_ms, source.time == "hour");
    let since = query.since / step_ms * step_ms;
    let times: Vec<i64> = (0..)
        .map(|index| since + index * step_ms)
        .take_while(|time| *time < query.until)
        .collect();

    let window = Window {
        source: &source,
        origin: 0,
        step_ms,
        since,
        until: query.until,
    };
    let cells = read_cells(conn, query, &shape, aggregation, &window)?;
    let (series, is_truncated) = to_lines(cells, &times, step_ms);
    let exemplars = read_exemplars(conn, query)?;

    Ok(Ok(MetricResult {
        name: query.name.clone(),
        kind: shape.kind,
        is_monotonic: shape.is_monotonic,
        unit: shape.unit,
        aggregation,
        step_ms,
        times,
        series,
        is_truncated,
        exemplars,
    }))
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricReading {
    pub group: Option<String>,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricMeasure {
    pub aggregation: Aggregation,
    pub unit: String,
    pub readings: Vec<MetricReading>,
}

pub async fn measure_metric(
    analytics: &Analytics,
    query: MetricQuery,
    max_groups: usize,
) -> Result<Option<MetricMeasure>> {
    analytics
        .read(move |conn| {
            let (shape, aggregation) = match resolve(conn, &query)? {
                Ok(resolved) => resolved,
                Err(error) if error.name() == ErrorName::NotFound => return Ok(Ok(None)),
                Err(error) => return Ok(Err(error)),
            };

            let source = Source::points();
            let window = Window {
                source: &source,
                origin: query.since,
                step_ms: (query.until - query.since).max(1),
                since: query.since,
                until: query.until,
            };
            let mut readings: Vec<MetricReading> =
                read_cells(conn, &query, &shape, aggregation, &window)?
                    .into_iter()
                    .map(|cell| MetricReading { group: cell.group, value: cell.value })
                    .collect();
            readings.sort_by(|a, b| b.value.total_cmp(&a.value));
            readings.truncate(max_groups);

            Ok(Ok(Some(MetricMeasure { aggregation, unit: shape.unit, readings })))
        })
        .await?
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricWindow {
    pub at: i64,
    pub group: Option<String>,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricWindows {
    pub aggregation: Aggregation,
    pub unit: String,
    pub windows: Vec<MetricWindow>,
}

pub async fn measure_metric_windows(
    analytics: &Analytics,
    query: MetricQuery,
    window_ms: i64,
) -> Result<Option<MetricWindows>> {
    analytics
        .read(move |conn| {
            let (shape, aggregation) = match resolve(conn, &query)? {
                Ok(resolved) => resolved,
                Err(error) if error.name() == ErrorName::NotFound => return Ok(Ok(None)),
                Err(error) => return Ok(Err(error)),
            };

            let source = Source::points();
            let window = Window {
                source: &source,
                origin: 0,
                step_ms: window_ms.max(MINUTE_MS),
                since: query.since,
                until: query.until,
            };
            let mut windows: Vec<MetricWindow> =
                read_cells(conn, &query, &shape, aggregation, &window)?
                    .into_iter()
                    .map(|cell| MetricWindow {
                        at: cell.bucket,
                        group: cell.group,
                        value: cell.value,
                    })
                    .collect();
            windows.sort_by_key(|window| window.at);

            Ok(Ok(Some(MetricWindows { aggregation, unit: shape.unit, windows })))
        })
        .await?
}

pub fn format_metric_value(value: f64, unit: &str, aggregation: Aggregation) -> String {
    let amount = format_amount(value, unit);

    match aggregation {
        Aggregation::RatePerMinute => format!("{amount}/min"),
        Aggregation::Rate => format!("{amount}/s"),
        Aggregation::CountRate => format!("{}/s", format_number(value)),
        Aggregation::P50 | Aggregation::P90 | Aggregation::P95 | Aggregation::P99 => {
            format!("{} {amount}", aggregation.label())
        }
        _ => amount,
    }
}

fn format_amount(value: f64, unit: &str) -> String {
    match unit {
        "s" => format_seconds(value),
        "ms" => format_seconds(value / 1_000.0),
        "By" => format!("{} B", format_number(value)),
        "1" => format!("{}%", format_number(value * 100.0)),
        "%" => format!("{}%", format_number(value)),
        _ => {
            let word = unit.trim_matches(['{', '}']).trim();
            if word.is_empty() {
                return format_number(value);
            }

            let plural = if word.ends_with('s') { "" } else { "s" };
            format!("{} {word}{plural}", format_number(value))
        }
    }
}

fn format_seconds(seconds: f64) -> String {
    if seconds < 1.0 {
        return format!("{} ms", format_number(seconds * 1_000.0));
    }

    format!("{} s", format_number(seconds))
}

fn format_number(value: f64) -> String {
    let text =
        if value.abs() >= 100.0 { format!("{value:.0}") } else { format!("{value:.2}") };
    if !text.contains('.') {
        return text;
    }

    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn read_shape(conn: &Connection, query: &MetricQuery) -> duckdb::Result<Option<Shape>> {
    conn.query_row(
        "SELECT arg_max(kind, last_seen), bool_or(is_monotonic), arg_max(unit, last_seen),
                arg_max(CAST(bounds AS VARCHAR), last_seen)
         FROM metric_series WHERE name = ? HAVING count(*) > 0",
        [&query.name],
        |row| {
            let kind: String = row.get(0)?;

            Ok(Shape {
                kind: MetricKind::read(&kind).unwrap_or(MetricKind::Gauge),
                is_monotonic: row.get(1)?,
                unit: row.get(2)?,
                bounds: row.get(3)?,
            })
        },
    )
    .optional()
}

fn pick_step(range: i64, wanted: Option<i64>, is_hourly: bool) -> i64 {
    let floor = if is_hourly { HOUR_MS } else { MINUTE_MS };
    let floor = floor.max(range / MAX_BUCKETS);
    if let Some(wanted) = wanted {
        return wanted.max(floor);
    }

    let ideal = range / TARGET_BUCKETS;

    STEPS_MS
        .into_iter()
        .find(|step| *step >= ideal && *step >= floor)
        .unwrap_or(24 * HOUR_MS)
}

struct Cell {
    group: Option<String>,
    bucket: i64,
    value: f64,
}

fn read_values(
    conn: &Connection,
    query: &MetricQuery,
    shape: &Shape,
    aggregation: Aggregation,
    window: &Window,
) -> duckdb::Result<Vec<Cell>> {
    let selection = Selection::new(query, None);
    let Source { table, time, .. } = window.source;
    let step_secs = window.step_ms as f64 / 1_000.0;

    let value = match aggregation {
        Aggregation::Rate => Some(format!("sum(delta) / {step_secs}")),
        Aggregation::RatePerMinute => Some(format!("sum(delta) / {}", step_secs / 60.0)),
        Aggregation::Increase => Some("sum(delta)".to_string()),
        Aggregation::CountRate => Some(format!("sum(count_delta) / {step_secs}")),
        Aggregation::Avg if shape.kind == MetricKind::Histogram => {
            Some("sum(sum_delta) / nullif(sum(count_delta), 0)".to_string())
        }
        _ => None,
    };

    let sql = match value {
        Some(value) => format!(
            "WITH {sel}
             SELECT g, ({time} - ?) // ? * ? + ? AS bucket, {value}
             FROM {table} JOIN sel USING (series_id)
             WHERE {time} >= ? AND {time} < ?
             GROUP BY g, bucket",
            sel = selection.sql
        ),
        None => {
            let (inner, outer) = match aggregation {
                Aggregation::Avg => (window.source.avg, "avg"),
                Aggregation::Min => (window.source.min, "min"),
                Aggregation::Max => (window.source.max, "max"),
                _ => (window.source.last, "sum"),
            };
            format!(
                "WITH {sel}
                 SELECT g, bucket, {outer}(v) FROM (
                    SELECT series_id, g, ({time} - ?) // ? * ? + ? AS bucket, {inner} AS v
                    FROM {table} JOIN sel USING (series_id)
                    WHERE {time} >= ? AND {time} < ?
                    GROUP BY series_id, g, bucket
                 )
                 WHERE v IS NOT NULL
                 GROUP BY g, bucket",
                sel = selection.sql
            )
        }
    };

    let mut params = selection.params;
    params.extend(window.params());

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(params), |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<f64>>(2)?,
        ))
    })?;

    let is_rate = matches!(
        aggregation,
        Aggregation::Rate | Aggregation::RatePerMinute | Aggregation::CountRate
    );
    let mut cells = Vec::new();
    for row in rows {
        let (group, bucket, value) = row?;
        let Some(value) = value.filter(|value| value.is_finite()) else { continue };
        let covered = (window.until - bucket).clamp(1, window.step_ms);
        let value =
            if is_rate { value * window.step_ms as f64 / covered as f64 } else { value };
        cells.push(Cell { group, bucket, value });
    }

    Ok(cells)
}

fn read_quantiles(
    conn: &Connection,
    query: &MetricQuery,
    shape: &Shape,
    window: &Window,
    quantile: f64,
) -> duckdb::Result<Vec<Cell>> {
    let bounds: Vec<f64> = shape
        .bounds
        .as_deref()
        .and_then(|bounds| serde_json::from_str(bounds).ok())
        .unwrap_or_default();
    let selection = Selection::new(query, shape.bounds.as_deref());
    let Source { table, time, .. } = window.source;
    let sql = format!(
        "WITH {sel},
         slots AS (
            SELECT g, ({time} - ?) // ? * ? + ? AS bucket,
                   generate_subscripts(bucket_deltas, 1) AS slot,
                   unnest(bucket_deltas) AS count
            FROM {table} JOIN sel USING (series_id)
            WHERE {time} >= ? AND {time} < ? AND bucket_deltas IS NOT NULL
         )
         SELECT g, bucket, CAST(to_json(list(total ORDER BY slot)) AS VARCHAR)
         FROM (SELECT g, bucket, slot, sum(count) AS total FROM slots GROUP BY g, bucket, slot)
         GROUP BY g, bucket",
        sel = selection.sql
    );

    let mut params = selection.params;
    params.extend(window.params());

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(params), |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get(1)?, row.get::<_, String>(2)?))
    })?;

    let mut cells = Vec::new();
    for row in rows {
        let (group, bucket, buckets) = row?;
        let buckets: Vec<u64> = serde_json::from_str(&buckets).unwrap_or_default();
        if let Some(value) = quantile_of(&bounds, &buckets, quantile) {
            cells.push(Cell { group, bucket, value });
        }
    }

    Ok(cells)
}

fn to_lines(cells: Vec<Cell>, times: &[i64], step_ms: i64) -> (Vec<MetricLine>, bool) {
    let mut groups: HashMap<Option<String>, BTreeMap<i64, f64>> = HashMap::new();
    for cell in cells {
        groups.entry(cell.group).or_default().insert(cell.bucket, cell.value);
    }

    let mut ranked: Vec<(Option<String>, BTreeMap<i64, f64>, f64)> = groups
        .into_iter()
        .map(|(group, values)| {
            let total = values.values().map(|value| value.abs()).sum();
            (group, values, total)
        })
        .collect();
    ranked.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

    let is_truncated = ranked.len() > MAX_GROUPS;
    let lines = ranked
        .into_iter()
        .take(MAX_GROUPS)
        .map(|(group, values, _)| MetricLine {
            group,
            values: times
                .iter()
                .map(|time| values.get(&(time / step_ms * step_ms)).copied())
                .collect(),
        })
        .collect();

    (lines, is_truncated)
}

fn read_exemplars(
    conn: &Connection,
    query: &MetricQuery,
) -> duckdb::Result<Vec<MetricExemplar>> {
    let selection = Selection::new(query, None);
    let sql = format!(
        "WITH {sel}
         SELECT g, taken_at, trace_id, span_id
         FROM metric_points JOIN sel USING (series_id)
         WHERE taken_at >= ? AND taken_at < ? AND trace_id IS NOT NULL
         ORDER BY taken_at DESC
         LIMIT ?",
        sel = selection.sql
    );

    let mut params = selection.params;
    params.extend([
        SqlValue::BigInt(query.since),
        SqlValue::BigInt(query.until),
        SqlValue::BigInt(MAX_EXEMPLARS),
    ]);

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(params), |row| {
        Ok(MetricExemplar {
            group: row.get(0)?,
            at: row.get(1)?,
            trace_id: row.get(2)?,
            span_id: row.get(3)?,
        })
    })?;

    rows.collect()
}

#[cfg(test)]
mod tests {
    use daroyan_migration::duckdb::run_duckdb_migrations;

    use super::*;
    use crate::analytics::metrics::ingest::{SeriesRow, Temporality};
    use crate::analytics::metrics::series::PointRow;
    use crate::analytics::metrics::store::{append_metrics, roll_up_metrics};

    const DAY_MS: i64 = 24 * HOUR_MS;

    fn series(series_id: i64, name: &str, kind: MetricKind, route: &str) -> SeriesRow {
        SeriesRow {
            series_id,
            token_id: "ing_1".into(),
            name: name.into(),
            unit: "".into(),
            description: "".into(),
            kind,
            is_monotonic: kind == MetricKind::Sum,
            temporality: Some(Temporality::Delta),
            service: "api".into(),
            server_id: None,
            instance: None,
            attributes: format!(r#"{{"route":"{route}"}}"#),
            resource: None,
            bounds: None,
            first_seen: 0,
        }
    }

    #[tokio::test]
    async fn reads_long_ranges_from_hourly_rollups() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-metric-query-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics.run(run_duckdb_migrations).unwrap();

        let start = 10 * DAY_MS;
        let points: Vec<PointRow> = (0..120)
            .flat_map(|minute| {
                let at = start + minute * MINUTE_MS;
                [
                    PointRow {
                        series_id: 1,
                        at,
                        value: Some(1.0),
                        delta: Some(1.0),
                        ..PointRow::default()
                    },
                    PointRow {
                        series_id: 2,
                        at,
                        value: Some(2.0),
                        delta: Some(2.0),
                        ..PointRow::default()
                    },
                    PointRow {
                        series_id: 3,
                        at,
                        value: Some(minute as f64),
                        ..PointRow::default()
                    },
                ]
            })
            .collect();
        let rows = [
            series(1, "emails.sent", MetricKind::Sum, "/a"),
            series(2, "emails.sent", MetricKind::Sum, "/b"),
            series(3, "queue.depth", MetricKind::Gauge, "/a"),
        ];
        analytics
            .run(|conn| {
                append_metrics(conn, &rows, &points)?;
                roll_up_metrics(conn, start + 3 * HOUR_MS)
            })
            .unwrap();

        let query = |name: &str, aggregation, group_by| MetricQuery {
            name: name.to_string(),
            aggregation: Some(aggregation),
            group_by,
            since: start - DAY_MS,
            until: start + DAY_MS,
            step_ms: Some(HOUR_MS),
            ..MetricQuery::default()
        };

        let sent = query_metric(
            &analytics,
            query("emails.sent", Aggregation::Increase, GroupBy::read("route")),
        )
        .await
        .unwrap();
        assert_eq!(sent.step_ms, HOUR_MS);
        assert_eq!(sent.series.len(), 2);
        assert_eq!(sent.series[0].group.as_deref(), Some("/b"));
        let busy: Vec<f64> = sent.series[0].values.iter().flatten().copied().collect();
        assert_eq!(busy, [120.0, 120.0]);

        let quiet: Vec<f64> = sent.series[1].values.iter().flatten().copied().collect();
        assert_eq!(quiet, [60.0, 60.0]);

        let fresh = PointRow {
            series_id: 1,
            at: start + 3 * HOUR_MS + 5 * MINUTE_MS,
            value: Some(1.0),
            delta: Some(4.0),
            ..PointRow::default()
        };
        analytics.run(|conn| append_metrics(conn, &[], &[fresh])).unwrap();
        let sent = query_metric(
            &analytics,
            query("emails.sent", Aggregation::Increase, GroupBy::read("route")),
        )
        .await
        .unwrap();
        let quiet: Vec<f64> = sent.series[1].values.iter().flatten().copied().collect();
        assert_eq!(quiet, [60.0, 60.0, 4.0]);

        let depth =
            query_metric(&analytics, query("queue.depth", Aggregation::Max, None))
                .await
                .unwrap();
        let peaks: Vec<f64> = depth.series[0].values.iter().flatten().copied().collect();
        assert_eq!(peaks, [59.0, 119.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_filters() {
        assert_eq!(
            MetricFilter::read_all("route=/a, code != 500,=x,broken"),
            [
                MetricFilter {
                    key: "route".into(),
                    value: "/a".into(),
                    is_negated: false
                },
                MetricFilter {
                    key: "code".into(),
                    value: "500".into(),
                    is_negated: true
                },
            ]
        );
    }

    #[test]
    fn picks_a_step_for_about_120_buckets() {
        assert_eq!(pick_step(HOUR_MS, None, false), MINUTE_MS);
        assert_eq!(pick_step(24 * HOUR_MS, None, false), 15 * MINUTE_MS);
        assert_eq!(pick_step(7 * 24 * HOUR_MS, None, true), 3 * HOUR_MS);
        assert_eq!(pick_step(HOUR_MS, Some(1_000), false), MINUTE_MS);
        assert_eq!(pick_step(30 * 24 * HOUR_MS, Some(HOUR_MS), true), HOUR_MS);
    }

    #[test]
    fn only_allows_aggregations_that_fit_the_kind() {
        assert!(Aggregation::Rate.fits(MetricKind::Sum, true));
        assert!(!Aggregation::Rate.fits(MetricKind::Gauge, false));
        assert!(!Aggregation::P95.fits(MetricKind::Sum, false));
        assert!(Aggregation::Avg.fits(MetricKind::Histogram, false));
        assert_eq!(
            Aggregation::default_for(MetricKind::Histogram, false),
            Aggregation::P95
        );
    }
}
