use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use crate::analytics::metrics::ingest::{
    HistogramReading, MetricKind, MetricSample, Reading, SeriesRow, Temporality,
};
use crate::config::MetricSettings;

pub const IDLE_MS: i64 = 60 * 60 * 1000;
const SWEEP_MS: i64 = 60 * 1000;
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
    pub rejected: u64,
    pub out_of_order: u64,
    pub full_metric: Option<Arc<str>>,
}

#[derive(Debug, Default)]
pub struct Admitted {
    pub series: Vec<SeriesRow>,
    pub points: Vec<PointRow>,
    pub admission: Admission,
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
    active_at: i64,
    last: Option<LastPoint>,
}

#[derive(Default)]
struct Counts {
    per_token: HashMap<Arc<str>, usize>,
    per_metric: HashMap<(Arc<str>, Arc<str>), usize>,
}

impl Counts {
    fn is_full(&self, series: &SeriesRow, limits: SeriesLimits) -> bool {
        let token_count =
            self.per_token.get(&series.token_id).copied().unwrap_or_default();
        let metric_count = self
            .per_metric
            .get(&(series.token_id.clone(), series.name.clone()))
            .copied()
            .unwrap_or_default();

        token_count >= limits.per_token || metric_count >= limits.per_metric
    }

    fn add(&mut self, tracked: &Tracked) {
        *self.per_token.entry(tracked.token_id.clone()).or_default() += 1;
        *self
            .per_metric
            .entry((tracked.token_id.clone(), tracked.name.clone()))
            .or_default() += 1;
    }

    fn remove(&mut self, tracked: &Tracked) {
        if let Entry::Occupied(mut entry) = self.per_token.entry(tracked.token_id.clone())
        {
            *entry.get_mut() -= 1;
            if *entry.get() == 0 {
                entry.remove();
            }
        }

        let key = (tracked.token_id.clone(), tracked.name.clone());
        if let Entry::Occupied(mut entry) = self.per_metric.entry(key) {
            *entry.get_mut() -= 1;
            if *entry.get() == 0 {
                entry.remove();
            }
        }
    }
}

#[derive(Default)]
pub struct SeriesCache {
    series: HashMap<i64, Tracked>,
    counts: Counts,
    swept_at: i64,
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
            let Entry::Vacant(entry) = self.series.entry(series.series_id) else {
                continue;
            };

            let tracked = Tracked {
                token_id: series.token_id,
                name: series.name,
                active_at: series.last_seen,
                last: series.last,
            };
            self.counts.add(&tracked);
            entry.insert(tracked);
        }

        self.is_loaded = true;
    }

    pub fn forget_idle(&mut self, now: i64) {
        let cutoff = now - IDLE_MS;
        let counts = &mut self.counts;

        self.series.retain(|_, tracked| {
            let is_active = tracked.active_at >= cutoff;
            if !is_active {
                counts.remove(tracked);
            }

            is_active
        });

        self.swept_at = now;
    }

    pub fn forget(&mut self, series_ids: &[i64]) {
        for series_id in series_ids {
            if let Some(tracked) = self.series.remove(series_id) {
                self.counts.remove(&tracked);
            }
        }
    }

    pub fn admit(
        &mut self,
        mut samples: Vec<MetricSample>,
        limits: SeriesLimits,
        now: i64,
    ) -> Admitted {
        if now - self.swept_at >= SWEEP_MS {
            self.forget_idle(now);
        }

        samples.sort_by_key(|sample| sample.at);

        let mut admitted = Admitted::default();
        for sample in samples {
            match self.series.entry(sample.series.series_id) {
                Entry::Occupied(entry) => {
                    let tracked = entry.into_mut();
                    tracked.active_at = now;

                    if tracked.last.as_ref().is_some_and(|last| sample.at <= last.at) {
                        if sample.series.temporality == Some(Temporality::Delta) {
                            admitted.admission.out_of_order += 1;
                        }
                        continue;
                    }

                    let (_, point, last) = to_point(sample, tracked.last.as_ref());
                    tracked.last = Some(last);
                    admitted.points.push(point);
                }
                Entry::Vacant(entry) => {
                    if self.counts.is_full(&sample.series, limits) {
                        admitted.admission.rejected += 1;
                        admitted
                            .admission
                            .full_metric
                            .get_or_insert_with(|| sample.series.name.clone());
                        continue;
                    }

                    let (series, point, last) = to_point(sample, None);
                    let tracked = Tracked {
                        token_id: series.token_id.clone(),
                        name: series.name.clone(),
                        active_at: now,
                        last: Some(last),
                    };
                    self.counts.add(&tracked);
                    entry.insert(tracked);

                    admitted.series.push(series);
                    admitted.points.push(point);
                }
            }
        }

        admitted
    }
}

enum HistogramChange {
    Whole,
    Since(HistogramReading),
}

