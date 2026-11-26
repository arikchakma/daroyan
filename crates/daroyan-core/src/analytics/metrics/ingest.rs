use std::str::FromStr;
use std::sync::Arc;

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value as NumberValue;
use opentelemetry_proto::tonic::metrics::v1::{
    AggregationTemporality, Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint,
    Metric, NumberDataPoint,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::analytics::metrics::histogram::{
    exponential_bounds, sum_counts, to_fixed_buckets,
};
use crate::analytics::spans::{UNKNOWN_SERVICE, find_text, to_hex, truncate};
use crate::servers::hosts::HostIndex;

const MAX_NAME_CHARS: usize = 255;
const MAX_DESCRIPTION_CHARS: usize = 1024;
const MAX_VALUE_CHARS: usize = 256;
const SCALAR_CHARS: usize = 8;
const MAX_BUCKETS: usize = 160;
const MAX_FUTURE_MS: i64 = 5 * 60 * 1000;
const NO_RECORDED_VALUE: u32 = 1;

const POINT_ATTRIBUTES: AttributeLimits =
    AttributeLimits { count: 32, total_chars: 4 * 1024 };
const RESOURCE_ATTRIBUTES: AttributeLimits =
    AttributeLimits { count: 64, total_chars: 8 * 1024 };

#[derive(Clone, Copy)]
struct AttributeLimits {
    count: usize,
    total_chars: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Sum,
    Gauge,
    Histogram,
}

impl MetricKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        }
    }
}

impl FromStr for MetricKind {
    type Err = String;

    fn from_str(kind: &str) -> Result<Self, Self::Err> {
        match kind {
            "sum" => Ok(Self::Sum),
            "gauge" => Ok(Self::Gauge),
            "histogram" => Ok(Self::Histogram),
            _ => Err(format!("unknown metric kind: {kind}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Temporality {
    Delta,
    Cumulative,
}

impl Temporality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::Cumulative => "cumulative",
        }
    }
}

impl TryFrom<i32> for Temporality {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match AggregationTemporality::try_from(value) {
            Ok(AggregationTemporality::Delta) => Ok(Self::Delta),
            Ok(AggregationTemporality::Cumulative) => Ok(Self::Cumulative),
            Ok(AggregationTemporality::Unspecified) | Err(_) => Err(value),
        }
    }
}

#[derive(Debug)]
pub struct SeriesRow {
    pub series_id: i64,
    pub token_id: Arc<str>,
    pub name: Arc<str>,
    pub unit: Arc<str>,
    pub description: Arc<str>,
    pub kind: MetricKind,
    pub is_monotonic: bool,
    pub temporality: Option<Temporality>,
    pub service: Arc<str>,
    pub server_id: Option<Arc<str>>,
    pub instance: Option<Arc<str>>,
    pub attributes: String,
    pub resource: Option<Arc<str>>,
    pub bounds: Option<Arc<[f64]>>,
    pub first_seen: i64,
}

