//! How many generating sequences decode per engine step, chosen from the engine's measured step
//! cost (CONTINUUM-ENGINE 4.10 P5 slice 2): the one owner the dyno's replay and the live publisher
//! share.
//!
//! The fork decodes every generating slot on every step unless told a width (`POST
//! /engine/decode-set`, llama.cpp#120). On the M5 (2026-11-16 04:10Z) four streams decoded together
//! at 1.4-4.3 tok/s each against ~60 alone: a step's cost grows with the streams and their summed
//! depth, so past some count every stream waits longer than it gains. The plane below is fitted to
//! measured decode steps (`d0 + d1 n + d2 D/1000` seconds for `n` decoders over summed depth `D`),
//! and the width is the count that minimizes the mean time for the running sequences to finish.
//!
//! What the core does not know is how many tokens each sequence still has to write (a thinking
//! model decides). Unknown is not a number: with equal remaining work the remaining length scales
//! every candidate's mean finish alike, so the argmin depends only on the plane and the depths.
//! The engine chooses WHICH sequences fill the width (the longest waiting, so none starves).

use std::num::NonZeroUsize;

pub mod live;

/// The summed-depth resolution measured steps are grouped at before the plane is fitted: one group
/// per (decoders, band). A grouping of evidence, never a decision: the plane is continuous in depth.
/// The dyno fits the recorded hour at the same resolution, so its plane and the live one agree.
pub const FIT_DEPTH_BAND: u64 = 50_000;

/// Seconds per decode step: `base + per_decoder n + per_kdepth (D / 1000)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodePlane {
    pub base: f64,
    pub per_decoder: f64,
    pub per_kdepth: f64,
}

/// One measured decode step: how many sequences decoded, their summed depth, and the seconds the
/// step took per token each sequence gained (MTP's accepted drafts count as gained tokens).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodeStep {
    pub decoders: usize,
    pub summed_depth: u64,
    pub seconds_per_token: f64,
}

/// A plane fitted to measured steps, with the median relative residual of the fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    pub plane: DecodePlane,
    pub residual: f64,
}

impl DecodePlane {
    /// Seconds for one decode step of `decoders` over summed depth `depth`.
    pub fn step_s(&self, decoders: usize, depth: u64) -> f64 {
        match decoders {
            0 => 0.0,
            n => (self.base + self.per_decoder * n as f64 + self.per_kdepth * depth as f64 / 1000.0).max(0.0),
        }
    }

    /// The plane through `steps` (least squares; each step weighted by `weight`). `None` when the
    /// steps do not determine a plane (too few, or no spread in decoders or depth).
    pub fn fit(steps: &[(DecodeStep, f64)]) -> Option<Fitted> {
        let rows: Vec<(Vec<f64>, f64, f64)> = steps
            .iter()
            .map(|(s, w)| (vec![1.0, s.decoders as f64, s.summed_depth as f64 / 1000.0], s.seconds_per_token, *w))
            .collect();
        let c = least_squares(&rows)?;
        let plane = DecodePlane { base: c[0], per_decoder: c[1], per_kdepth: c[2] };
        let residual = median_relative_residual(steps.iter().map(|(s, _)| (plane.step_s(s.decoders, s.summed_depth), s.seconds_per_token)))?;
        Some(Fitted { plane, residual })
    }

    /// The width that minimizes the mean finish of sequences at `depths` with equal remaining work.
    /// `None` when nothing is generating.
    pub fn width(&self, depths: &[u64]) -> Option<NonZeroUsize> {
        let equal = vec![1.0; depths.len()];
        let n = NonZeroUsize::new(depths.len())?;
        (1..=n.get()).filter_map(NonZeroUsize::new).min_by(|a, b| {
            self.mean_finish(&equal, depths, *a).total_cmp(&self.mean_finish(&equal, depths, *b))
        })
    }