fn to_point(
    sample: MetricSample,
    last: Option<&LastPoint>,
) -> (SeriesRow, PointRow, LastPoint) {
    let mut point = PointRow {
        series_id: sample.series.series_id,
        at: sample.at,
        start_at: sample.start_at,
        ..PointRow::default()
    };

    match &sample.reading {
        Reading::Number(value) => {
            point.value = Some(*value);
            point.delta = number_change(&sample, *value, last);
        }
        Reading::Histogram(histogram) => {
            point.count = Some(histogram.count);
            point.sum = histogram.sum;
            point.min = histogram.min;
            point.max = histogram.max;
            point.buckets = Some(histogram.buckets.clone());

            match histogram_change(&sample, histogram, last) {
                Some(HistogramChange::Whole) => {
                    point.count_delta = Some(histogram.count);
                    point.sum_delta = histogram.sum;
                    point.bucket_deltas = Some(histogram.buckets.clone());
                }
                Some(HistogramChange::Since(change)) => {
                    point.count_delta = Some(change.count);
                    point.sum_delta = change.sum;
                    point.bucket_deltas = Some(change.buckets);
                }
                None => {}
            }
        }
    }

    let MetricSample { series, at, start_at, reading, trace_id, span_id } = sample;
    point.trace_id = trace_id;
    point.span_id = span_id;

    (series, point, LastPoint { at, start_at, reading })
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
) -> Option<HistogramChange> {
    if sample.series.kind != MetricKind::Histogram {
        return None;
    }
    if sample.series.temporality == Some(Temporality::Delta) {
        return Some(HistogramChange::Whole);
    }

    let Some(last) = last else {
        return is_fresh_start(sample).then_some(HistogramChange::Whole);
    };
    let Reading::Histogram(previous) = &last.reading else {
        return Some(HistogramChange::Whole);
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
        return Some(HistogramChange::Whole);
    }

    Some(HistogramChange::Since(HistogramReading {
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
    }))
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
        assert_eq!(admission.admission.rejected, 2);
        assert_eq!(admission.admission.full_metric.as_deref(), Some("a"));

        let later = cache.admit(
            vec![sample(counter(1, "a", Temporality::Delta), 2_000, 1_000, 1.0)],
            limits,
            2_000,
        );
        assert_eq!(later.points.len(), 1);
        assert_eq!(later.admission.rejected, 0);
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

        let later = IDLE_MS + 1;
        let admission = cache.admit(
            vec![sample(counter(2, "a", Temporality::Delta), later, 0, 1.0)],
            limits,
            later,
        );

        assert_eq!(admission.admission.rejected, 0);
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

    #[test]
    fn counts_activity_by_arrival_time_not_point_time() {
        let mut cache = SeriesCache::default();
        let limits = SeriesLimits { per_token: 1, per_metric: 1 };
        let now = 10 * IDLE_MS;
        let old = now - 2 * IDLE_MS;

        let first = cache.admit(
            vec![sample(counter(1, "a", Temporality::Delta), old, 0, 1.0)],
            limits,
            now,
        );
        let second = cache.admit(
            vec![sample(counter(2, "a", Temporality::Delta), old, 0, 1.0)],
            limits,
            now + 1,
        );

        assert_eq!(first.admission.rejected, 0);
        assert_eq!(second.admission.rejected, 1);
        assert!(second.points.is_empty());
    }

    #[test]
    fn comes_back_after_going_idle_without_a_made_up_change() {
        let mut cache = SeriesCache::default();
        let make = || counter(1, "emails.sent", Temporality::Cumulative);
        cache.admit(vec![sample(make(), 60_000, 1_000, 5.0)], LIMITS, 60_000);

        let later = 60_000 + IDLE_MS + SWEEP_MS;
        let admitted =
            cache.admit(vec![sample(make(), later, 1_000, 900.0)], LIMITS, later);

        assert_eq!(cache.series.len(), 1);
        assert_eq!(admitted.series.len(), 1);
        assert_eq!(admitted.points[0].delta, None);
    }

    #[test]
    fn writes_the_series_again_after_a_failed_flush() {
        let mut cache = SeriesCache::default();
        let make = || counter(1, "emails.sent", Temporality::Delta);
        let first = cache.admit(vec![sample(make(), 1_000, 0, 1.0)], LIMITS, 1_000);
        assert_eq!(first.series.len(), 1);

        cache.forget(&[1]);
        let second = cache.admit(vec![sample(make(), 2_000, 1_000, 1.0)], LIMITS, 2_000);

        assert_eq!(second.series.len(), 1);
        assert!(cache.counts.per_token.values().all(|count| *count == 1));
    }

    #[test]
    fn counts_late_delta_points_as_out_of_order() {
        let mut cache = SeriesCache::default();
        let delta = || counter(1, "emails.sent", Temporality::Delta);
        let cumulative = || counter(2, "emails.sent", Temporality::Cumulative);
        cache.admit(
            vec![sample(delta(), 2_000, 1_000, 1.0), sample(cumulative(), 2_000, 0, 5.0)],
            LIMITS,
            2_000,
        );

        let admitted = cache.admit(
            vec![sample(delta(), 1_500, 1_000, 1.0), sample(cumulative(), 2_000, 0, 5.0)],
            LIMITS,
            3_000,
        );

        assert!(admitted.points.is_empty());
        assert_eq!(admitted.admission.out_of_order, 1);
        assert_eq!(admitted.admission.rejected, 0);
    }
}