impl SeriesRow {
    pub fn is_counter(&self) -> bool {
        self.kind == MetricKind::Sum && self.is_monotonic
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistogramReading {
    pub count: u64,
    pub sum: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub buckets: Vec<u64>,
}

impl HistogramReading {
    fn is_valid(&self) -> bool {
        let is_finite = [self.sum, self.min, self.max]
            .into_iter()
            .flatten()
            .all(f64::is_finite);

        is_finite && sum_counts(&self.buckets) == Some(self.count)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Number(f64),
    Histogram(HistogramReading),
}

#[derive(Debug)]
pub struct MetricSample {
    pub series: SeriesRow,
    pub at: i64,
    pub start_at: Option<i64>,
    pub reading: Reading,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct ParsedMetrics {
    pub samples: Vec<MetricSample>,
    pub skipped: u64,
}

#[derive(Clone)]
struct Identity(Sha256);

impl Identity {
    fn new(token_id: &str) -> Self {
        let mut identity = Self(Sha256::new());
        identity.add(token_id);

        identity
    }

    fn with(&self, parts: &[&str]) -> Self {
        let mut identity = self.clone();
        for part in parts {
            identity.add(part);
        }

        identity
    }

    fn add(&mut self, part: &str) {
        self.0.update((part.len() as u64).to_be_bytes());
        self.0.update(part.as_bytes());
    }

    fn finish(mut self, bounds: Option<&[f64]>) -> i64 {
        for bound in bounds.unwrap_or_default() {
            self.0.update(bound.to_bits().to_be_bytes());
        }

        let digest = self.0.finalize();

        i64::from_be_bytes(digest[..8].try_into().unwrap_or_default())
    }
}

struct Origin {
    token_id: Arc<str>,
    service: Arc<str>,
    server_id: Option<Arc<str>>,
    instance: Option<Arc<str>>,
    resource: Option<Arc<str>>,
    received_at: i64,
}

struct Descriptor {
    name: Arc<str>,
    unit: Arc<str>,
    description: Arc<str>,
    kind: MetricKind,
    is_monotonic: bool,
    temporality: Option<Temporality>,
    identity: Identity,
}

struct PointParts<'a> {
    at_nanos: u64,
    start_nanos: u64,
    attributes: &'a [KeyValue],
    exemplars: &'a [Exemplar],
    bounds: Option<Arc<[f64]>>,
    reading: Reading,
}

pub fn count_metric_points(request: &ExportMetricsServiceRequest) -> u64 {
    request
        .resource_metrics
        .iter()
        .flat_map(|resource| &resource.scope_metrics)
        .flat_map(|scope| &scope.metrics)
        .map(|metric| count_points(metric.data.as_ref()))
        .sum()
}

pub fn to_metric_samples(
    request: &ExportMetricsServiceRequest,
    hosts: &HostIndex,
    token_id: &str,
    received_at: i64,
) -> ParsedMetrics {
    let token_id: Arc<str> = token_id.into();
    let mut parsed = ParsedMetrics::default();

    for resource_metrics in &request.resource_metrics {
        let attributes = resource_metrics
            .resource
            .as_ref()
            .map(|resource| resource.attributes.as_slice())
            .unwrap_or_default();
        let resource = to_capped_json(attributes, RESOURCE_ATTRIBUTES);
        let resource_identity = Identity::new(&token_id).with(&[&resource]);
        let origin = Origin {
            token_id: token_id.clone(),
            service: find_text(attributes, &["service.name"])
                .unwrap_or_else(|| UNKNOWN_SERVICE.to_string())
                .into(),
            server_id: find_text(attributes, &["host.name"])
                .and_then(|host_name| hosts.find(&host_name).map(Into::into)),
            instance: find_text(attributes, &["service.instance.id"]).map(Into::into),
            resource: Some(resource.into()),
            received_at,
        };

        for scope_metrics in &resource_metrics.scope_metrics {
            let scope_name = scope_metrics
                .scope
                .as_ref()
                .map(|scope| scope.name.as_str())
                .unwrap_or_default();
            let scope_identity = resource_identity.with(&[scope_name]);

            for metric in &scope_metrics.metrics {
                push_metric(&mut parsed, &origin, &scope_identity, metric);
            }
        }
    }

    parsed
}

fn push_metric(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    scope_identity: &Identity,
    metric: &Metric,
) {
    let name: Arc<str> = truncate(&metric.name, MAX_NAME_CHARS).into();
    if name.is_empty() {
        parsed.skipped += count_points(metric.data.as_ref());
        return;
    }

    let describe =
        |kind: MetricKind, is_monotonic, temporality: Option<Temporality>| Descriptor {
            name: name.clone(),
            unit: truncate(&metric.unit, MAX_NAME_CHARS).into(),
            description: truncate(&metric.description, MAX_DESCRIPTION_CHARS).into(),
            kind,
            is_monotonic,
            temporality,
            identity: scope_identity.with(&[
                &name,
                kind.as_str(),
                if is_monotonic { "monotonic" } else { "" },
                temporality.map(Temporality::as_str).unwrap_or_default(),
            ]),
        };

    match &metric.data {
        Some(Data::Gauge(gauge)) => {
            let descriptor = describe(MetricKind::Gauge, false, None);
            for point in &gauge.data_points {
                push_number(parsed, origin, &descriptor, point);
            }
        }
        Some(Data::Sum(sum)) => {
            let Ok(temporality) = Temporality::try_from(sum.aggregation_temporality)
            else {
                parsed.skipped += count_points(metric.data.as_ref());
                return;
            };

            let descriptor =
                describe(MetricKind::Sum, sum.is_monotonic, Some(temporality));
            for point in &sum.data_points {
                push_number(parsed, origin, &descriptor, point);
            }
        }
        Some(Data::Histogram(histogram)) => {
            let Ok(temporality) =
                Temporality::try_from(histogram.aggregation_temporality)
            else {
                parsed.skipped += count_points(metric.data.as_ref());
                return;
            };

            let descriptor = describe(MetricKind::Histogram, false, Some(temporality));
            let mut bounds = None;
            for point in &histogram.data_points {
                push_histogram(parsed, origin, &descriptor, &mut bounds, point);
            }
        }
        Some(Data::ExponentialHistogram(histogram)) => {
            let Ok(temporality) =
                Temporality::try_from(histogram.aggregation_temporality)
            else {
                parsed.skipped += count_points(metric.data.as_ref());
                return;
            };

            let descriptor = describe(MetricKind::Histogram, false, Some(temporality));
            let bounds = exponential_bounds();
            for point in &histogram.data_points {
                push_exponential(parsed, origin, &descriptor, &bounds, point);
            }
        }
        data => parsed.skipped += count_points(data.as_ref()),
    }
}

fn count_points(data: Option<&Data>) -> u64 {
    let count = match data {
        Some(Data::Gauge(gauge)) => gauge.data_points.len(),
        Some(Data::Sum(sum)) => sum.data_points.len(),
        Some(Data::Histogram(histogram)) => histogram.data_points.len(),
        Some(Data::ExponentialHistogram(histogram)) => histogram.data_points.len(),
        Some(Data::Summary(summary)) => summary.data_points.len(),
        None => 0,
    };

    count as u64
}

fn push_number(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    point: &NumberDataPoint,
) {
    if point.flags & NO_RECORDED_VALUE != 0 {
        return;
    }

    let value = match point.value {
        Some(NumberValue::AsDouble(value)) if value.is_finite() => value,
        Some(NumberValue::AsInt(value)) => value as f64,
        _ => {
            parsed.skipped += 1;
            return;
        }
    };

    push_sample(
        parsed,
        origin,
        descriptor,
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
            bounds: None,
            reading: Reading::Number(value),
        },
    );
}

fn push_histogram(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    shared_bounds: &mut Option<Arc<[f64]>>,
    point: &HistogramDataPoint,
) {
    if point.flags & NO_RECORDED_VALUE != 0 {
        return;
    }

    let (bounds, buckets) = if point.bucket_counts.is_empty() {
        (&[][..], vec![point.count])
    } else {
        (point.explicit_bounds.as_slice(), point.bucket_counts.clone())
    };

    let is_sorted = bounds.windows(2).all(|pair| pair[0] < pair[1]);
    let is_finite = bounds.iter().all(|bound| bound.is_finite());
    if buckets.len() != bounds.len() + 1
        || buckets.len() > MAX_BUCKETS
        || !is_sorted
        || !is_finite
    {
        parsed.skipped += 1;
        return;
    }

    let reading = HistogramReading {
        count: point.count,
        sum: point.sum,
        min: point.min,
        max: point.max,
        buckets,
    };
    if !reading.is_valid() {
        parsed.skipped += 1;
        return;
    }

    push_sample(
        parsed,
        origin,
        descriptor,
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
            bounds: Some(share_bounds(shared_bounds, bounds)),
            reading: Reading::Histogram(reading),
        },
    );
}

fn share_bounds(shared: &mut Option<Arc<[f64]>>, bounds: &[f64]) -> Arc<[f64]> {
    if let Some(shared) = shared.as_ref().filter(|shared| ***shared == *bounds) {
        return shared.clone();
    }

    let bounds: Arc<[f64]> = bounds.into();
    *shared = Some(bounds.clone());

    bounds
}

fn push_exponential(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    bounds: &Arc<[f64]>,
    point: &ExponentialHistogramDataPoint,
) {
    if point.flags & NO_RECORDED_VALUE != 0 {
        return;
    }

    let reading = to_fixed_buckets(point).map(|buckets| HistogramReading {
        count: point.count,
        sum: point.sum,
        min: point.min,
        max: point.max,
        buckets,
    });
    let Some(reading) = reading.filter(HistogramReading::is_valid) else {
        parsed.skipped += 1;
        return;
    };

    push_sample(
        parsed,
        origin,
        descriptor,
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
            bounds: Some(bounds.clone()),
            reading: Reading::Histogram(reading),
        },
    );
}

