//! Small numeric statistics helpers used by the top-level metrics.
//!
//! Each function here is a self-contained piece of arithmetic -- widening a
//! `u64` too large for an exact `f64` cast, reading a percentile out of a
//! sorted sample, or ranking a tied vector -- that [`super::ratio`],
//! [`super::paired_bootstrap`], and [`super::spearman_milli`] build on.

/// Widen a value too large for an exact `f64::from`, accepting the rounding
/// that only matters far above any sample size this benchmark runs.
pub(super) fn lossy(value: u64) -> f64 {
    let high = f64::from(u32::try_from(value >> 32).unwrap_or(u32::MAX));
    let low = f64::from(u32::try_from(value & 0xFFFF_FFFF).unwrap_or(0));
    high.mul_add(4_294_967_296.0, low)
}

/// Read a percentile out of an already-sorted sample, by linear
/// interpolation between the two nearest ranks.
///
/// `per_mille` is the percentile scaled by ten (`25` for the 2.5th, `975` for
/// the 97.5th), so the rank arithmetic stays in integers up to the final
/// interpolation weight and no float is ever truncated back into an index.
pub(super) fn percentile(sorted: &[f64], per_mille: u32) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    if last == 0 {
        return sorted[0];
    }
    let last = u64::try_from(last).unwrap_or(u64::MAX);
    let numerator = u64::from(per_mille) * last;
    let lower = usize::try_from(numerator / 1000).unwrap_or(0);
    let remainder = numerator % 1000;
    let upper = (lower + 1).min(sorted.len() - 1);
    if remainder == 0 || lower == upper {
        return sorted[lower];
    }
    let weight = f64::from(u32::try_from(remainder).unwrap_or(0)) / 1000.0;
    sorted[lower] + (sorted[upper] - sorted[lower]) * weight
}

/// Doubled average ranks (1-based), so a tie group gets an exact integer.
pub(super) fn doubled_ranks(values: &[u32]) -> Vec<i64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by_key(|&index| values[index]);
    let mut doubled = vec![0_i64; values.len()];
    let mut position = 0_usize;
    while position < order.len() {
        let mut end = position;
        while end + 1 < order.len() && values[order[end + 1]] == values[order[position]] {
            end += 1;
        }
        // 1-based first and last rank of the tie group.
        let first = i64::try_from(position + 1).unwrap_or(i64::MAX);
        let last = i64::try_from(end + 1).unwrap_or(i64::MAX);
        let doubled_rank = first + last;
        for slot in order.iter().take(end + 1).skip(position) {
            doubled[*slot] = doubled_rank;
        }
        position = end + 1;
    }
    doubled
}
