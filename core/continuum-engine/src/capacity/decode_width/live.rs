//! The live decode width (CONTINUUM-ENGINE 4.10 P5 slice 2): the engine's clean decode intervals,
//! as the prefill knee already reads them from `/slots` (`prefill_knee::clean_decode`), fitted to the
//! decode plane per model, and the width the plane chooses at the depths generating now.
//!
//! Each clean interval is one measured step shape: how many streams decoded, their summed depth,
//! and the seconds per token each gained (1 / its tok/s; MTP's accepted drafts are gained tokens).
//! The steps are kept as a running mean per (decoders, depth band) group, the grouping the dyno fits
//! the recorded hour with, so what is held is bounded by the shapes seen, never by a count. A model
//! with too few distinct shapes has no plane yet, and no width is published: the engine decodes every
//! generating slot, as it does today.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::LazyLock;

use parking_lot::Mutex;

use super::{DecodePlane, DecodeStep, FIT_DEPTH_BAND};

/// One (decoders, depth band) group's running mean of seconds per token, and how many intervals
/// made it (the fit's weight).
#[derive(Debug, Clone, Copy, Default)]
struct Group {
    mean: f64,
    count: u64,
}

/// Per model, the measured step shapes.
static OBSERVED: LazyLock<Mutex<BTreeMap<String, BTreeMap<(usize, u64), Group>>>> = LazyLock::new(Default::default);

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

/// Fold one measured interval of `model` into its shapes.
pub fn observe(model: &str, step: DecodeStep) {
    if step.decoders == 0 || !step.seconds_per_token.is_finite() || step.seconds_per_token <= 0.0 {
        return;
    }
    let mut observed = OBSERVED.lock();
    let group = observed
        .entry(model.to_owned())
        .or_default()
        .entry((step.decoders, step.summed_depth / FIT_DEPTH_BAND))
        .or_default();
    group.count += 1;
    group.mean += (step.seconds_per_token - group.mean) / group.count as f64;
}

/// The plane `model`'s measured shapes determine, `None` while they do not determine one.
pub fn plane(model: &str) -> Option<DecodePlane> {
    let steps: Vec<(DecodeStep, f64)> = OBSERVED
        .lock()
        .get(model)?
        .iter()
        .map(|((decoders, band), group)| {
            let step = DecodeStep { decoders: *decoders, summed_depth: band * FIT_DEPTH_BAND + FIT_DEPTH_BAND / 2, seconds_per_token: group.mean };
            (step, group.count as f64)
        })
        .collect();
    DecodePlane::fit(&steps).map(|fitted| fitted.plane)
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
    // follow the measured shapes. One shape determines nothing; spread shapes from a depth-bound
    // engine choose one decoder at long contexts.
    #[test]
    fn a_width_follows_only_from_measured_shapes_that_determine_a_plane() {
        let model = "test: depth-bound";
        let truth = DecodePlane { base: 0.005, per_decoder: 0.001, per_kdepth: 0.002 };
        observe(model, DecodeStep { decoders: 2, summed_depth: 60_000, seconds_per_token: truth.step_s(2, 75_000) });
        assert_eq!(width(model, &[150_000, 100_000]), None, "one shape determines no plane");
        for (n, d) in [(1usize, 10_000u64), (3, 160_000), (4, 260_000), (1, 120_000)] {
            let mid = d / FIT_DEPTH_BAND * FIT_DEPTH_BAND + FIT_DEPTH_BAND / 2;
            observe(model, DecodeStep { decoders: n, summed_depth: d, seconds_per_token: truth.step_s(n, mid) });
        }
        assert_eq!(width(model, &[150_000, 100_000, 60_000, 1_200]).map(NonZeroUsize::get), Some(1));
        assert_eq!(width("test: never measured", &[1_000, 2_000]), None);
    }
}