fn push_sample(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    parts: PointParts,
) {
    let at = to_millis(parts.at_nanos);
    if parts.at_nanos == 0 || at > origin.received_at.saturating_add(MAX_FUTURE_MS) {
        parsed.skipped += 1;
        return;
    }

    let start_at = (parts.start_nanos > 0).then(|| to_millis(parts.start_nanos));
    let attributes = to_capped_json(parts.attributes, POINT_ATTRIBUTES);
    let series_id = descriptor
        .identity
        .with(&[&attributes])
        .finish(parts.bounds.as_deref());
    let (trace_id, span_id) = parts
        .exemplars
        .iter()
        .rev()
        .find_map(|exemplar| {
            Some((to_hex(&exemplar.trace_id)?, to_hex(&exemplar.span_id)?))
        })
        .unzip();

    parsed.samples.push(MetricSample {
        series: SeriesRow {
            series_id,
            token_id: origin.token_id.clone(),
            name: descriptor.name.clone(),
            unit: descriptor.unit.clone(),
            description: descriptor.description.clone(),
            kind: descriptor.kind,
            is_monotonic: descriptor.is_monotonic,
            temporality: descriptor.temporality,
            service: origin.service.clone(),
            server_id: origin.server_id.clone(),
            instance: origin.instance.clone(),
            attributes,
            resource: origin.resource.clone(),
            bounds: parts.bounds,
            first_seen: at,
        },
        at,
        start_at,
        reading: parts.reading,
        trace_id,
        span_id,
    });
}

