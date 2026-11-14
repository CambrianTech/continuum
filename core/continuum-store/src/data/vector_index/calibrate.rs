//! Measures the prefilter's candidate count against the recall requirement.
//!
//! For evenly spaced probe rows (each used as a query against the other rows), the exact
//! top-j is ranked by dot product, and each true neighbor's position in the sign-sketch
//! ordering is read off a cumulative Hamming histogram: the number of rows a radius search
//! must admit to keep it. A probe's need at depth j is the largest such count among its
//! first j neighbors. The candidate count for k is the `target` quantile of the probes'
//! needs at depth k, so `target` of the probes keep their COMPLETE top-k. Nothing here is a
//! tuned constant: the probe count and the measured depth both come from the target.
use super::cpu::{dot, hamming, Ranked, Rows};
use super::RecallTarget;
use std::num::NonZeroUsize;

/// Candidates needed per k, measured on the rows they belong to.
#[derive(Debug, Clone)]
pub(super) enum CandidateNeed {
    /// Fewer than two rows: there is no other row to measure a probe against, so every row
    /// is scored (a measured fact about this corpus, not a fallback).
    Unmeasurable,
    /// `by_k[j - 1]`: candidates so that `target` of the probes keep their whole top-j.
    Measured { by_k: Vec<NonZeroUsize> },
}

impl CandidateNeed {
    /// Rows to admit for `k`; `None` means every row is scored.
    pub(super) fn candidates(&self, k: NonZeroUsize) -> Option<NonZeroUsize> {
        let Self::Measured { by_k } = self else { return None };
        let deepest = by_k.last()?;
        match by_k.get(k.get() - 1) {
            Some(need) => Some(*need),
            // Past the measured depth, the deepest need scales linearly with k.
            None => deepest.checked_mul(k).map(|scaled| scaled.get().div_ceil(by_k.len())).and_then(NonZeroUsize::new),
        }
    }
}

/// Probes enough rows that the `target` quantile is an observed value, not an interpolation
/// past the last sample: 1 / (1 - target) probes (100 for 0.99), every row for target 1.
fn probes(rows: usize, target: RecallTarget) -> usize {
    let wanted = (1.0 - target.fraction()).recip().ceil();
    if wanted.is_finite() && (wanted as usize) < rows { wanted as usize } else { rows }
}

pub(super) fn measure(rows: &Rows, target: RecallTarget) -> CandidateNeed {
    let count = rows.ids.len();
    let dimension = rows.dimension.get();
    let words = super::cpu::words(rows.dimension);
    let probes = probes(count, target);
    // Neighbors are measured as deep as the target's quantile is observed: as many as there are
    // probes (100 at 0.99, past `vector/search`'s default k of 10). A larger k scales the
    // deepest measured need linearly ([`CandidateNeed::candidates`]).
    let depth = probes.min(count.saturating_sub(1));
    if probes == 0 || depth == 0 {
        return CandidateNeed::Unmeasurable;
    }
    // needs[p * depth + (j - 1)] = rows probe p must admit to keep its top-j.
    let mut needs = vec![0usize; probes * depth];
    let mut scored: Vec<Ranked> = Vec::with_capacity(count);
    let mut cumulative = vec![0usize; dimension + 1];
    for (probe, need) in needs.chunks_exact_mut(depth).enumerate() {
        let row = probe * count / probes;
        let query = &rows.values[row * dimension..(row + 1) * dimension];
        let sketch = &rows.sketches[row * words..(row + 1) * words];
        scored.clear();
        cumulative.iter_mut().for_each(|bucket| *bucket = 0);
        let others = rows.values.chunks_exact(dimension).zip(rows.sketches.chunks_exact(words)).enumerate();
        for (other, (values, bits)) in others.filter(|(other, _)| *other != row) {
            scored.push(Ranked { score: dot(query, values), row: other });
            if let Some(bucket) = cumulative.get_mut(hamming(sketch, bits)) {
                *bucket += 1;
            }
        }
        let mut within = 0usize;
        for bucket in cumulative.iter_mut() {
            within += *bucket;
            *bucket = within;
        }
        scored.select_nth_unstable_by(depth - 1, |a, b| b.cmp(a));
        let best = &mut scored[..depth];
        best.sort_unstable_by(|a, b| b.cmp(a));
        let mut deepest = 0usize;
        for (slot, neighbor) in need.iter_mut().zip(best.iter()) {
            let bits = &rows.sketches[neighbor.row * words..(neighbor.row + 1) * words];
            // A Hamming distance never exceeds the dimension: the histogram has a bucket for it.
            deepest = deepest.max(cumulative[hamming(sketch, bits)]);
            *slot = deepest;
        }
    }
    let mut column = vec![0usize; probes];
    // ceil(target * probes) is in 1..=probes for a target in (0, 1].
    let quantile = ((target.fraction() * probes as f32).ceil() as usize).saturating_sub(1).min(probes - 1);
    let by_k = (0..depth)
        .map(|j| {
            for (slot, need) in column.iter_mut().zip(needs.chunks_exact(depth)) {
                *slot = need[j];
            }
            column.sort_unstable();
            // Each need counts at least the neighbor itself, so the quantile is never zero.
            NonZeroUsize::new(column[quantile])
        })
        .collect::<Option<Vec<_>>>();
    match by_k {
        Some(by_k) => CandidateNeed::Measured { by_k },
        // Unreachable by construction (see above); scoring every row is still exact.
        None => CandidateNeed::Unmeasurable,
    }
}
