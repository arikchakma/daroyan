use std::sync::Arc;

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::KeyValue;
use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value as NumberValue;
use opentelemetry_proto::tonic::metrics::v1::{
    AggregationTemporality, Exemplar, ExponentialHistogramDataPoint, HistogramDataPoint,
    NumberDataPoint,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::analytics::clamp_future_ms;
use crate::analytics::metrics::histogram::{exponential_bounds, to_fixed_buckets};
use crate::analytics::spans::{
    UNKNOWN_SERVICE, find_text, to_hex, to_json_map, to_json_value, truncate,
};
use crate::servers::hosts::HostIndex;

const MAX_NAME_CHARS: usize = 255;
const MAX_DESCRIPTION_CHARS: usize = 1024;
const MAX_ATTRIBUTES: usize = 32;
const MAX_ATTRIBUTE_CHARS: usize = 256;
const MAX_BUCKETS: usize = 160;
const NO_RECORDED_VALUE: u32 = 1;

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

    pub fn read(text: &str) -> Option<Self> {
        match text {
            "sum" => Some(Self::Sum),
            "gauge" => Some(Self::Gauge),
            "histogram" => Some(Self::Histogram),
            _ => None,
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

    fn read(value: i32) -> Option<Self> {
        match AggregationTemporality::try_from(value).ok()? {
            AggregationTemporality::Delta => Some(Self::Delta),
            AggregationTemporality::Cumulative => Some(Self::Cumulative),
            AggregationTemporality::Unspecified => None,
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
        let origin = Origin {
            token_id: token_id.clone(),
            service: find_text(attributes, &["service.name"])
                .unwrap_or_else(|| UNKNOWN_SERVICE.to_string())
                .into(),
            server_id: find_text(attributes, &["host.name"])
                .and_then(|host_name| hosts.find(&host_name).map(Into::into)),
            instance: find_text(attributes, &["service.instance.id"]).map(Into::into),
            resource: serde_json::to_string(&to_json_map(attributes))
                .ok()
                .map(Into::into),
            received_at,
        };

        for metric in
            resource_metrics.scope_metrics.iter().flat_map(|scope| &scope.metrics)
        {
            let name: Arc<str> = truncate(&metric.name, MAX_NAME_CHARS).into();
            if name.is_empty() {
                parsed.skipped += count_points(metric.data.as_ref());
                continue;
            }

            let describe = |kind, is_monotonic, temporality| Descriptor {
                name: name.clone(),
                unit: truncate(&metric.unit, MAX_NAME_CHARS).into(),
                description: truncate(&metric.description, MAX_DESCRIPTION_CHARS).into(),
                kind,
                is_monotonic,
                temporality,
            };

            match &metric.data {
                Some(Data::Gauge(gauge)) => {
                    let descriptor = describe(MetricKind::Gauge, false, None);
                    for point in &gauge.data_points {
                        push_number(&mut parsed, &origin, &descriptor, point);
                    }
                }
                Some(Data::Sum(sum)) => {
                    let Some(temporality) =
                        Temporality::read(sum.aggregation_temporality)
                    else {
                        parsed.skipped += sum.data_points.len() as u64;
                        continue;
                    };
                    let descriptor =
                        describe(MetricKind::Sum, sum.is_monotonic, Some(temporality));
                    for point in &sum.data_points {
                        push_number(&mut parsed, &origin, &descriptor, point);
                    }
                }
                Some(Data::Histogram(histogram)) => {
                    let Some(temporality) =
                        Temporality::read(histogram.aggregation_temporality)
                    else {
                        parsed.skipped += histogram.data_points.len() as u64;
                        continue;
                    };
                    let descriptor =
                        describe(MetricKind::Histogram, false, Some(temporality));
                    for point in &histogram.data_points {
                        push_histogram(&mut parsed, &origin, &descriptor, point);
                    }
                }
                Some(Data::ExponentialHistogram(histogram)) => {
                    let Some(temporality) =
                        Temporality::read(histogram.aggregation_temporality)
                    else {
                        parsed.skipped += histogram.data_points.len() as u64;
                        continue;
                    };
                    let descriptor =
                        describe(MetricKind::Histogram, false, Some(temporality));
                    let bounds: Arc<[f64]> = exponential_bounds().into();
                    for point in &histogram.data_points {
                        push_exponential(
                            &mut parsed,
                            &origin,
                            &descriptor,
                            &bounds,
                            point,
                        );
                    }
                }
                data => parsed.skipped += count_points(data.as_ref()),
            }
        }
    }

    parsed
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
        None,
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
        },
        Reading::Number(value),
    );
}

fn push_histogram(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    point: &HistogramDataPoint,
) {
    if point.flags & NO_RECORDED_VALUE != 0 {
        return;
    }

    let mut bounds = point.explicit_bounds.clone();
    let mut buckets = point.bucket_counts.clone();
    if bounds.is_empty() && buckets.is_empty() {
        buckets.push(point.count);
    }

    let is_sorted = bounds.windows(2).all(|pair| pair[0] < pair[1]);
    if buckets.len() != bounds.len() + 1 || buckets.len() > MAX_BUCKETS || !is_sorted {
        parsed.skipped += 1;
        return;
    }

    bounds.shrink_to_fit();
    push_sample(
        parsed,
        origin,
        descriptor,
        Some(bounds.into()),
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
        },
        Reading::Histogram(HistogramReading {
            count: point.count,
            sum: point.sum,
            min: point.min,
            max: point.max,
            buckets,
        }),
    );
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

    push_sample(
        parsed,
        origin,
        descriptor,
        Some(bounds.clone()),
        PointParts {
            at_nanos: point.time_unix_nano,
            start_nanos: point.start_time_unix_nano,
            attributes: &point.attributes,
            exemplars: &point.exemplars,
        },
        Reading::Histogram(HistogramReading {
            count: point.count,
            sum: point.sum,
            min: point.min,
            max: point.max,
            buckets: to_fixed_buckets(point),
        }),
    );
}

