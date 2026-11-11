use std::collections::HashMap;
use std::sync::Arc;

use crate::analytics::metrics::ingest::{
    HistogramReading, MetricKind, MetricSample, Reading, SeriesRow, Temporality,
};
use crate::config::MetricSettings;

const ACTIVE_MS: i64 = 60 * 60 * 1000;
const FRESH_START_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Copy, Debug)]
pub struct SeriesLimits {
    pub per_token: usize,
    pub per_metric: usize,
}

impl From<&MetricSettings> for SeriesLimits {
    fn from(settings: &MetricSettings) -> Self {
        Self {
            per_token: settings.series_limit,
            per_metric: settings.series_per_metric,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct PointRow {
    pub series_id: i64,
    pub at: i64,
    pub start_at: Option<i64>,
    pub value: Option<f64>,
    pub delta: Option<f64>,
    pub count: Option<u64>,
    pub sum: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub buckets: Option<Vec<u64>>,
    pub count_delta: Option<u64>,
    pub sum_delta: Option<f64>,
    pub bucket_deltas: Option<Vec<u64>>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct Admission {
    pub series: Vec<SeriesRow>,
    pub points: Vec<PointRow>,
    pub rejected: u64,
    pub full_metric: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct LastPoint {
    pub at: i64,
    pub start_at: Option<i64>,
    pub reading: Reading,
}

pub struct KnownSeries {
    pub series_id: i64,
    pub token_id: Arc<str>,
    pub name: Arc<str>,
    pub last_seen: i64,
    pub last: Option<LastPoint>,
}

struct Tracked {
    token_id: Arc<str>,
    name: Arc<str>,
    last_seen: i64,
    last: Option<LastPoint>,
}

#[derive(Default)]
pub struct SeriesCache {
    series: HashMap<i64, Tracked>,
    is_loaded: bool,
}

impl SeriesCache {
    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    pub fn load(&mut self, known: Vec<KnownSeries>) {
        if self.is_loaded {
            return;
        }

        for series in known {
            self.series.entry(series.series_id).or_insert(Tracked {
                token_id: series.token_id,
                name: series.name,
                last_seen: series.last_seen,
                last: series.last,
            });
        }
        self.is_loaded = true;
    }

    pub fn forget_before(&mut self, cutoff: i64) {
        self.series.retain(|_, series| series.last_seen >= cutoff);
    }

    pub fn admit(
        &mut self,
        mut samples: Vec<MetricSample>,
        limits: SeriesLimits,
        now: i64,
    ) -> Admission {
        samples.sort_by_key(|sample| sample.at);

        let mut admission = Admission::default();
        let (mut per_token, mut per_metric) = self.count_active(now);

        for sample in samples {
            let series_id = sample.series.series_id;
            let is_active = self
                .series
                .get(&series_id)
                .is_some_and(|series| series.last_seen >= now - ACTIVE_MS);

            if !is_active {
                let token_count =
                    per_token.entry(sample.series.token_id.clone()).or_default();
                let metric_count = per_metric
                    .entry((sample.series.token_id.clone(), sample.series.name.clone()))
                    .or_default();
                if *token_count >= limits.per_token || *metric_count >= limits.per_metric
                {
                    admission.rejected += 1;
                    admission
                        .full_metric
                        .get_or_insert_with(|| sample.series.name.clone());
                    continue;
                }

                *token_count += 1;
                *metric_count += 1;
            }

            let tracked = self.series.get(&series_id);
            if tracked
                .and_then(|series| series.last.as_ref())
                .is_some_and(|last| sample.at <= last.at)
            {
                continue;
            }

            let point =
                to_point(&sample, tracked.and_then(|series| series.last.as_ref()));
            let last = LastPoint {
                at: sample.at,
                start_at: sample.start_at,
                reading: sample.reading.clone(),
            };

            match self.series.get_mut(&series_id) {
                Some(series) => {
                    series.last_seen = series.last_seen.max(sample.at);
                    series.last = Some(last);
                }
                None => {
                    self.series.insert(
                        series_id,
                        Tracked {
                            token_id: sample.series.token_id.clone(),
                            name: sample.series.name.clone(),
                            last_seen: sample.at,
                            last: Some(last),
                        },
                    );
                    admission.series.push(sample.series);
                }
            }
            admission.points.push(point);
        }

        admission
    }

    fn count_active(
        &self,
        now: i64,
    ) -> (HashMap<Arc<str>, usize>, HashMap<(Arc<str>, Arc<str>), usize>) {
        let mut per_token: HashMap<Arc<str>, usize> = HashMap::new();
        let mut per_metric: HashMap<(Arc<str>, Arc<str>), usize> = HashMap::new();
        for series in self
            .series
            .values()
            .filter(|series| series.last_seen >= now - ACTIVE_MS)
        {
            *per_token.entry(series.token_id.clone()).or_default() += 1;
            *per_metric
                .entry((series.token_id.clone(), series.name.clone()))
                .or_default() += 1;
        }

        (per_token, per_metric)
    }
}

fn to_point(sample: &MetricSample, last: Option<&LastPoint>) -> PointRow {
    let mut point = PointRow {
        series_id: sample.series.series_id,
        at: sample.at,
        start_at: sample.start_at,
        trace_id: sample.trace_id.clone(),
        span_id: sample.span_id.clone(),
        ..PointRow::default()
    };

    match &sample.reading {
        Reading::Number(value) => {
            point.value = Some(*value);
            point.delta = number_change(sample, *value, last);
        }
        Reading::Histogram(histogram) => {
            point.count = Some(histogram.count);
            point.sum = histogram.sum;
            point.min = histogram.min;
            point.max = histogram.max;
            point.buckets = Some(histogram.buckets.clone());

            if let Some(change) = histogram_change(sample, histogram, last) {
                point.count_delta = Some(change.count);
                point.sum_delta = change.sum;
                point.bucket_deltas = Some(change.buckets);
            }
        }
    }

    point
}

fn number_change(
    sample: &MetricSample,
    value: f64,
    last: Option<&LastPoint>,
) -> Option<f64> {
    if !sample.series.is_counter() {
        return None;
    }
    if sample.series.temporality == Some(Temporality::Delta) {
        return Some(value);
    }

    let Some(last) = last else {
        return is_fresh_start(sample).then_some(value);
    };
    let Reading::Number(previous) = last.reading else { return Some(value) };
    if last.start_at != sample.start_at || value < previous {
        return Some(value);
    }

    Some(value - previous)
}

fn histogram_change(
    sample: &MetricSample,
    histogram: &HistogramReading,
    last: Option<&LastPoint>,
) -> Option<HistogramReading> {
    if sample.series.kind != MetricKind::Histogram {
        return None;
    }
    if sample.series.temporality == Some(Temporality::Delta) {
        return Some(histogram.clone());
    }

    let Some(last) = last else {
        return is_fresh_start(sample).then(|| histogram.clone());
    };
    let Reading::Histogram(previous) = &last.reading else {
        return Some(histogram.clone());
    };

    let is_reset = last.start_at != sample.start_at
        || histogram.count < previous.count
        || histogram.buckets.len() != previous.buckets.len()
        || histogram
            .buckets
            .iter()
            .zip(&previous.buckets)
            .any(|(now, before)| now < before);
    if is_reset {
        return Some(histogram.clone());
    }

    Some(HistogramReading {
        count: histogram.count - previous.count,
        sum: histogram.sum.zip(previous.sum).map(|(now, before)| now - before),
        min: None,
        max: None,
        buckets: histogram
            .buckets
            .iter()
            .zip(&previous.buckets)
            .map(|(now, before)| now - before)
            .collect(),
    })
}

fn is_fresh_start(sample: &MetricSample) -> bool {
    sample
        .start_at
        .is_some_and(|start_at| sample.at - start_at <= FRESH_START_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter(series_id: i64, name: &str, temporality: Temporality) -> SeriesRow {
        SeriesRow {
            series_id,
            token_id: "ing_1".into(),
            name: name.into(),
            unit: "".into(),
            description: "".into(),
            kind: MetricKind::Sum,
            is_monotonic: true,
            temporality: Some(temporality),
            service: "api".into(),
            server_id: None,
            instance: None,
            attributes: "{}".to_string(),
            resource: None,
            bounds: None,
            first_seen: 0,
        }
    }

    fn sample(series: SeriesRow, at: i64, start_at: i64, value: f64) -> MetricSample {
        MetricSample {
            series,
            at,
            start_at: Some(start_at),
            reading: Reading::Number(value),
            trace_id: None,
            span_id: None,
        }
    }

    const LIMITS: SeriesLimits = SeriesLimits { per_token: 10, per_metric: 10 };

    fn deltas(cache: &mut SeriesCache, samples: Vec<MetricSample>) -> Vec<Option<f64>> {
        let now = samples.iter().map(|sample| sample.at).max().unwrap_or_default();
        cache
            .admit(samples, LIMITS, now)
            .points
            .iter()
            .map(|point| point.delta)
            .collect()
    }

    #[test]
    fn turns_cumulative_counters_into_changes_and_handles_restarts() {
        let mut cache = SeriesCache::default();
        let make = || counter(1, "emails.sent", Temporality::Cumulative);

        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 60_000, 1_000, 5.0)]),
            [Some(5.0)]
        );
        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 120_000, 1_000, 12.0)]),
            [Some(7.0)]
        );
        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 180_000, 170_000, 2.0)]),
            [Some(2.0)]
        );
        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 240_000, 170_000, 1.0)]),
            [Some(1.0)]
        );
        assert!(
            deltas(&mut cache, vec![sample(make(), 240_000, 170_000, 9.0)]).is_empty()
        );
    }

    #[test]
    fn skips_the_first_point_of_a_long_running_counter() {
        let mut cache = SeriesCache::default();
        let make = || counter(1, "emails.sent", Temporality::Cumulative);

        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 86_400_000, 1_000, 900.0)]),
            [None]
        );
        assert_eq!(
            deltas(&mut cache, vec![sample(make(), 86_460_000, 1_000, 930.0)]),
            [Some(30.0)]
        );
    }

    #[test]
    fn keeps_delta_counters_as_sent() {
        let mut cache = SeriesCache::default();
        let make = || counter(1, "emails.sent", Temporality::Delta);

        assert_eq!(
            deltas(
                &mut cache,
                vec![
                    sample(make(), 120_000, 60_000, 4.0),
                    sample(make(), 60_000, 0, 3.0)
                ]
            ),
            [Some(3.0), Some(4.0)]
        );
    }

    #[test]
    fn rejects_new_series_past_the_limits() {
        let mut cache = SeriesCache::default();
        let limits = SeriesLimits { per_token: 3, per_metric: 2 };
        let samples = (1..=3)
            .map(|id| sample(counter(id, "a", Temporality::Delta), 1_000, 0, 1.0))
            .chain([sample(counter(9, "b", Temporality::Delta), 1_000, 0, 1.0)])
            .chain([sample(counter(10, "c", Temporality::Delta), 1_000, 0, 1.0)])
            .collect();

        let admission = cache.admit(samples, limits, 1_000);

        assert_eq!(admission.series.len(), 3);
        assert_eq!(admission.rejected, 2);
        assert_eq!(admission.full_metric.as_deref(), Some("a"));

        let later = cache.admit(
            vec![sample(counter(1, "a", Temporality::Delta), 2_000, 1_000, 1.0)],
            limits,
            2_000,
        );
        assert_eq!(later.points.len(), 1);
        assert_eq!(later.rejected, 0);
    }

    #[test]
    fn frees_room_when_series_go_quiet() {
        let mut cache = SeriesCache::default();
        let limits = SeriesLimits { per_token: 1, per_metric: 1 };
        cache.admit(
            vec![sample(counter(1, "a", Temporality::Delta), 0, 0, 1.0)],
            limits,
            0,
        );

        let later = ACTIVE_MS + 1;
        let admission = cache.admit(
            vec![sample(counter(2, "a", Temporality::Delta), later, 0, 1.0)],
            limits,
            later,
        );

        assert_eq!(admission.rejected, 0);
        assert_eq!(admission.points.len(), 1);
    }

    #[test]
    fn works_out_histogram_bucket_changes() {
        let mut cache = SeriesCache::default();
        let series = || SeriesRow {
            kind: MetricKind::Histogram,
            is_monotonic: false,
            bounds: Some(vec![1.0, 2.0].into()),
            ..counter(1, "job.duration", Temporality::Cumulative)
        };
        let reading = |count, buckets: Vec<u64>| {
            Reading::Histogram(HistogramReading {
                count,
                sum: Some(count as f64),
                min: None,
                max: None,
                buckets,
            })
        };
        let make = |at, count, buckets| MetricSample {
            series: series(),
            at,
            start_at: Some(0),
            reading: reading(count, buckets),
            trace_id: None,
            span_id: None,
        };

        cache.admit(vec![make(60_000, 3, vec![1, 1, 1])], LIMITS, 60_000);
        let admission =
            cache.admit(vec![make(120_000, 7, vec![1, 4, 2])], LIMITS, 120_000);

        let point = &admission.points[0];
        assert_eq!(point.count_delta, Some(4));
        assert_eq!(point.sum_delta, Some(4.0));
        assert_eq!(point.bucket_deltas, Some(vec![0, 3, 1]));
    }
}
