//! The engine's measured step costs on the recorded M5 generation, FITTED here from the log's
//! own observations (`fixtures/dyno/2026-11-06-m5-batches.json`), each fit with its residual stated:
//!
//! - prefill: one slot's chunk of `t` tokens at depth `d` takes `f0 + f1 d + t (g0 + g1 d)`
//!   seconds, from every clean solo chunk: a per-chunk cost that grows with depth (the log's
//!   3-token checkpoint-tail chunks take 0.4-9 s; a checkpoint copies the slot's state) plus a
//!   per-token cost that grows with depth (attention over the resident context);
//! - decode: one decode step of `n` decoders over summed depth `D` takes `d0 + d1 n + d2 D`
//!   seconds, fitted to the median step of each (decoders, depth) group of decode-only windows
//!   (no prompt work between two `n_gen` prints), weighted by group size, each
//!   step yielding the task's recorded MTP mean accepted length, not one token.
#![cfg(test)]

use super::{Evidence, Rates};
use crate::capacity::dyno::knee::{derive_knee, Knee, NoiseBand, ServiceCurve};

use crate::capacity::decode_width::{least_squares, median_relative_residual, DecodePlane, FIT_DEPTH_BAND};

/// The fitted costs and their residuals.
#[derive(Debug, Clone)]
pub(crate) struct Costs {
    /// Seconds per chunk: `[f0, f1, g0, g1]` in `f0 + f1 d + t (g0 + g1 d)`, `d` and `t` in
    /// thousands of tokens.
    pub prefill: [f64; 4],
    /// Seconds per decode step: `[d0, d1, d2]` in `d0 + d1 n + d2 (D / 1000)`.
    pub decode: [f64; 3],
    /// `None` with no observation to measure the fit against.
    pub prefill_residual: Option<f64>,
    pub decode_residual: Option<f64>,
    /// The recorded per-batch rates by sequences (for the knee of candidate (d)).
    rates: Rates,
}

impl Costs {
    pub(crate) fn of(e: &Evidence) -> Self {
        let chunks = &e.m5_solo_chunks.value;
        let row = |t: u32, d: u32| {
            let (t, d) = (f64::from(t) / 1000.0, f64::from(d) / 1000.0);
            vec![1.0, d, t, t * d]
        };
        let rows: Vec<(Vec<f64>, f64, f64)> = chunks.iter().map(|c| (row(c.1, c.2), c.0, 1.0)).collect();
        let p = least_squares(&rows).expect("evidence: the solo chunks determine the fit");
        let prefill = [p[0], p[1], p[2], p[3]];
        let at = |t: u32, d: u32| row(t, d).iter().zip(prefill).map(|(x, c)| x * c).sum::<f64>();
        let prefill_residual = median_relative_residual(chunks.iter().map(|c| (at(c.1, c.2), c.0)));
        let windows = &e.m5_decode_windows.value;
        let step = |w: &(f64, u32, u32, u64, f64)| w.4 * w.0 / f64::from(w.1);
        // Median per (decoders, 50k summed-depth) group, weighted by its count: a window that
        // straddles an unprinted stall would drag a mean fit (the per-window tail is long).
        let mut groups: std::collections::BTreeMap<(u32, u64), Vec<f64>> = std::collections::BTreeMap::new();
        for w in windows {
            groups.entry((w.2, w.3 / FIT_DEPTH_BAND)).or_default().push(step(w));
        }
        let rows: Vec<(Vec<f64>, f64, f64)> = groups.values().filter(|v| v.len() >= 10).map(|v| {
            let mut v = v.clone();
            v.sort_by(f64::total_cmp);
            (v.len(), v[v.len() / 2])
        }).zip(groups.iter().filter(|(_, v)| v.len() >= 10).map(|(k, _)| *k))
            .map(|((n, median), (decoders, bucket))| (vec![1.0, f64::from(decoders), (bucket * FIT_DEPTH_BAND + FIT_DEPTH_BAND / 2) as f64 / 1000.0], median, n as f64))
            .collect();
        let d = least_squares(&rows).expect("evidence: the decode groups determine a plane");
        let decode = [d[0], d[1], d[2]];
        let decode_residual = median_relative_residual(windows.iter().map(|w| (decode[0] + decode[1] * f64::from(w.2) + decode[2] * w.3 as f64 / 1000.0, step(w))));
        Self { prefill, decode, prefill_residual, decode_residual, rates: e.m5_batch_rates.value.clone() }
    }

    /// Seconds for one slot's chunk of `tokens` starting at `depth`.
    pub(crate) fn prompt_s(&self, tokens: u32, depth: u64) -> f64 {
        let (t, d) = (f64::from(tokens) / 1000.0, depth as f64 / 1000.0);
        (self.prefill[0] + self.prefill[1] * d + t * (self.prefill[2] + self.prefill[3] * d)).max(0.0)
    }

    /// The fitted decode plane, the one the live publisher fits the same way (`decode_width`).
    pub(crate) fn plane(&self) -> DecodePlane {
        DecodePlane { base: self.decode[0], per_decoder: self.decode[1], per_kdepth: self.decode[2] }
    }

    /// Seconds for one decode step of `decoders` over summed depth `depth`.
    pub(crate) fn decode_s(&self, decoders: usize, depth: u64) -> f64 {
        self.plane().step_s(decoders, depth)
    }

    /// The prompt-concurrency knee of the recorded no-decoder batch curve, with the noise band the
    /// recording itself shows: the spread between its one- and two-or-more-decoder readings at the
    /// same sequence count (the closest thing to repeats it has).
    pub(crate) fn knee(&self) -> Knee {
        let one = ServiceCurve::new(self.rates.one_decoder.clone());
        let more = ServiceCurve::new(self.rates.two_or_more_decoders.clone());
        let band = one.points().iter()
            .filter_map(|(n, a)| more.points().iter().find(|(m, _)| m == n).map(|(_, b)| (a - b).abs()))
            .fold(0.0, f64::max);
        derive_knee(&ServiceCurve::new(self.rates.no_decoder.clone()), NoiseBand::Measured(band))
    }
}