fn to_millis(nanos: u64) -> i64 {
    i64::try_from(nanos / 1_000_000).unwrap_or(i64::MAX)
}

fn to_capped_json(attributes: &[KeyValue], limits: AttributeLimits) -> String {
    let mut sorted: Vec<&KeyValue> = attributes.iter().collect();
    sorted.sort_by(|left, right| left.key.cmp(&right.key));

    let mut chosen: Vec<&KeyValue> = Vec::with_capacity(limits.count);
    for attribute in sorted {
        let is_repeat = chosen.last().is_some_and(|last| last.key == attribute.key);
        if is_repeat {
            chosen.pop();
        } else if chosen.len() == limits.count {
            break;
        }

        chosen.push(attribute);
    }

    let mut left = limits.total_chars;
    let mut values = Map::new();
    for attribute in chosen {
        if left == 0 {
            break;
        }

        let key = take_text(&attribute.key, MAX_NAME_CHARS.min(left), &mut left);
        let mut value_left = MAX_VALUE_CHARS.min(left);
        let allowed = value_left;
        let value = attribute
            .value
            .as_ref()
            .map_or(Value::Null, |value| to_capped_value(value, &mut value_left));
        left -= allowed - value_left;

        values.insert(key, value);
    }

    serde_json::to_string(&values).unwrap_or_else(|_| "{}".to_string())
}