struct PointParts<'a> {
    at_nanos: u64,
    start_nanos: u64,
    attributes: &'a [KeyValue],
    exemplars: &'a [Exemplar],
}

fn push_sample(
    parsed: &mut ParsedMetrics,
    origin: &Origin,
    descriptor: &Descriptor,
    bounds: Option<Arc<[f64]>>,
    parts: PointParts,
    reading: Reading,
) {
    if parts.at_nanos == 0 {
        parsed.skipped += 1;
        return;
    }

    let at = clamp_future_ms(to_millis(parts.at_nanos), origin.received_at);
    let start_at = (parts.start_nanos > 0).then(|| to_millis(parts.start_nanos));
    let attributes = to_series_attributes(parts.attributes);
    let series_id = hash_series(origin, descriptor, bounds.as_deref(), &attributes);
    let exemplar = parts.exemplars.iter().rev().find_map(|exemplar| {
        Some((to_hex(&exemplar.trace_id)?, to_hex(&exemplar.span_id)?))
    });

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
            bounds,
            first_seen: at,
        },
        at,
        start_at,
        reading,
        trace_id: exemplar.as_ref().map(|(trace_id, _)| trace_id.clone()),
        span_id: exemplar.map(|(_, span_id)| span_id),
    });
}

fn to_millis(nanos: u64) -> i64 {
    i64::try_from(nanos / 1_000_000).unwrap_or(i64::MAX)
}

fn to_series_attributes(attributes: &[KeyValue]) -> String {
    let mut values = Map::new();
    for attribute in attributes {
        let value = attribute.value.as_ref().map(to_json_value).unwrap_or(Value::Null);
        let value = match value {
            Value::String(text) => Value::String(truncate(&text, MAX_ATTRIBUTE_CHARS)),
            value => value,
        };
        values.insert(truncate(&attribute.key, MAX_NAME_CHARS), value);
    }

    let values: Map<String, Value> = values.into_iter().take(MAX_ATTRIBUTES).collect();

    serde_json::to_string(&values).unwrap_or_else(|_| "{}".to_string())
}

fn hash_series(
    origin: &Origin,
    descriptor: &Descriptor,
    bounds: Option<&[f64]>,
    attributes: &str,
) -> i64 {
    let bounds = bounds
        .map(|bounds| bounds.iter().map(f64::to_string).collect::<Vec<_>>().join(","))
        .unwrap_or_default();
    let parts = [
        &*origin.token_id,
        &*descriptor.name,
        descriptor.kind.as_str(),
        if descriptor.is_monotonic { "monotonic" } else { "" },
        descriptor.temporality.map(Temporality::as_str).unwrap_or_default(),
        &*origin.service,
        origin.server_id.as_deref().unwrap_or_default(),
        origin.instance.as_deref().unwrap_or_default(),
        attributes,
        &bounds,
    ];
    let digest = Sha256::digest(parts.join("\u{1f}").as_bytes());

    i64::from_be_bytes(digest[..8].try_into().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use opentelemetry_proto::tonic::common::v1::AnyValue;
    use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
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
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![
                        text("service.name", "inboxjot-api"),
                        text("service.instance.id", "worker-1"),
                    ],
                    ..Default::default()
                }),
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
}
