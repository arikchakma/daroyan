use opentelemetry_proto::tonic::metrics::v1::ExponentialHistogramDataPoint;

const LOWEST_STEP: i32 = -20;
const HIGHEST_STEP: i32 = 80;

pub fn exponential_bounds() -> Vec<f64> {
    (LOWEST_STEP..=HIGHEST_STEP)
        .map(|step| 2_f64.powf(f64::from(step) / 2.0))
        .collect()
}

pub fn to_fixed_buckets(point: &ExponentialHistogramDataPoint) -> Vec<u64> {
    let size = (HIGHEST_STEP - LOWEST_STEP + 2) as usize;
    let mut buckets = vec![0; size];
    buckets[0] = point.zero_count;

    if let Some(negative) = &point.negative {
        buckets[0] += negative.bucket_counts.iter().sum::<u64>();
    }

    let Some(positive) = &point.positive else { return buckets };
    for (offset, count) in positive.bucket_counts.iter().enumerate() {
        let index = i64::from(positive.offset) + offset as i64;
        let step = to_half_step(index, point.scale);
        let slot = (step - i64::from(LOWEST_STEP) + 1).clamp(0, size as i64 - 1);
        buckets[slot as usize] += count;
    }

    buckets
}

fn to_half_step(index: i64, scale: i32) -> i64 {
    if scale >= 1 {
        return index >> (scale - 1).min(62);
    }

    index << (1 - scale).min(62)
}

pub fn quantile_of(bounds: &[f64], buckets: &[u64], quantile: f64) -> Option<f64> {
    let total: u64 = buckets.iter().sum();
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
        seen += count;
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
    fn maps_exponential_buckets_onto_half_powers_of_two() {
        let bounds = exponential_bounds();
        let point = ExponentialHistogramDataPoint {
            scale: 2,
            zero_count: 1,
            positive: Some(Buckets { offset: 4, bucket_counts: vec![3, 4] }),
            ..Default::default()
        };
        let buckets = to_fixed_buckets(&point);

        assert_eq!(buckets.len(), bounds.len() + 1);
        assert_eq!(buckets[0], 1);
        assert_eq!(buckets.iter().sum::<u64>(), 8);

        let slot = buckets.iter().rposition(|count| *count > 0).unwrap();
        assert!(bounds[slot - 1] <= 2.0 && bounds[slot] >= 2.0);
    }
}
