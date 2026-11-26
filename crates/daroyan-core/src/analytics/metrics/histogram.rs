use std::sync::{Arc, LazyLock};

use opentelemetry_proto::tonic::metrics::v1::ExponentialHistogramDataPoint;

const LOWEST_STEP: i32 = -20;
const HIGHEST_STEP: i32 = 80;
const LOWEST_SCALE: i32 = -10;
const HIGHEST_SCALE: i32 = 20;
const SLOTS: usize = HIGHEST_STEP.abs_diff(LOWEST_STEP) as usize + 2;

static EXPONENTIAL_BOUNDS: LazyLock<Arc<[f64]>> = LazyLock::new(|| {
    (LOWEST_STEP..=HIGHEST_STEP)
        .map(|step| 2_f64.powf(f64::from(step) / 2.0))
        .collect()
});

pub fn exponential_bounds() -> Arc<[f64]> {
    EXPONENTIAL_BOUNDS.clone()
}

pub fn to_fixed_buckets(point: &ExponentialHistogramDataPoint) -> Option<Vec<u64>> {
    if !(LOWEST_SCALE..=HIGHEST_SCALE).contains(&point.scale) {
        return None;
    }

    let mut buckets = vec![0; SLOTS];
    buckets[0] = point.zero_count;

    if let Some(negative) = &point.negative {
        buckets[0] = buckets[0].checked_add(sum_counts(&negative.bucket_counts)?)?;
    }

    let Some(positive) = &point.positive else { return Some(buckets) };

    for (offset, count) in (0_i64..).zip(&positive.bucket_counts) {
        let index = i64::from(positive.offset) + offset;
        let (lower, upper) = to_half_steps(index, point.scale);
        spread(&mut buckets, lower, upper, *count)?;
    }

    Some(buckets)
}

pub fn sum_counts(counts: &[u64]) -> Option<u64> {
    counts
        .iter()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
}

fn to_half_steps(index: i64, scale: i32) -> (i64, i64) {
    if scale >= 1 {
        let step = index >> (scale - 1);
        return (step, step + 1);
    }

    (index << (1 - scale), (index + 1) << (1 - scale))
}

fn to_slot(step: i64) -> usize {
    let slot = step - i64::from(LOWEST_STEP) + 1;

    usize::try_from(slot).unwrap_or_default().min(SLOTS - 1)
}

fn spread(buckets: &mut [u64], lower: i64, upper: i64, count: u64) -> Option<()> {
    let steps = u64::try_from(upper - lower).ok().filter(|steps| *steps > 0)?;
    let share = count / steps;
    let extra = count % steps;

    let (first, last) = (to_slot(lower), to_slot(upper - 1));
    for (slot, bucket) in buckets.iter_mut().enumerate().take(last + 1).skip(first) {
        let first_step = i64::from(LOWEST_STEP) + i64::try_from(slot).ok()? - 1;
        let from = if slot == 0 { lower } else { lower.max(first_step) };
        let to = if slot == SLOTS - 1 { upper } else { upper.min(first_step + 1) };

        let start = u64::try_from(from - lower).ok()?;
        let end = u64::try_from(to - lower).ok()?;
        let total = share
            .checked_mul(end - start)?
            .checked_add(extra.min(end).saturating_sub(start))?;

        *bucket = bucket.checked_add(total)?;
    }

    Some(())
}

pub fn quantile_of(bounds: &[f64], buckets: &[u64], quantile: f64) -> Option<f64> {
    let total = buckets
        .iter()
        .fold(0_u64, |total, count| total.saturating_add(*count));
    if total == 0 || buckets.len() != bounds.len() + 1 {
        return None;
    }

    let rank = quantile.clamp(0.0, 1.0) * total as f64;
    let mut seen = 0_u64;
    for (index, count) in buckets.iter().enumerate() {
        if *count == 0 {
            continue;
        }

        let before = seen;
        seen = seen.saturating_add(*count);
        if (seen as f64) < rank {
            continue;
        }

        if index == bounds.len() {
            return bounds.last().copied();
        }

        let upper = bounds[index];
        let lower = if index == 0 { upper.min(0.0) } else { bounds[index - 1] };
        let share = (rank - before as f64) / *count as f64;

        return Some(lower + (upper - lower) * share);
    }

    bounds.last().copied()
}

