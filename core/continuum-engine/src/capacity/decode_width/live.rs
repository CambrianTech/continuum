//! The live decode width (CONTINUUM-ENGINE 4.10 P5 slice 2): the engine's clean decode intervals,
//! as the prefill knee already reads them from `/slots` (`prefill_knee::clean_decode`), fitted to the
//! decode plane per model, and the width the plane chooses at the depths generating now.
//!
//! Each clean interval is one measured step shape: how many streams decoded, their summed depth,
//! and the seconds per token each gained (1 / its tok/s; MTP's accepted drafts are gained tokens).
//! The fit keeps only its normal equations, so what is held per model does not grow with the
//! evidence, and nothing is grouped by a chosen size. A plane is only claimed once the evidence
//! spans more than one stream count and more than one depth (otherwise it determines no slope); until
//! then no width is published and the engine decodes every generating slot, as it does today.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::LazyLock;

use parking_lot::Mutex;

use super::{DecodePlane, DecodeStep, NormalEquations};

/// One model's evidence: the fit's normal equations and the span it covers.
#[derive(Debug, Clone)]
struct Evidence {
    equations: NormalEquations,
    decoders: (usize, usize),
    depth: (u64, u64),
}

impl Evidence {
    fn of(step: &DecodeStep) -> Self {
        Self { equations: NormalEquations::new(3), decoders: (step.decoders, step.decoders), depth: (step.summed_depth, step.summed_depth) }
    }

    fn add(&mut self, step: &DecodeStep) {
        self.equations.add(&DecodePlane::row(step.decoders, step.summed_depth), step.seconds_per_token, 1.0);
        self.decoders = (self.decoders.0.min(step.decoders), self.decoders.1.max(step.decoders));
        self.depth = (self.depth.0.min(step.summed_depth), self.depth.1.max(step.summed_depth));
    }

    /// The plane, once the evidence spans more than one stream count and more than one depth.
    fn plane(&self) -> Option<DecodePlane> {
        if self.decoders.0 == self.decoders.1 || self.depth.0 == self.depth.1 {
            return None;
        }
        let c = self.equations.solve()?;
        Some(DecodePlane { base: c[0], per_decoder: c[1], per_kdepth: c[2] })
    }
}

/// Per model, the measured evidence.
static OBSERVED: LazyLock<Mutex<BTreeMap<String, Evidence>>> = LazyLock::new(Default::default);

/// The width last told to each engine process (`None`: every generating slot, the engine's own
/// default, so an engine never told, a new process included, needs no publication to be there).
static PUBLISHED: LazyLock<Mutex<BTreeMap<u32, Option<NonZeroUsize>>>> = LazyLock::new(Default::default);

/// Whether `width` differs from what engine process `pid` was last told; marks it told. An engine is
/// told only on a change (one publication per decision, never per read).
pub fn tell(pid: u32, width: Option<NonZeroUsize>) -> bool {
    let mut published = PUBLISHED.lock();
    let told = published.get(&pid).copied().flatten();
    published.insert(pid, width);
    told != width
}

/// Fold one measured interval of `model` into its evidence.
pub fn observe(model: &str, step: DecodeStep) {
    if step.decoders == 0 || !step.seconds_per_token.is_finite() || step.seconds_per_token <= 0.0 {
        return;
    }
    OBSERVED.lock().entry(model.to_owned()).or_insert_with(|| Evidence::of(&step)).add(&step);
}

/// The plane `model`'s evidence determines, `None` while it does not determine one.
pub fn plane(model: &str) -> Option<DecodePlane> {
    OBSERVED.lock().get(model)?.plane()
}

/// The width for `model` with sequences generating at `depths`; `None` with no plane yet or nothing
/// generating (the engine then decodes every generating slot).
pub fn width(model: &str, depths: &[u64]) -> Option<NonZeroUsize> {
    plane(model)?.width(depths)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a live width published from too little evidence, or one that does not
    // follow the measured steps. One stream count determines no slope; spread steps from a
    // depth-bound engine recover its plane and choose one decoder at long contexts.
    #[test]
    fn a_width_follows_only_from_measured_steps_that_determine_a_plane() {
        let model = "test: depth-bound";
        let truth = DecodePlane { base: 0.005, per_decoder: 0.001, per_kdepth: 0.002 };
        let step = |n: usize, d: u64| DecodeStep { decoders: n, summed_depth: d, seconds_per_token: truth.step_s(n, d) };
        observe(model, step(2, 60_000));
        observe(model, step(2, 160_000));
        assert_eq!(width(model, &[150_000, 100_000]), None, "one stream count determines no slope");
        for (n, d) in [(1usize, 10_000u64), (3, 160_000), (4, 260_000), (1, 120_000)] {
            observe(model, step(n, d));
        }
        let fitted = plane(model).expect("test: spread steps determine the plane"); // test: three stream counts, several depths
        assert!((fitted.per_kdepth - truth.per_kdepth).abs() < 1e-9, "{fitted:?}");
        assert_eq!(width(model, &[150_000, 100_000, 60_000, 1_200]).map(NonZeroUsize::get), Some(1));
        assert_eq!(width("test: never measured", &[1_000, 2_000]), None);
    }
}
