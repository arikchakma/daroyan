use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use duckdb::types::{Type, Value as SqlValue};
use duckdb::{Connection, OptionalExt, Row, params_from_iter};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::analytics::Analytics;
use crate::analytics::metrics::histogram::quantile_of;
use crate::analytics::metrics::ingest::MetricKind;
use crate::analytics::metrics::store::HOUR_MS;
use crate::error::{Error, ErrorName, Result};

const RAW_RANGE_MS: i64 = 24 * HOUR_MS;
const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const SETTLING_MS: i64 = 3 * HOUR_MS;
const STALE_MS: i64 = 5 * MINUTE_MS;
const TARGET_BUCKETS: i64 = 120;
const MAX_BUCKETS: i64 = 1_500;
const MAX_GROUPS: usize = 50;
const MAX_EXEMPLARS: i64 = 200;
const MAX_ATTRIBUTE_VALUES: i64 = 20;
const MAX_WINDOWS: i64 = 10_080;
const MAX_WINDOW_CELLS: i64 = 50_000;
const STEPS_MS: [i64; 8] = [
    MINUTE_MS,
    5 * MINUTE_MS,
    15 * MINUTE_MS,
    30 * MINUTE_MS,
    HOUR_MS,
    3 * HOUR_MS,
    6 * HOUR_MS,
    DAY_MS,
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
                    | Self::Min
                    | Self::Max
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum GroupBy {
    Service,
    Server,
    Attribute(String),
}

impl GroupBy {
    pub fn read(text: &str) -> Option<Self> {
        let text = text.trim();

        (!text.is_empty()).then(|| Self::from(text))
    }

    pub fn attribute(&self) -> Option<&str> {
        match self {
            Self::Attribute(key) => Some(key),
            Self::Service | Self::Server => None,
        }
    }
}

impl From<&str> for GroupBy {
    fn from(text: &str) -> Self {
        match text.trim() {
            "service" => Self::Service,
            "server" | "server_id" => Self::Server,
            key => Self::Attribute(key.to_string()),
        }
    }
}

impl From<String> for GroupBy {
    fn from(text: String) -> Self {
        if text.trim().len() != text.len() {
            return Self::from(text.as_str());
        }

        match text.as_str() {
            "service" => Self::Service,
            "server" | "server_id" => Self::Server,
            _ => Self::Attribute(text),
        }
    }
}

impl From<GroupBy> for String {
    fn from(group_by: GroupBy) -> Self {
        group_by.to_string()
    }
}

impl fmt::Display for GroupBy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Service => formatter.write_str("service"),
            Self::Server => formatter.write_str("server"),
            Self::Attribute(key) => formatter.write_str(key),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ranking {
    pub max_groups: usize,
    pub is_lowest_first: bool,
}

impl Ranking {
    fn compare(self, a: f64, b: f64) -> Ordering {
        if self.is_lowest_first { a.total_cmp(&b) } else { b.total_cmp(&a) }
    }

    fn pick_groups(self, windows: &[MetricWindow]) -> HashSet<Option<String>> {
        let mut extremes: HashMap<&Option<String>, f64> = HashMap::new();
        for window in windows {
            extremes
                .entry(&window.group)
                .and_modify(|value| {
                    if self.compare(window.value, *value) == Ordering::Less {
                        *value = window.value;
                    }
                })
                .or_insert(window.value);
        }

        let mut ranked: Vec<(&Option<String>, f64)> = extremes.into_iter().collect();
        ranked.sort_by(|a, b| self.compare(a.1, b.1).then_with(|| a.0.cmp(b.0)));

        ranked
            .into_iter()
            .take(self.max_groups)
            .map(|(group, _)| group.clone())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricFilter {
    pub key: String,
    pub value: String,
    pub is_negated: bool,
}

impl FromStr for MetricFilter {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        let split = match text.split_once("!=") {
            Some((key, value)) => Some((key, value, true)),
            None => text.split_once('=').map(|(key, value)| (key, value, false)),
        };

        let Some((key, value, is_negated)) =
            split.filter(|(key, _, _)| !key.trim().is_empty())
        else {
            return Err(Error::new(
                ErrorName::ValidationError,
                format!("Write the filter {text:?} as key=value or key!=value"),
            ));
        };

        Ok(Self {
            key: key.trim().to_string(),
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
    pub span_name: Option<String>,
    pub duration_us: Option<i64>,
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
                "WITH seen AS (
                    SELECT s.*, greatest(s.last_seen, coalesce(p.taken_at, 0)) AS seen
                    FROM metric_series s
                    LEFT JOIN (
                        SELECT series_id, max(taken_at) AS taken_at FROM metric_points
                        WHERE taken_at >= ? GROUP BY series_id
                    ) p USING (series_id)
                 )
                 SELECT name, arg_max(kind, seen), bool_or(is_monotonic),
                        arg_max(unit, seen), arg_max(description, seen),
                        CAST(to_json(list_sort(list(DISTINCT service))) AS VARCHAR),
                        count(*), max(seen)
                 FROM seen
                 WHERE seen >= ?
                 GROUP BY name
                 ORDER BY name",
            )?;
            let rows = statement.query_map([since, since], |row| {
                Ok(MetricSummary {
                    name: row.get(0)?,
                    kind: read_kind(row, 1)?,
                    is_monotonic: row.get(2)?,
                    unit: row.get(3)?,
                    description: row.get(4)?,
                    services: read_json(row, 5)?,
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
                    SELECT key, attributes ->> ('/' || replace(replace(key, '~', '~0'), '/', '~1')) AS value
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
    edges: Vec<f64>,
}

impl Shape {
    fn pick(&self, name: &str, wanted: Option<Aggregation>) -> Result<Aggregation> {
        let aggregation = wanted
            .unwrap_or_else(|| Aggregation::default_for(self.kind, self.is_monotonic));
        if aggregation.fits(self.kind, self.is_monotonic) {
            return Ok(aggregation);
        }

        Err(Error::new(
            ErrorName::ValidationError,
            format!(
                "{name} is a {} metric, so it cannot be read with {}",
                self.kind.as_str(),
                aggregation.as_str()
            ),
        ))
    }
}

fn to_json_pointer(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
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
                params.push(SqlValue::Text(to_json_pointer(key)));
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
            params.push(SqlValue::Text(to_json_pointer(&filter.key)));
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

enum Source {
    Points,
    Hourly { from: i64, to: i64 },
}

struct Edges {
    since: i64,
    until: i64,
    points_since: i64,
}

impl Edges {
    fn to_source(&self, settled: i64) -> Source {
        let from = if self.since >= self.points_since {
            ceil_hour(self.since)
        } else {
            floor_hour(self.since)
        };
        let last = if self.until >= self.points_since {
            floor_hour(self.until)
        } else {
            ceil_hour(self.until)
        };

        let settled = settled.max(ceil_hour(self.points_since));

        Source::Hourly { from, to: last.min(settled).max(from) }
    }
}

fn floor_hour(at: i64) -> i64 {
    at.div_euclid(HOUR_MS) * HOUR_MS
}

fn ceil_hour(at: i64) -> i64 {
    floor_hour(at.saturating_add(HOUR_MS - 1))
}

fn read_settled(conn: &Connection) -> duckdb::Result<i64> {
    conn.query_row(
        &format!(
            "SELECT coalesce(max(hour) + {HOUR_MS}, 0) - {SETTLING_MS} FROM metric_rollups"
        ),
        [],
        |row| row.get(0),
    )
}

struct Window {
    source: Source,
    origin: i64,
    step_ms: i64,
    since: i64,
    until: i64,
    limit: Option<i64>,
}

impl Window {
    fn lookback(&self) -> i64 {
        self.step_ms.min(HOUR_MS)
    }

    fn bucket(&self) -> String {
        let Self { origin, step_ms, since, .. } = self;

        format!(
            "(greatest(point_at, {since}) - {origin}) // {step_ms} * {step_ms} + {origin}"
        )
    }

    fn covered_secs(&self) -> String {
        let step_ms = self.step_ms;
        let cap = step_ms.max(HOUR_MS);

        format!(
            "(coalesce(nullif(least(max(end_at) - min(coalesce(begin_at, {})), {cap}), 0), {step_ms}) / 1000.0)",
            self.bucket()
        )
    }

    fn order(&self) -> String {
        self.limit
            .map(|limit| format!("ORDER BY bucket DESC LIMIT {limit}"))
            .unwrap_or_default()
    }

    fn rows(&self) -> String {
        let Self { since, until, .. } = *self;
        let start = since.saturating_sub(self.lookback());

        let (scan, keep, rollups) = match self.source {
            Source::Points => (
                format!("p.taken_at >= {start} AND p.taken_at < {until}"),
                format!("point_at >= {since}"),
                String::new(),
            ),
            Source::Hourly { from, to } => (
                format!(
                    "(p.taken_at >= {start} AND p.taken_at < {from})
                     OR (p.taken_at >= {} AND p.taken_at < {until})",
                    to.saturating_sub(self.lookback())
                ),
                format!("point_at >= {since} AND (point_at < {from} OR point_at >= {to})"),
                format!(
                    "UNION ALL
                     SELECT r.series_id, sel.g, r.hour, r.hour, r.hour + {HOUR_MS}, {HOUR_MS},
                            r.points, r.last, r.min, r.max, r.avg, r.delta, r.count_delta,
                            r.sum_delta, r.bucket_deltas
                     FROM metric_rollups r JOIN sel USING (series_id)
                     WHERE r.hour >= {from} AND r.hour < {to}"
                ),
            ),
        };

        format!(
            "rows AS (
                SELECT * FROM (
                    SELECT p.series_id, sel.g, p.taken_at AS point_at,
                           greatest(lag(p.taken_at) OVER series, p.start_at) AS begin_at,
                           p.taken_at AS end_at,
                           p.taken_at - lag(p.taken_at) OVER series AS gap,
                           1 AS points, p.value AS last,
                           coalesce(p.min, p.value) AS min, coalesce(p.max, p.value) AS max,
                           p.value AS avg, p.delta, p.count_delta, p.sum_delta, p.bucket_deltas
                    FROM metric_points p JOIN sel USING (series_id)
                    WHERE {scan}
                    WINDOW series AS (PARTITION BY p.series_id ORDER BY p.taken_at)
                ) AS points WHERE {keep}
                {rollups}
            )"
        )
    }
}

struct Lookup<'a> {
    query: &'a MetricQuery,
    shape: &'a Shape,
    aggregation: Aggregation,
}

pub async fn query_metric(
    analytics: &Analytics,
    query: MetricQuery,
    points_since: i64,
) -> Result<MetricResult> {
    let Some((shape, aggregation)) = resolve(analytics, &query).await? else {
        return Err(Error::new(
            ErrorName::NotFound,
            format!("No metric is called {}", query.name),
        ));
    };

    analytics
        .read(move |conn| {
            let lookup = Lookup { query: &query, shape: &shape, aggregation };

            run_query(conn, &lookup, points_since)
        })
        .await
}

pub async fn check_aggregation(
    analytics: &Analytics,
    name: &str,
    aggregation: Aggregation,
) -> Result<()> {
    let query = MetricQuery {
        name: name.to_string(),
        aggregation: Some(aggregation),
        ..MetricQuery::default()
    };
    resolve(analytics, &query).await?;

    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricFormat {
    pub aggregation: Aggregation,
    pub unit: String,
}

impl MetricFormat {
    pub fn format(&self, value: f64) -> String {
        format_metric_value(value, &self.unit, self.aggregation)
    }
}

pub async fn find_metric_format(
    analytics: &Analytics,
    name: &str,
    aggregation: Option<Aggregation>,
) -> Result<Option<MetricFormat>> {
    let query = MetricQuery {
        name: name.to_string(),
        aggregation,
        ..MetricQuery::default()
    };
    let resolved = resolve(analytics, &query).await?;

    Ok(resolved
        .map(|(shape, aggregation)| MetricFormat { aggregation, unit: shape.unit }))
}

async fn resolve(
    analytics: &Analytics,
    query: &MetricQuery,
) -> Result<Option<(Shape, Aggregation)>> {
    let name = query.name.clone();
    let Some(shape) = analytics.read(move |conn| read_shape(conn, &name)).await? else {
        return Ok(None);
    };

    let aggregation = shape.pick(&query.name, query.aggregation)?;

    Ok(Some((shape, aggregation)))
}

fn read_cells(
    conn: &Connection,
    lookup: &Lookup,
    window: &Window,
) -> duckdb::Result<Vec<Cell>> {
    match lookup.aggregation.quantile() {
        Some(quantile) => read_quantiles(conn, lookup, window, quantile),
        None => read_values(conn, lookup, window),
    }
}

fn run_query(
    conn: &Connection,
    lookup: &Lookup,
    points_since: i64,
) -> duckdb::Result<MetricResult> {
    let query = lookup.query;
    let range = query.until.saturating_sub(query.since).max(MINUTE_MS);
    let is_hourly = range > RAW_RANGE_MS || query.since < points_since;
    let is_single = query.step_ms.is_some_and(|wanted| wanted >= range);

    let step_ms =
        if is_single { range } else { pick_step(range, query.step_ms, is_hourly) };
    let origin = if is_single { query.since } else { 0 };
    let since =
        if is_single { query.since } else { query.since.div_euclid(step_ms) * step_ms };

    let times: Vec<i64> =
        std::iter::successors(Some(since), |time| time.checked_add(step_ms))
            .take_while(|time| *time < query.until)
            .collect();

    let source = if is_hourly {
        let edges = Edges { since, until: query.until, points_since };
        edges.to_source(read_settled(conn)?)
    } else {
        Source::Points
    };
    let window = Window {
        source,
        origin,
        step_ms,
        since,
        until: query.until,
        limit: None,
    };

    let cells = read_cells(conn, lookup, &window)?;
    let (series, is_truncated) = to_lines(cells, &times);
    let exemplars = read_exemplars(conn, query)?;

    Ok(MetricResult {
        name: query.name.clone(),
        kind: lookup.shape.kind,
        is_monotonic: lookup.shape.is_monotonic,
        unit: lookup.shape.unit.clone(),
        aggregation: lookup.aggregation,
        step_ms,
        times,
        series,
        is_truncated,
        exemplars,
    })
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
    pub cut_groups: Vec<Option<String>>,
}

pub async fn measure_metric(
    analytics: &Analytics,
    query: MetricQuery,
    ranking: Ranking,
) -> Result<Option<MetricMeasure>> {
    let Some((shape, aggregation)) = resolve(analytics, &query).await? else {
        return Ok(None);
    };

    let unit = shape.unit.clone();
    let cells = analytics
        .read(move |conn| {
            let window = Window {
                source: Source::Points,
                origin: query.since,
                step_ms: query.until.saturating_sub(query.since).max(1),
                since: query.since,
                until: query.until,
                limit: None,
            };
            let lookup = Lookup { query: &query, shape: &shape, aggregation };

            read_cells(conn, &lookup, &window)
        })
        .await?;

    let mut readings: Vec<MetricReading> = cells
        .into_iter()
        .map(|cell| MetricReading { group: cell.group, value: cell.value })
        .collect();
    readings.sort_by(|a, b| ranking.compare(a.value, b.value));

    let cut = readings.split_off(ranking.max_groups.min(readings.len()));
    let cut_groups = cut.into_iter().map(|reading| reading.group).collect();

    Ok(Some(MetricMeasure { aggregation, unit, readings, cut_groups }))
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
    ranking: Ranking,
) -> Result<Option<MetricWindows>> {
    let Some((shape, aggregation)) = resolve(analytics, &query).await? else {
        return Ok(None);
    };

    let unit = shape.unit.clone();
    let step_ms = window_ms.max(MINUTE_MS);
    let earliest = query.until.saturating_sub(step_ms.saturating_mul(MAX_WINDOWS));
    let start = query.since.max(earliest);
    let offset = start.rem_euclid(step_ms);
    let since = if offset == 0 { start } else { start.saturating_add(step_ms - offset) };

    let cells = analytics
        .read(move |conn| {
            let window = Window {
                source: Source::Points,
                origin: 0,
                step_ms,
                since,
                until: query.until,
                limit: Some(MAX_WINDOW_CELLS),
            };
            let lookup = Lookup { query: &query, shape: &shape, aggregation };

            read_cells(conn, &lookup, &window)
        })
        .await?;

    let mut windows: Vec<MetricWindow> = cells
        .into_iter()
        .map(|cell| MetricWindow {
            at: cell.bucket,
            group: cell.group,
            value: cell.value,
        })
        .collect();

    let kept = ranking.pick_groups(&windows);
    windows.retain(|window| kept.contains(&window.group));
    windows.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.group.cmp(&b.group)));

    Ok(Some(MetricWindows { aggregation, unit, windows }))
}

const BYTE_UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
const COMPACT_UNITS: [(f64, &str); 4] = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")];

pub fn format_metric_value(value: f64, unit: &str, aggregation: Aggregation) -> String {
    match aggregation {
        Aggregation::CountRate => format!("{}/s", format_number(value)),
        Aggregation::Rate => format!("{}/s", format_amount(value, unit)),
        Aggregation::RatePerMinute => format!("{}/min", format_amount(value, unit)),
        _ => format_amount(value, unit),
    }
}

fn format_amount(value: f64, unit: &str) -> String {
    if let Some(micros) = to_micros(unit).filter(|_| value >= 0.0) {
        return format_micros(value * micros);
    }

    if let Some(size) = to_byte_size(unit).filter(|_| value >= 0.0) {
        return format_bytes(value * size);
    }

    let word = to_unit_word(unit);
    if word.is_empty() {
        return format_number(value);
    }

    if word.starts_with('%') || word.starts_with('/') {
        return format!("{}{word}", format_number(value));
    }

    format!("{} {word}", format_number(value))
}

fn to_micros(unit: &str) -> Option<f64> {
    match unit {
        "s" => Some(1_000_000.0),
        "ms" => Some(1_000.0),
        "us" => Some(1.0),
        "ns" => Some(0.001),
        _ => None,
    }
}

fn to_byte_size(unit: &str) -> Option<f64> {
    match unit {
        "By" => Some(1.0),
        "KiBy" => Some(1_024.0),
        "MiBy" => Some(1_024_f64.powi(2)),
        "GiBy" => Some(1_024_f64.powi(3)),
        "TiBy" => Some(1_024_f64.powi(4)),
        "KBy" => Some(1_000.0),
        "MBy" => Some(1_000_f64.powi(2)),
        "GBy" => Some(1_000_f64.powi(3)),
        _ => None,
    }
}

fn to_unit_word(unit: &str) -> String {
    let known = match unit {
        "1" => Some(""),
        "%" => Some("%"),
        "us" => Some("µs"),
        "By" => Some("bytes"),
        "KiBy" => Some("KiB"),
        "MiBy" => Some("MiB"),
        "GiBy" => Some("GiB"),
        "TiBy" => Some("TiB"),
        "KBy" => Some("kB"),
        "MBy" => Some("MB"),
        "GBy" => Some("GB"),
        "bit" => Some("bits"),
        "By/s" => Some("B/s"),
        "Cel" => Some("°C"),
        _ => None,
    };
    if let Some(word) = known {
        return word.to_string();
    }

    let annotation = unit
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .map(str::trim)
        .filter(|annotation| !annotation.is_empty());
    match annotation {
        Some(word) if word.ends_with('s') => word.to_string(),
        Some(word) => format!("{word}s"),
        None => unit.replace(['{', '}'], "").trim().to_string(),
    }
}

fn format_micros(micros: f64) -> String {
    if micros < 1_000.0 {
        return format!("{} µs", micros.round());
    }

    if micros < 1_000_000.0 {
        let ms = micros / 1_000.0;
        let text =
            if ms < 10.0 { trim_zeros(format!("{ms:.1}")) } else { format!("{ms:.0}") };
        return format!("{text} ms");
    }

    let seconds = micros / 1_000_000.0;
    let text = if seconds < 10.0 {
        trim_zeros(format!("{seconds:.2}"))
    } else {
        format!("{seconds:.1}")
    };

    format!("{text} s")
}

fn format_bytes(bytes: f64) -> String {
    let mut value = bytes;
    let mut index = 0;
    while value >= 1_024.0 && index < BYTE_UNITS.len() - 1 {
        value /= 1_024.0;
        index += 1;
    }

    let text = if value >= 10.0 || value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };

    format!("{text} {}", BYTE_UNITS[index])
}

fn format_number(value: f64) -> String {
    if value.abs() >= 10_000.0 {
        let (size, suffix) = COMPACT_UNITS
            .into_iter()
            .find(|(size, _)| value.abs() >= *size)
            .unwrap_or((1.0, ""));
        let text = trim_zeros(format!("{:.1}", value / size));

        return format!("{text}{suffix}");
    }

    let text = trim_zeros(format!("{:.2}", value.abs()));
    let (whole, fraction) = text
        .split_once('.')
        .map_or((text.as_str(), None), |(whole, fraction)| (whole, Some(fraction)));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }

    let sign = if value < 0.0 && text != "0" { "-" } else { "" };
    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

fn trim_zeros(text: String) -> String {
    if !text.contains('.') {
        return text;
    }

    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn read_kind(row: &Row, column: usize) -> duckdb::Result<MetricKind> {
    let text: String = row.get(column)?;

    text.parse().map_err(|error: String| {
        duckdb::Error::FromSqlConversionFailure(column, Type::Text, error.into())
    })
}

fn read_json<T: DeserializeOwned>(row: &Row, column: usize) -> duckdb::Result<T> {
    let text: String = row.get(column)?;

    serde_json::from_str(&text).map_err(|error| {
        duckdb::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn read_shape(conn: &Connection, name: &str) -> duckdb::Result<Option<Shape>> {
    conn.query_row(
        "SELECT arg_max(kind, last_seen), bool_or(is_monotonic), arg_max(unit, last_seen),
                arg_max(CAST(bounds AS VARCHAR), last_seen)
         FROM metric_series WHERE name = ? HAVING count(*) > 0",
        [name],
        |row| {
            let bounds: Option<String> = row.get(3)?;
            let edges = match bounds {
                Some(_) => read_json(row, 3)?,
                None => Vec::new(),
            };

            Ok(Shape {
                kind: read_kind(row, 0)?,
                is_monotonic: row.get(1)?,
                unit: row.get(2)?,
                bounds,
                edges,
            })
        },
    )
    .optional()
}

fn pick_step(range: i64, wanted: Option<i64>, is_hourly: bool) -> i64 {
    let grain = if is_hourly { HOUR_MS } else { MINUTE_MS };
    let floor = (range / (MAX_BUCKETS - 1) / grain + 1) * grain;
    if let Some(wanted) = wanted {
        let step = wanted.max(floor);
        if !is_hourly {
            return step;
        }

        return ceil_hour(step);
    }

    let ideal = range / TARGET_BUCKETS;

    STEPS_MS
        .into_iter()
        .find(|step| *step >= ideal && *step >= floor)
        .unwrap_or(floor.max(DAY_MS))
}

struct Cell {
    group: Option<String>,
    bucket: i64,
    value: f64,
}

struct ValueSql {
    value: String,
    weight: &'static str,
    outer: &'static str,
}

impl ValueSql {
    fn new(value: impl Into<String>, outer: &'static str) -> Self {
        Self { value: value.into(), weight: "NULL", outer }
    }
}

fn read_values(
    conn: &Connection,
    lookup: &Lookup,
    window: &Window,
) -> duckdb::Result<Vec<Cell>> {
    let selection = Selection::new(lookup.query, None);
    let covered = window.covered_secs();

    let ValueSql { value, weight, outer } = match lookup.aggregation {
        Aggregation::Rate => ValueSql::new(format!("sum(delta) / {covered}"), "sum(v)"),
        Aggregation::RatePerMinute => {
            ValueSql::new(format!("sum(delta) * 60 / {covered}"), "sum(v)")
        }
        Aggregation::Increase => ValueSql::new("sum(delta)", "sum(v)"),
        Aggregation::CountRate => {
            ValueSql::new(format!("sum(count_delta) / {covered}"), "sum(v)")
        }
        Aggregation::Avg if lookup.shape.kind == MetricKind::Histogram => ValueSql {
            value: "sum(sum_delta)".to_string(),
            weight: "sum(count_delta)",
            outer: "sum(v) / nullif(sum(w), 0)",
        },
        Aggregation::Avg => ValueSql::new(
            "sum(avg * points) / nullif(sum(points) FILTER (WHERE avg IS NOT NULL), 0)",
            "avg(v)",
        ),
        Aggregation::Min => ValueSql::new("min(min)", "min(v)"),
        Aggregation::Max => ValueSql::new("max(max)", "max(v)"),
        _ => ValueSql::new("arg_max(last, point_at)", "sum(v)"),
    };

    let fresh = if lookup.aggregation == Aggregation::Last {
        format!(
            "AND seen + greatest({STALE_MS}, 2 * coalesce(gap, 0)) >= least(bucket + {}, {})",
            window.step_ms, window.until
        )
    } else {
        String::new()
    };

    let sql = format!(
        "WITH {sel}, {rows}
         SELECT g, bucket, {outer} FROM (
            SELECT series_id, g, {bucket} AS bucket, {value} AS v, {weight} AS w,
                   max(end_at) AS seen, median(gap) AS gap
            FROM rows
            GROUP BY series_id, g, bucket
         )
         WHERE v IS NOT NULL {fresh}
         GROUP BY g, bucket
         {order}",
        sel = selection.sql,
        rows = window.rows(),
        bucket = window.bucket(),
        order = window.order(),
    );

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(selection.params), |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<f64>>(2)?,
        ))
    })?;

    let mut cells = Vec::new();
    for row in rows {
        let (group, bucket, value) = row?;
        if let Some(value) = value.filter(|value| value.is_finite()) {
            cells.push(Cell { group, bucket, value });
        }
    }

    Ok(cells)
}

fn read_quantiles(
    conn: &Connection,
    lookup: &Lookup,
    window: &Window,
    quantile: f64,
) -> duckdb::Result<Vec<Cell>> {
    let shape = lookup.shape;
    let selection = Selection::new(lookup.query, shape.bounds.as_deref());
    let sql = format!(
        "WITH {sel}, {rows},
         slots AS (
            SELECT g, {bucket} AS bucket,
                   generate_subscripts(bucket_deltas, 1) AS slot,
                   unnest(bucket_deltas) AS count
            FROM rows
            WHERE bucket_deltas IS NOT NULL
         )
         SELECT g, bucket, CAST(to_json(list(total ORDER BY slot)) AS VARCHAR)
         FROM (SELECT g, bucket, slot, sum(count) AS total FROM slots GROUP BY g, bucket, slot)
         GROUP BY g, bucket
         {order}",
        sel = selection.sql,
        rows = window.rows(),
        bucket = window.bucket(),
        order = window.order(),
    );

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(selection.params), |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, i64>(1)?,
            read_json::<Vec<u64>>(row, 2)?,
        ))
    })?;

    let mut cells = Vec::new();
    for row in rows {
        let (group, bucket, buckets) = row?;
        if let Some(value) = quantile_of(&shape.edges, &buckets, quantile) {
            cells.push(Cell { group, bucket, value });
        }
    }

    Ok(cells)
}