fn to_capped_value(value: &AnyValue, left: &mut usize) -> Value {
    match &value.value {
        Some(AnyValueKind::StringValue(text)) => {
            Value::String(take_text(text, *left, left))
        }
        Some(AnyValueKind::BoolValue(flag)) => {
            *left = left.saturating_sub(SCALAR_CHARS);
            Value::Bool(*flag)
        }
        Some(AnyValueKind::IntValue(number)) => {
            *left = left.saturating_sub(SCALAR_CHARS);
            Value::from(*number)
        }
        Some(AnyValueKind::DoubleValue(number)) => {
            *left = left.saturating_sub(SCALAR_CHARS);
            Value::from(*number)
        }
        Some(AnyValueKind::BytesValue(bytes)) => {
            let kept = &bytes[..bytes.len().min(*left / 2)];
            *left -= kept.len() * 2;
            Value::String(hex::encode(kept))
        }
        Some(AnyValueKind::ArrayValue(array)) => {
            let mut items = Vec::new();
            for item in &array.values {
                if *left == 0 {
                    break;
                }
                items.push(to_capped_value(item, left));
            }

            Value::Array(items)
        }
        Some(AnyValueKind::KvlistValue(list)) => {
            let mut entries = Map::new();
            for entry in &list.values {
                if *left == 0 {
                    break;
                }

                let key = take_text(&entry.key, *left, left);
                let value = entry
                    .value
                    .as_ref()
                    .map_or(Value::Null, |value| to_capped_value(value, left));
                entries.insert(key, value);
            }

            Value::Object(entries)
        }
        Some(AnyValueKind::StringValueStrindex(_)) | None => Value::Null,
    }
}