    /// Mean finishing time of sequences with `left` tokens to go at `depths` (in finishing order,
    /// shortest first) when at most `width` decode per step: each step every active sequence advances
    /// one token at the plane's step time for that many sequences and their summed depth; a finished
    /// one hands its place to the next. `left` is never empty: every caller asks about the sequences
    /// generating now (`width` returns before asking when none are).
    pub fn mean_finish(&self, left: &[f64], depths: &[u64], width: NonZeroUsize) -> f64 {
        let mut left = left.to_vec();
        let (mut now, mut finished) = (0.0, 0.0);
        for done in 0..left.len() {
            // finished together with an earlier one (equal work): no further step is spent on it
            if left[done] <= 0.0 {
                finished += now;
                continue;
            }
            let active = done..left.len().min(done + width.get());
            let summed: u64 = depths[active.clone()].iter().sum();
            let steps = left[done].max(1.0);
            now += steps * self.step_s(active.len(), summed);
            for i in active {
                left[i] -= steps;
            }
            finished += now;
        }
        finished / left.len() as f64
    }
}

/// Least squares by the normal equations (at most a handful of unknowns, a weight per row). `None`
/// when the rows do not determine the unknowns.
pub(crate) fn least_squares(rows: &[(Vec<f64>, f64, f64)]) -> Option<Vec<f64>> {
    let k = rows.first()?.0.len();
    let mut m = vec![vec![0.0; k + 1]; k];
    for (x, y, w) in rows {
        for i in 0..k {
            for j in 0..k {
                m[i][j] += w * x[i] * x[j];
            }
            m[i][k] += w * x[i] * y;
        }
    }
    for i in 0..k {
        let p = (i..k).max_by(|a, b| m[*a][i].abs().total_cmp(&m[*b][i].abs()))?;
        m.swap(i, p);
        if m[i][i].abs() < 1e-12 {
            return None;
        }
        for r in 0..k {
            if r != i {
                let f = m[r][i] / m[i][i];
                for c in i..=k {
                    m[r][c] -= f * m[i][c];
                }
            }
        }
    }
    Some((0..k).map(|i| m[i][k] / m[i][i]).collect())
}

/// The median of |predicted - observed| / observed over the positive observations; `None` with none.
pub(crate) fn median_relative_residual(pairs: impl Iterator<Item = (f64, f64)>) -> Option<f64> {
    let mut r: Vec<f64> = pairs.filter(|(_, o)| *o > 0.0).map(|(p, o)| (p - o).abs() / o).collect();
    r.sort_by(f64::total_cmp);
    r.get(r.len() / 2).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a fit that does not recover the plane its steps were drawn from, or a
    // width rule that ignores the cost of depth: a plane where depth dominates chooses one decoder
    // at long contexts, and one where each step's fixed cost dominates batches everyone.
    #[test]
    fn the_fit_recovers_its_plane_and_the_width_follows_what_a_step_costs() {
        let truth = DecodePlane { base: 0.010, per_decoder: 0.004, per_kdepth: 0.0006 };
        let steps: Vec<(DecodeStep, f64)> = (1..=4usize)
            .flat_map(|n| [10_000u64, 60_000, 150_000].map(move |d| (n, d)))
            .map(|(n, d)| (DecodeStep { decoders: n, summed_depth: d, seconds_per_token: truth.step_s(n, d) }, 1.0))
            .collect();
        let fitted = DecodePlane::fit(&steps).expect("test: twelve spread steps determine the plane"); // test: spread by construction
        assert!((fitted.plane.per_kdepth - truth.per_kdepth).abs() < 1e-9 && fitted.residual < 1e-6, "{fitted:?}");

        let deep = [150_000u64, 100_000, 60_000, 1_200];
        let depth_bound = DecodePlane { base: 0.005, per_decoder: 0.001, per_kdepth: 0.002 };
        assert_eq!(depth_bound.width(&deep).map(NonZeroUsize::get), Some(1), "depth dominates: one at a time");
        let step_bound = DecodePlane { base: 0.050, per_decoder: 0.001, per_kdepth: 0.00001 };
        assert_eq!(step_bound.width(&deep).map(NonZeroUsize::get), Some(4), "the fixed step dominates: batch everyone");
        assert_eq!(step_bound.width(&[]), None, "nothing generating, nothing to choose");
    }
}