fn to_lines(cells: Vec<Cell>, times: &[i64]) -> (Vec<MetricLine>, bool) {
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
            values: times.iter().map(|time| values.get(time).copied()).collect(),
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
        "WITH {sel},
         picked AS (
            SELECT g, taken_at, trace_id, span_id
            FROM metric_points JOIN sel USING (series_id)
            WHERE taken_at >= ? AND taken_at < ? AND trace_id IS NOT NULL
            ORDER BY taken_at DESC
            LIMIT ?
         )
         SELECT picked.g, picked.taken_at, picked.trace_id, picked.span_id,
                any_value(spans.name), max(spans.duration_us)
         FROM picked
         LEFT JOIN spans ON spans.trace_id = picked.trace_id AND spans.span_id = picked.span_id
         GROUP BY ALL
         ORDER BY picked.taken_at DESC",
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
            span_name: row.get(4)?,
            duration_us: row.get(5)?,
        })
    })?;

    rows.collect()
}

#[cfg(test)]
mod tests {
    use daroyan_migration::duckdb::run_migrations;

    use super::*;
    use crate::analytics::metrics::ingest::{SeriesRow, Temporality};
    use crate::analytics::metrics::series::PointRow;
    use crate::analytics::metrics::store::{append_metrics, roll_up_metrics};

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
        analytics.run(run_migrations).unwrap();

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
            0,
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
            0,
        )
        .await
        .unwrap();
        let quiet: Vec<f64> = sent.series[1].values.iter().flatten().copied().collect();
        assert_eq!(quiet, [60.0, 60.0, 4.0]);

        let depth =
            query_metric(&analytics, query("queue.depth", Aggregation::Max, None), 0)
                .await
                .unwrap();
        let peaks: Vec<f64> = depth.series[0].values.iter().flatten().copied().collect();
        assert_eq!(peaks, [59.0, 119.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn reads_histogram_extremes_and_exemplar_spans() {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-metric-exemplars-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics.run(run_migrations).unwrap();

        let start = 10 * DAY_MS;
        let point = |minute: i64, min: f64, max: f64, trace_id: &str| PointRow {
            series_id: 1,
            at: start + minute * MINUTE_MS,
            count: Some(4),
            count_delta: Some(4),
            sum: Some(1.0),
            sum_delta: Some(1.0),
            min: Some(min),
            max: Some(max),
            trace_id: Some(trace_id.into()),
            span_id: Some("s1".into()),
            ..PointRow::default()
        };
        let rows =
            [series(1, "http.server.request.duration", MetricKind::Histogram, "/a")];
        let points = [point(1, 0.02, 0.4, "t1"), point(2, 0.01, 1.9, "t2")];
        analytics
            .run(|conn| {
                append_metrics(conn, &rows, &points)?;
                conn.execute(
                    "INSERT INTO spans (trace_id, span_id, service, name, kind, started_at,
                                        duration_us, status, received_at)
                     VALUES ('t2', 's1', 'api', 'GET /a', 'server', 0, 1900000, 'ok', 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let query = |aggregation| MetricQuery {
            name: "http.server.request.duration".into(),
            aggregation: Some(aggregation),
            since: start,
            until: start + HOUR_MS,
            step_ms: Some(HOUR_MS),
            ..MetricQuery::default()
        };

        let whole = MetricQuery {
            since: start + 30 * 1_000,
            until: start + 3 * MINUTE_MS,
            step_ms: Some(3 * MINUTE_MS),
            ..query(Aggregation::CountRate)
        };
        let whole = query_metric(&analytics, whole, 0).await.unwrap();
        assert_eq!(whole.times, [start + 30 * 1_000]);
        assert_eq!(whole.series[0].values, [Some(8.0 / 90.0)]);

        let listed = list_metrics(&analytics, start + MINUTE_MS).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].last_seen_at, start + 2 * MINUTE_MS);

        let low = query_metric(&analytics, query(Aggregation::Min), 0).await.unwrap();
        let high = query_metric(&analytics, query(Aggregation::Max), 0).await.unwrap();
        assert_eq!(low.series[0].values, [Some(0.01)]);
        assert_eq!(high.series[0].values, [Some(1.9)]);

        let exemplars = &high.exemplars;
        assert_eq!(exemplars.len(), 2);
        assert_eq!(exemplars[0].span_name.as_deref(), Some("GET /a"));
        assert_eq!(exemplars[0].duration_us, Some(1_900_000));
        assert_eq!(exemplars[1].span_name, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_filters_with_commas_and_equals_in_the_value() {
        let read = |text: &str| text.parse::<MetricFilter>().ok();
        let filter = |key: &str, value: &str, is_negated| MetricFilter {
            key: key.into(),
            value: value.into(),
            is_negated,
        };

        assert_eq!(read("route=/a,/b"), Some(filter("route", "/a,/b", false)));
        assert_eq!(read(" code != 500"), Some(filter("code", "500", true)));
        assert_eq!(read("url=/a?b=1"), Some(filter("url", "/a?b=1", false)));
        assert_eq!(read("url!=/a?b=1"), Some(filter("url", "/a?b=1", true)));
        assert_eq!(read("a=b!=c"), Some(filter("a=b", "c", true)));
        assert_eq!(read("broken"), None);
        assert_eq!(read("=x"), None);
        assert_eq!(read(" !=x"), None);
    }

    #[test]
    fn picks_a_step_for_about_120_buckets() {
        assert_eq!(pick_step(HOUR_MS, None, false), MINUTE_MS);
        assert_eq!(pick_step(24 * HOUR_MS, None, false), 15 * MINUTE_MS);
        assert_eq!(pick_step(7 * 24 * HOUR_MS, None, true), 3 * HOUR_MS);
        assert_eq!(pick_step(HOUR_MS, Some(1_000), false), MINUTE_MS);
        assert_eq!(pick_step(30 * 24 * HOUR_MS, Some(HOUR_MS), true), HOUR_MS);
        assert_eq!(pick_step(2 * 24 * HOUR_MS, Some(90 * MINUTE_MS), true), 2 * HOUR_MS);
        assert_eq!(
            pick_step(2 * 24 * HOUR_MS, Some(90 * MINUTE_MS), false),
            90 * MINUTE_MS
        );
    }

    #[test]
    fn only_allows_aggregations_that_fit_the_kind() {
        assert!(Aggregation::Rate.fits(MetricKind::Sum, true));
        assert!(!Aggregation::Rate.fits(MetricKind::Gauge, false));
        assert!(!Aggregation::P95.fits(MetricKind::Sum, false));
        assert!(Aggregation::Avg.fits(MetricKind::Histogram, false));
        assert!(Aggregation::Max.fits(MetricKind::Histogram, false));
        assert_eq!(
            Aggregation::default_for(MetricKind::Histogram, false),
            Aggregation::P95
        );
    }

    fn open(name: &str) -> (std::path::PathBuf, Analytics) {
        let dir = std::env::temp_dir()
            .join(format!("daroyan-metric-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let analytics = Analytics::open(&dir.join("live.duckdb")).unwrap();
        analytics.run(run_migrations).unwrap();

        (dir, analytics)
    }

    fn gauge_point(series_id: i64, at: i64, value: f64) -> PointRow {
        PointRow {
            series_id,
            at,
            value: Some(value),
            ..PointRow::default()
        }
    }

    fn delta_point(series_id: i64, at: i64, delta: f64) -> PointRow {
        PointRow {
            series_id,
            at,
            value: Some(delta),
            delta: Some(delta),
            ..PointRow::default()
        }
    }

    fn values(result: &MetricResult, line: usize) -> Vec<f64> {
        result.series[line].values.iter().flatten().copied().collect()
    }

    #[tokio::test]
    async fn caps_buckets_for_huge_ranges() {
        let (dir, analytics) = open("huge");
        let rows = [series(1, "emails.sent", MetricKind::Sum, "/a")];
        let points = [delta_point(1, 10 * DAY_MS, 1.0)];
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        for step_ms in [None, Some(1)] {
            let query = MetricQuery {
                name: "emails.sent".into(),
                since: 0,
                until: i64::MAX,
                step_ms,
                ..MetricQuery::default()
            };
            let result = query_metric(&analytics, query, 0).await.unwrap();
            assert!(result.times.len() as i64 <= MAX_BUCKETS, "{}", result.times.len());
        }

        for range in [HOUR_MS, 400 * DAY_MS, i64::MAX] {
            let step = pick_step(range, None, true);
            assert!(range / step < MAX_BUCKETS, "{range}");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn does_not_stretch_the_last_partial_bucket() {
        let (dir, analytics) = open("edge");
        let start = 10 * DAY_MS;
        let rows = [series(1, "emails.sent", MetricKind::Sum, "/a")];
        let points: Vec<PointRow> = (1..=5)
            .map(|minute| delta_point(1, start + minute * MINUTE_MS, 60.0))
            .collect();
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "emails.sent".into(),
            aggregation: Some(Aggregation::Rate),
            since: start,
            until: start + 5 * MINUTE_MS + 1_000,
            step_ms: Some(MINUTE_MS),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query, 0).await.unwrap();
        assert_eq!(values(&result, 0), [1.0; 5]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn ranks_the_lowest_groups_first_for_below_rules() {
        let (dir, analytics) = open("ranking");
        let until = 10 * DAY_MS;
        let rows = [
            series(1, "pool.free", MetricKind::Gauge, "/a"),
            series(2, "pool.free", MetricKind::Gauge, "/b"),
            series(3, "pool.free", MetricKind::Gauge, "/c"),
        ];
        let points = [
            gauge_point(1, until - MINUTE_MS, 1.0),
            gauge_point(2, until - MINUTE_MS, 2.0),
            gauge_point(3, until - MINUTE_MS, 3.0),
        ];
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "pool.free".into(),
            group_by: GroupBy::read("route"),
            since: until - 5 * MINUTE_MS,
            until,
            ..MetricQuery::default()
        };
        let lowest = Ranking { max_groups: 1, is_lowest_first: true };
        let measure = measure_metric(&analytics, query.clone(), lowest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            measure.readings,
            [MetricReading { group: Some("/a".into()), value: 1.0 }]
        );

        let highest = Ranking { max_groups: 1, is_lowest_first: false };
        let measure = measure_metric(&analytics, query, highest).await.unwrap().unwrap();
        assert_eq!(measure.readings[0].group.as_deref(), Some("/c"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn leaves_stale_series_out_of_the_last_value() {
        let (dir, analytics) = open("stale");
        let until = 10 * DAY_MS;
        let mut restarted = series(2, "queue.depth", MetricKind::Gauge, "/a");
        restarted.instance = Some("worker-2".into());
        let rows = [series(1, "queue.depth", MetricKind::Gauge, "/a"), restarted];
        let points = [
            gauge_point(1, until - 30 * MINUTE_MS, 100.0),
            gauge_point(2, until - MINUTE_MS, 5.0),
        ];
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "queue.depth".into(),
            aggregation: Some(Aggregation::Last),
            since: until - HOUR_MS,
            until,
            ..MetricQuery::default()
        };
        let ranking = Ranking { max_groups: 10, is_lowest_first: false };
        let measure = measure_metric(&analytics, query.clone(), ranking)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(measure.readings[0].value, 5.0);

        let chart = MetricQuery { step_ms: Some(HOUR_MS), ..query };
        let result = query_metric(&analytics, chart, 0).await.unwrap();
        assert_eq!(values(&result, 0), [5.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn reads_late_points_and_weights_rollup_averages() {
        let (dir, analytics) = open("late");
        let start = 10 * DAY_MS;
        let rows = [
            series(1, "queue.depth", MetricKind::Gauge, "/a"),
            series(2, "jobs.done", MetricKind::Sum, "/a"),
        ];
        let mut points: Vec<PointRow> = (0..60)
            .map(|minute| gauge_point(1, start + minute * MINUTE_MS, 1.0))
            .collect();
        points.extend([
            gauge_point(1, start + HOUR_MS, 100.0),
            gauge_point(1, start + 6 * HOUR_MS, 1.0),
            delta_point(2, start + 30 * MINUTE_MS, 2.0),
            delta_point(2, start + 5 * HOUR_MS, 1.0),
        ]);
        analytics
            .run(|conn| {
                append_metrics(conn, &rows, &points)?;
                roll_up_metrics(conn, start + 2 * HOUR_MS)?;
                roll_up_metrics(conn, start + 7 * HOUR_MS)
            })
            .unwrap();

        let late = [delta_point(2, start + 5 * HOUR_MS + 30 * MINUTE_MS, 4.0)];
        analytics.run(|conn| append_metrics(conn, &[], &late)).unwrap();

        let query = |name: &str, aggregation, step_ms| MetricQuery {
            name: name.into(),
            aggregation: Some(aggregation),
            since: start - DAY_MS,
            until: start + DAY_MS,
            step_ms: Some(step_ms),
            ..MetricQuery::default()
        };

        let done = query_metric(
            &analytics,
            query("jobs.done", Aggregation::Increase, HOUR_MS),
            0,
        )
        .await
        .unwrap();
        assert_eq!(values(&done, 0), [2.0, 5.0]);

        let depth = query_metric(
            &analytics,
            query("queue.depth", Aggregation::Avg, 2 * HOUR_MS),
            0,
        )
        .await
        .unwrap();
        let first = depth.series[0].values
            [depth.times.iter().position(|at| *at == start).unwrap()];
        assert_eq!(first, Some(160.0 / 61.0));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn starts_backtest_windows_on_a_boundary() {
        let (dir, analytics) = open("windows");
        let start = 10 * DAY_MS;
        let rows = [series(1, "queue.depth", MetricKind::Gauge, "/a")];
        let points = [
            gauge_point(1, start + 2 * MINUTE_MS, 9.0),
            gauge_point(1, start + 6 * MINUTE_MS, 1.0),
        ];
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "queue.depth".into(),
            aggregation: Some(Aggregation::Max),
            since: start + 90_000,
            until: start + 10 * MINUTE_MS,
            ..MetricQuery::default()
        };
        let ranking = Ranking { max_groups: 10, is_lowest_first: false };
        let measured = measure_metric_windows(&analytics, query, 5 * MINUTE_MS, ranking)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            measured.windows,
            [MetricWindow { at: start + 5 * MINUTE_MS, group: None, value: 1.0 }]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn reads_attribute_keys_that_look_like_json_paths() {
        let (dir, analytics) = open("keys");
        let until = 10 * DAY_MS;
        let mut row = series(1, "queue.depth", MetricKind::Gauge, "/a");
        row.attributes = r#"{"$env":"prod","a/b":"x","t~":"y"}"#.into();
        let points = [gauge_point(1, until - MINUTE_MS, 7.0)];
        analytics.run(|conn| append_metrics(conn, &[row], &points)).unwrap();

        let query = MetricQuery {
            name: "queue.depth".into(),
            filters: vec!["$env=prod".parse().unwrap(), "t~=y".parse().unwrap()],
            group_by: GroupBy::read("a/b"),
            since: until - HOUR_MS,
            until,
            step_ms: Some(HOUR_MS),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query, 0).await.unwrap();
        assert_eq!(result.series[0].group.as_deref(), Some("x"));
        assert_eq!(values(&result, 0), [7.0]);

        let attributes = list_metric_attributes(&analytics, "queue.depth".into())
            .await
            .unwrap();
        let keys: Vec<(&str, &str)> = attributes
            .iter()
            .map(|attribute| (attribute.key.as_str(), attribute.values[0].value.as_str()))
            .collect();
        assert_eq!(keys, [("$env", "prod"), ("a/b", "x"), ("t~", "y")]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn formats_values_like_the_web_app() {
        assert_eq!(format_metric_value(0.5, "1", Aggregation::Last), "0.5");
        assert_eq!(format_metric_value(42.0, "1", Aggregation::Last), "42");
        assert_eq!(format_metric_value(2_048.0, "By", Aggregation::Last), "2 KB");
        assert_eq!(format_metric_value(512.0, "By", Aggregation::Last), "512 B");
        assert_eq!(
            format_metric_value(3.0, "{request}", Aggregation::Rate),
            "3 requests/s"
        );
        assert_eq!(
            format_metric_value(1_200.0, "{job}", Aggregation::RatePerMinute),
            "1,200 jobs/min"
        );
        assert_eq!(format_metric_value(0.25, "s", Aggregation::P95), "250 ms");
        assert_eq!(format_metric_value(250.0, "us", Aggregation::Avg), "250 µs");
        assert_eq!(format_metric_value(12.5, "%", Aggregation::Last), "12.5%");
        assert_eq!(format_metric_value(25_000.0, "", Aggregation::Last), "25K");
    }

    #[test]
    fn group_by_round_trips_as_text() {
        let parsed: Option<GroupBy> = serde_json::from_str(r#""server_id""#).unwrap();
        assert_eq!(parsed, Some(GroupBy::Server));
        assert_eq!(serde_json::to_string(&GroupBy::from("route")).unwrap(), r#""route""#);
        assert_eq!(GroupBy::read("  "), None);
        assert_eq!(GroupBy::from("server".to_string()), GroupBy::Server);
        assert_eq!(
            GroupBy::from(" route ".to_string()),
            GroupBy::Attribute("route".into())
        );
    }

    fn sum_point(series_id: i64, at: i64, delta: f64) -> PointRow {
        PointRow {
            start_at: Some(at - MINUTE_MS),
            ..delta_point(series_id, at, delta)
        }
    }

    #[tokio::test]
    async fn reads_rollups_when_the_range_starts_before_point_retention() {
        let (dir, analytics) = open("retention");
        let start = 10 * DAY_MS;
        let rows = [series(1, "emails.sent", MetricKind::Sum, "/a")];
        let points: Vec<PointRow> = (0..120)
            .map(|minute| delta_point(1, start + minute * MINUTE_MS, 1.0))
            .collect();
        analytics
            .run(|conn| {
                append_metrics(conn, &rows, &points)?;
                roll_up_metrics(conn, start + 6 * HOUR_MS)?;
                conn.execute("DELETE FROM metric_points", [])?;
                Ok(())
            })
            .unwrap();

        let query = MetricQuery {
            name: "emails.sent".into(),
            aggregation: Some(Aggregation::Increase),
            since: start,
            until: start + DAY_MS,
            step_ms: Some(HOUR_MS),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query, start + 2 * DAY_MS).await.unwrap();
        assert_eq!(result.step_ms, HOUR_MS);
        assert_eq!(values(&result, 0), [60.0, 60.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn keeps_slow_gauges_in_the_last_value() {
        let (dir, analytics) = open("slow");
        let start = 10 * DAY_MS;
        let rows = [series(1, "pool.size", MetricKind::Gauge, "/a")];
        let points: Vec<PointRow> = (0..6)
            .map(|step| gauge_point(1, start + step * 10 * MINUTE_MS, 7.0))
            .collect();
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "pool.size".into(),
            aggregation: Some(Aggregation::Last),
            since: start,
            until: start + HOUR_MS,
            step_ms: Some(15 * MINUTE_MS),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query, 0).await.unwrap();
        assert_eq!(result.series[0].values, [Some(7.0); 4]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn divides_the_live_bucket_by_the_time_its_points_cover() {
        let (dir, analytics) = open("covered");
        let start = 10 * DAY_MS;
        let rows = [series(1, "emails.sent", MetricKind::Sum, "/a")];
        let points: Vec<PointRow> = (1..=20)
            .map(|minute| sum_point(1, start + minute * MINUTE_MS, 60.0))
            .collect();
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "emails.sent".into(),
            aggregation: Some(Aggregation::Rate),
            since: start,
            until: start + 20 * MINUTE_MS + 1_000,
            step_ms: Some(15 * MINUTE_MS),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query.clone(), 0).await.unwrap();
        assert_eq!(values(&result, 0), [1.0, 1.0]);

        let ranking = Ranking { max_groups: 10, is_lowest_first: false };
        let windows = measure_metric_windows(&analytics, query, 15 * MINUTE_MS, ranking)
            .await
            .unwrap()
            .unwrap();
        let rates: Vec<f64> = windows.windows.iter().map(|window| window.value).collect();
        assert_eq!(rates, [1.0, 1.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn reads_partial_edge_hours_from_points_in_one_bucket() {
        let (dir, analytics) = open("edges");
        let start = 10 * DAY_MS;
        let rows = [series(1, "emails.sent", MetricKind::Sum, "/a")];
        let points: Vec<PointRow> = (0..180)
            .map(|step| delta_point(1, start + step * 10 * MINUTE_MS, 1.0))
            .collect();
        analytics
            .run(|conn| {
                append_metrics(conn, &rows, &points)?;
                roll_up_metrics(conn, start + 30 * HOUR_MS)
            })
            .unwrap();

        let range = 25 * HOUR_MS + 10 * MINUTE_MS;
        let query = MetricQuery {
            name: "emails.sent".into(),
            aggregation: Some(Aggregation::Increase),
            since: start + 20 * MINUTE_MS,
            until: start + 20 * MINUTE_MS + range,
            step_ms: Some(range),
            ..MetricQuery::default()
        };
        let result = query_metric(&analytics, query, 0).await.unwrap();
        assert_eq!(values(&result, 0), [151.0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn caps_backtest_groups_and_windows() {
        let (dir, analytics) = open("caps");
        let until = 40 * DAY_MS;
        let rows = [
            series(1, "pool.free", MetricKind::Gauge, "/a"),
            series(2, "pool.free", MetricKind::Gauge, "/b"),
            series(3, "pool.free", MetricKind::Gauge, "/c"),
        ];
        let points = [
            gauge_point(1, until - 8 * DAY_MS, 100.0),
            gauge_point(1, until - MINUTE_MS, 1.0),
            gauge_point(2, until - MINUTE_MS, 2.0),
            gauge_point(3, until - MINUTE_MS, 3.0),
        ];
        analytics.run(|conn| append_metrics(conn, &rows, &points)).unwrap();

        let query = MetricQuery {
            name: "pool.free".into(),
            aggregation: Some(Aggregation::Max),
            group_by: GroupBy::read("route"),
            since: until - 30 * DAY_MS,
            until,
            ..MetricQuery::default()
        };
        let lowest = Ranking { max_groups: 2, is_lowest_first: true };
        let measured = measure_metric_windows(&analytics, query, MINUTE_MS, lowest)
            .await
            .unwrap()
            .unwrap();
        let groups: Vec<(Option<&str>, f64)> = measured
            .windows
            .iter()
            .map(|window| (window.group.as_deref(), window.value))
            .collect();
        assert_eq!(groups, [(Some("/a"), 1.0), (Some("/b"), 2.0)]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