fn take_text(text: &str, max: usize, left: &mut usize) -> String {
    let kept: String = text.chars().take(max).collect();
    *left = left.saturating_sub(kept.chars().count());

    kept
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::host::Host;
    use opentelemetry_proto::tonic::common::v1::AnyValue;
    use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
    use opentelemetry_proto::tonic::common::v1::{
        ArrayValue, InstrumentationScope, KeyValueList,
    };
    use opentelemetry_proto::tonic::metrics::v1::{
        Gauge, Histogram, Metric, ResourceMetrics, ScopeMetrics, Sum, Summary,
        SummaryDataPoint,
    };
    use opentelemetry_proto::tonic::resource::v1::Resource;

    use super::*;

    fn text(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(AnyValueKind::StringValue(value.to_string())),
            }),
            ..Default::default()
        }
    }

    fn number_point(route: &str, value: f64) -> NumberDataPoint {
        NumberDataPoint {
            attributes: vec![text("route", route)],
            start_time_unix_nano: 1_000_000_000,
            time_unix_nano: 61_000_000_000,
            value: Some(NumberValue::AsDouble(value)),
            ..Default::default()
        }
    }

    fn request(metrics: Vec<Metric>) -> ExportMetricsServiceRequest {
        request_from(
            vec![
                text("service.name", "inboxjot-api"),
                text("service.instance.id", "worker-1"),
            ],
            metrics,
        )
    }

    fn request_from(
        attributes: Vec<KeyValue>,
        metrics: Vec<Metric>,
    ) -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource { attributes, ..Default::default() }),
                scope_metrics: vec![ScopeMetrics { metrics, ..Default::default() }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn reads_each_kind_and_skips_summaries() {
        let request = request(vec![
            Metric {
                name: "emails.sent".to_string(),
                unit: "{email}".to_string(),
                data: Some(Data::Sum(Sum {
                    data_points: vec![number_point("/a", 3.0), number_point("/b", 4.0)],
                    aggregation_temporality: AggregationTemporality::Cumulative as i32,
                    is_monotonic: true,
                })),
                ..Default::default()
            },
            Metric {
                name: "queue.depth".to_string(),
                data: Some(Data::Gauge(Gauge {
                    data_points: vec![number_point("/a", 340.0)],
                })),
                ..Default::default()
            },
            Metric {
                name: "job.duration".to_string(),
                data: Some(Data::Histogram(Histogram {
                    data_points: vec![HistogramDataPoint {
                        time_unix_nano: 61_000_000_000,
                        count: 3,
                        explicit_bounds: vec![1.0, 2.0],
                        bucket_counts: vec![1, 1, 1],
                        ..Default::default()
                    }],
                    aggregation_temporality: AggregationTemporality::Delta as i32,
                })),
                ..Default::default()
            },
            Metric {
                name: "legacy".to_string(),
                data: Some(Data::Summary(Summary {
                    data_points: vec![SummaryDataPoint::default()],
                })),
                ..Default::default()
            },
        ]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 61_000);

        assert_eq!(parsed.samples.len(), 4);
        assert_eq!(parsed.skipped, 1);

        let sent = &parsed.samples[0];
        assert!(sent.series.is_counter());
        assert_eq!(sent.at, 61_000);
        assert_eq!(sent.start_at, Some(1_000));
        assert_eq!(&*sent.series.service, "inboxjot-api");
        assert_eq!(sent.series.instance.as_deref(), Some("worker-1"));
        assert_eq!(sent.series.attributes, r#"{"route":"/a"}"#);
        assert_ne!(sent.series.series_id, parsed.samples[1].series.series_id);

        assert_eq!(parsed.samples[2].series.kind, MetricKind::Gauge);
        assert_eq!(parsed.samples[3].series.bounds.as_deref(), Some(&[1.0, 2.0][..]));
    }

    #[test]
    fn keeps_the_same_id_for_the_same_series() {
        let metric = || Metric {
            name: "queue.depth".to_string(),
            data: Some(Data::Gauge(Gauge { data_points: vec![number_point("/a", 1.0)] })),
            ..Default::default()
        };
        let first = to_metric_samples(
            &request(vec![metric()]),
            &HostIndex::default(),
            "ing_1",
            0,
        );
        let second = to_metric_samples(
            &request(vec![metric()]),
            &HostIndex::default(),
            "ing_1",
            0,
        );

        assert_eq!(first.samples[0].series.series_id, second.samples[0].series.series_id);
    }

    #[test]
    fn rejects_histograms_with_mismatched_buckets() {
        let request = request(vec![Metric {
            name: "job.duration".to_string(),
            data: Some(Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    time_unix_nano: 1,
                    explicit_bounds: vec![1.0],
                    bucket_counts: vec![1, 1, 1],
                    ..Default::default()
                }],
                aggregation_temporality: AggregationTemporality::Delta as i32,
            })),
            ..Default::default()
        }]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        assert!(parsed.samples.is_empty());
        assert_eq!(parsed.skipped, 1);
    }

    fn histogram(points: Vec<HistogramDataPoint>) -> Metric {
        Metric {
            name: "job.duration".to_string(),
            data: Some(Data::Histogram(Histogram {
                data_points: points,
                aggregation_temporality: AggregationTemporality::Delta as i32,
            })),
            ..Default::default()
        }
    }

    #[test]
    fn rejects_histograms_with_bad_numbers_or_counts() {
        let point = |count, sum: f64, bounds: Vec<f64>| HistogramDataPoint {
            time_unix_nano: 1,
            count,
            sum: Some(sum),
            explicit_bounds: bounds,
            bucket_counts: vec![1, 1, 1],
            ..Default::default()
        };
        let request = request(vec![histogram(vec![
            point(3, 1.5, vec![1.0, 2.0]),
            point(3, f64::NAN, vec![1.0, 2.0]),
            point(3, 1.5, vec![1.0, f64::INFINITY]),
            point(4, 1.5, vec![1.0, 2.0]),
            HistogramDataPoint {
                max: Some(f64::NEG_INFINITY),
                ..point(3, 1.5, vec![1.0, 2.0])
            },
        ])]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        assert_eq!(parsed.samples.len(), 1);
        assert_eq!(parsed.skipped, 4);
    }

    #[test]
    fn rejects_exponential_histograms_with_bad_scales() {
        use opentelemetry_proto::tonic::metrics::v1::ExponentialHistogram;
        use opentelemetry_proto::tonic::metrics::v1::exponential_histogram_data_point::Buckets;

        let point = |scale, count| ExponentialHistogramDataPoint {
            time_unix_nano: 1,
            scale,
            count,
            positive: Some(Buckets { offset: 0, bucket_counts: vec![2] }),
            ..Default::default()
        };
        let request = request(vec![Metric {
            name: "job.duration".to_string(),
            data: Some(Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![point(0, 2), point(i32::MIN, 2), point(0, 5)],
                aggregation_temporality: AggregationTemporality::Delta as i32,
            })),
            ..Default::default()
        }]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        assert_eq!(parsed.samples.len(), 1);
        assert_eq!(parsed.skipped, 2);
    }

    #[test]
    fn tells_hosts_apart_when_there_is_no_instance_id() {
        let metric = || Metric {
            name: "queue.depth".to_string(),
            data: Some(Data::Gauge(Gauge { data_points: vec![number_point("/a", 1.0)] })),
            ..Default::default()
        };
        let series_id = |host: &str| {
            let request = request_from(
                vec![text("service.name", "inboxjot-api"), text("host.name", host)],
                vec![metric()],
            );
            to_metric_samples(&request, &HostIndex::default(), "ing_1", 0).samples[0]
                .series
                .series_id
        };

        assert_ne!(series_id("web-1"), series_id("web-2"));
        assert_eq!(series_id("web-1"), series_id("web-1"));
    }

    fn gauge(points: Vec<NumberDataPoint>) -> Metric {
        Metric {
            name: "queue.depth".to_string(),
            data: Some(Data::Gauge(Gauge { data_points: points })),
            ..Default::default()
        }
    }

    fn scoped(scope: &str, attributes: Vec<KeyValue>) -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource { attributes, ..Default::default() }),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: scope.to_string(),
                        ..Default::default()
                    }),
                    metrics: vec![gauge(vec![number_point("/a", 1.0)])],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn tells_processes_and_scopes_apart() {
        let resource = |pid: &str| {
            vec![
                text("service.name", "inboxjot-api"),
                text("host.name", "web-1"),
                text("process.pid", pid),
            ]
        };
        let series_id = |request: ExportMetricsServiceRequest| {
            to_metric_samples(&request, &HostIndex::default(), "ing_1", 0).samples[0]
                .series
                .series_id
        };

        let first = series_id(scoped("billing", resource("101")));

        assert_eq!(first, series_id(scoped("billing", resource("101"))));
        assert_ne!(first, series_id(scoped("billing", resource("102"))));
        assert_ne!(first, series_id(scoped("mailer", resource("101"))));
    }

    #[test]
    fn keeps_the_series_when_the_host_lookup_changes() {
        let host = Host {
            name: "web-1".to_string(),
            labels: Vec::new(),
            hostname: "web-1".to_string(),
            platform: String::new(),
            platform_version: String::new(),
            kernel: String::new(),
            arch: String::new(),
            cpu: String::new(),
            cpu_cores: 1,
            mem_total: 0,
            swap_total: 0,
            private_ip: None,
            boot_time: 0,
            version: String::new(),
        };
        let hosts: HostIndex = [("srv_1".to_string(), host)].into_iter().collect();
        let request = request_from(
            vec![text("service.name", "inboxjot-api"), text("host.name", "web-1")],
            vec![gauge(vec![number_point("/a", 1.0)])],
        );

        let found = to_metric_samples(&request, &hosts, "ing_1", 0);
        let missing = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        assert_eq!(found.samples[0].series.server_id.as_deref(), Some("srv_1"));
        assert_eq!(missing.samples[0].series.server_id, None);
        assert_eq!(
            found.samples[0].series.series_id,
            missing.samples[0].series.series_id
        );
    }

    #[test]
    fn rejects_points_too_far_in_the_future() {
        let now = 1_000_000_000_i64;
        let point = |at_ms: i64| NumberDataPoint {
            time_unix_nano: u64::try_from(at_ms * 1_000_000).unwrap(),
            value: Some(NumberValue::AsDouble(1.0)),
            ..Default::default()
        };
        let request = request(vec![gauge(vec![
            point(now + 4 * 60 * 1000),
            point(now + 10 * 60 * 1000),
        ])]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", now);

        assert_eq!(parsed.samples.len(), 1);
        assert_eq!(parsed.skipped, 1);
        assert_eq!(parsed.samples[0].at, now + 4 * 60 * 1000);
    }

    #[test]
    fn caps_every_kind_of_attribute_value() {
        let any = |value| Some(AnyValue { value: Some(value) });
        let long = "x".repeat(1_000);
        let mut attributes = vec![
            KeyValue {
                key: "array".to_string(),
                value: any(AnyValueKind::ArrayValue(ArrayValue {
                    values: (0..50)
                        .map(|_| AnyValue {
                            value: Some(AnyValueKind::StringValue(long.clone())),
                        })
                        .collect(),
                })),
                ..Default::default()
            },
            KeyValue {
                key: "bytes".to_string(),
                value: any(AnyValueKind::BytesValue(vec![7; 1_000])),
                ..Default::default()
            },
            KeyValue {
                key: "map".to_string(),
                value: any(AnyValueKind::KvlistValue(KeyValueList {
                    values: vec![text("inner", &long)],
                })),
                ..Default::default()
            },
        ];
        attributes.extend((0..40).map(|index| text(&format!("z{index:02}"), &long)));
        let mut resource = attributes.clone();
        resource.push(text("service.name", "inboxjot-api"));
        let mut point = number_point("/a", 1.0);
        point.attributes = attributes;

        let parsed = to_metric_samples(
            &request_from(resource, vec![gauge(vec![point])]),
            &HostIndex::default(),
            "ing_1",
            61_000,
        );

        let series = &parsed.samples[0].series;
        let values: Map<String, Value> =
            serde_json::from_str(&series.attributes).unwrap();
        let size = |key: &str| values[key].to_string().len();

        assert!(series.attributes.len() < 4_600, "{}", series.attributes.len());
        assert!(series.resource.as_deref().unwrap().len() < 9_000);
        assert!(size("array") <= MAX_VALUE_CHARS + 4);
        assert!(size("bytes") <= MAX_VALUE_CHARS + 2);
        assert!(size("map") <= MAX_VALUE_CHARS + 12);
        assert!(values.contains_key("z00"));
        assert!(!values.contains_key("z39"));
    }

    #[test]
    fn keeps_histograms_that_send_only_count_and_sum() {
        let point = |count| HistogramDataPoint {
            time_unix_nano: 1,
            count,
            sum: Some(12.5),
            explicit_bounds: vec![1.0, 2.0],
            ..Default::default()
        };
        let request = request(vec![histogram(vec![point(5), point(6)])]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        assert_eq!(parsed.skipped, 0);
        assert_eq!(parsed.samples.len(), 2);
        let Reading::Histogram(reading) = &parsed.samples[0].reading else { panic!() };
        assert_eq!((reading.count, reading.sum), (5, Some(12.5)));
    }

    #[test]
    fn shares_the_bounds_of_one_metric() {
        let point = || HistogramDataPoint {
            time_unix_nano: 1,
            count: 1,
            explicit_bounds: vec![1.0],
            bucket_counts: vec![1, 0],
            ..Default::default()
        };
        let request = request(vec![histogram(vec![point(), point()])]);

        let parsed = to_metric_samples(&request, &HostIndex::default(), "ing_1", 0);

        let bounds = |index: usize| parsed.samples[index].series.bounds.clone().unwrap();
        assert!(Arc::ptr_eq(&bounds(0), &bounds(1)));
    }

    #[test]
    fn parses_metric_kinds() {
        assert_eq!("gauge".parse(), Ok(MetricKind::Gauge));
        assert!("summary".parse::<MetricKind>().is_err());
        assert_eq!(Temporality::try_from(1), Ok(Temporality::Delta));
        assert!(Temporality::try_from(0).is_err());
    }
}