#[cfg(test)]
mod tests {
    use opentelemetry_proto::tonic::metrics::v1::ExponentialHistogramDataPoint;
    use opentelemetry_proto::tonic::metrics::v1::exponential_histogram_data_point::Buckets;

    use super::*;

    #[test]
    fn interpolates_inside_the_bucket() {
        let bounds = [100.0, 200.0, 400.0];
        let buckets = [0, 50, 50, 0];

        assert_eq!(quantile_of(&bounds, &buckets, 0.5), Some(200.0));
        assert_eq!(quantile_of(&bounds, &buckets, 0.75), Some(300.0));
        assert_eq!(quantile_of(&bounds, &buckets, 0.25), Some(150.0));
    }

    #[test]
    fn caps_at_the_last_bound() {
        let bounds = [1.0, 2.0];
        let buckets = [0, 0, 10];

        assert_eq!(quantile_of(&bounds, &buckets, 0.99), Some(2.0));
        assert_eq!(quantile_of(&bounds, &[0, 0, 0], 0.5), None);
    }

    #[test]
    fn survives_huge_bucket_counts() {
        let bounds = [1.0, 2.0];
        let buckets = [u64::MAX, u64::MAX, 0];

        assert_eq!(quantile_of(&bounds, &buckets, 0.5), Some(0.5));
    }

    #[test]
    fn maps_exponential_buckets_onto_half_powers_of_two() {
        let bounds = exponential_bounds();
        let point = ExponentialHistogramDataPoint {
            scale: 2,
            zero_count: 1,
            positive: Some(Buckets { offset: 4, bucket_counts: vec![3, 4] }),
            ..Default::default()
        };
        let buckets = to_fixed_buckets(&point).unwrap();

        assert_eq!(buckets.len(), bounds.len() + 1);
        assert_eq!(buckets[0], 1);
        assert_eq!(buckets.iter().sum::<u64>(), 8);

        let slot = buckets.iter().rposition(|count| *count > 0).unwrap();
        assert!(bounds[slot - 1] <= 2.0 && bounds[slot] >= 2.0);
    }

    #[test]
    fn rejects_bad_scales_and_overflowing_counts() {
        let point = |scale, counts| ExponentialHistogramDataPoint {
            scale,
            zero_count: 1,
            positive: Some(Buckets { offset: 0, bucket_counts: counts }),
            ..Default::default()
        };

        assert_eq!(to_fixed_buckets(&point(i32::MIN, vec![1])), None);
        assert_eq!(to_fixed_buckets(&point(21, vec![1])), None);
        assert_eq!(to_fixed_buckets(&point(20, vec![u64::MAX, u64::MAX])), None);
        assert!(to_fixed_buckets(&point(-10, vec![u64::MAX - 1])).is_some());
        assert!(to_fixed_buckets(&point(20, vec![1])).is_some());
    }

    #[test]
    fn spreads_wide_exponential_buckets_over_their_range() {
        let bounds = exponential_bounds();
        let point = |scale, offset| ExponentialHistogramDataPoint {
            scale,
            positive: Some(Buckets { offset, bucket_counts: vec![4] }),
            ..Default::default()
        };

        let buckets = to_fixed_buckets(&point(0, 0)).unwrap();
        let p99 = quantile_of(&bounds, &buckets, 0.99).unwrap();
        assert!(p99 > 1.9 && p99 <= 2.0, "{p99}");

        let buckets = to_fixed_buckets(&point(-1, 0)).unwrap();
        assert_eq!(buckets.iter().filter(|count| **count == 1).count(), 4);
        let p99 = quantile_of(&bounds, &buckets, 0.99).unwrap();
        assert!(p99 > 3.8 && p99 <= 4.0, "{p99}");

        let buckets = to_fixed_buckets(&point(-10, 0)).unwrap();
        assert_eq!(buckets.iter().sum::<u64>(), 4);
        assert_eq!(buckets[SLOTS - 1], 0);
    }
}
