//! Her held NEED: the window a turn must have, observed per turn either EXACTLY (the fit held
//! the turn, and this is the budgeter's floor it was served at) or AT LEAST (the window censored
//! it: her view overflowed what the window granted it and turned over, so what she would have
//! held is known only as a lower bound).
//!
//! The censoring is a type, never a convention a reader must remember: a need measured from
//! prompts the grant itself bounded can never exceed the grant (Aris on the M5, 2026-11-04
//! 16:55-17:16Z: granted 55,672 of a 654,336-token pool with 474k free, her view 26-37k tokens
//! appending 5-12k an act, so `delib.view.turnover` fired on most acts, each start collapsed what
//! she had read and she re-read the same files). Read as a size, a censored sample freezes the
//! grant that censored it; read as a bound, it lifts her need until her epoch fits.

use serde::{Deserialize, Serialize};

use super::need::product_limit;
use super::{PersonaDemand, RECENT_TURNS};

/// One turn's observation of her need, in window tokens.
///
/// Persisted untagged: an exact sample is the bare number every record written before the bound
/// existed holds, so those records read unchanged; a bound is `{"at_least": n}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NeedSample {
    /// The fit held the turn at this size: a measurement. `0` is an empty slot of the ring, as
    /// in every ring of [`PersonaDemand`].
    Exact(u32),
    /// Her view overflowed its grant: the window she was served plus what her view lacked to hold
    /// this act and the next one's measured appends. A lower bound, never a size.
    AtLeast { at_least: u32 },
}

impl Default for NeedSample {
    /// An empty slot of the ring.
    fn default() -> Self {
        Self::Exact(0)
    }
}

impl NeedSample {
    /// The tokens observed: the size, or the bound.
    pub fn tokens(self) -> u32 {
        match self {
            Self::Exact(tokens) | Self::AtLeast { at_least: tokens } => tokens,
        }
    }

    /// Whether this is a lower bound the window censored, never a size.
    pub fn is_bound(self) -> bool {
        matches!(self, Self::AtLeast { .. })
    }
}

/// Push one turn's need observation into her need ring (newest overwrites oldest).
pub(super) fn push_need(d: &mut PersonaDemand, sample: NeedSample) {
    let i = (d.need_next as usize) % RECENT_TURNS;
    d.need_recent[i] = sample;
    d.need_next = ((i + 1) % RECENT_TURNS) as u8;
}

/// The size a HELD turn contributes to her need ring — `None` when the turn was not
/// held, so the ring never learns a starved window's echo (the 5090's self-sealed
/// 2048, 2026-09-20). A turn is held when the fit kept the source's truthful minimum
/// (`fit_ok`) AND something survived it (`sent > framing`): a fit that dropped every
/// message and "succeeded" is the window measuring itself, not the turn. Pure.
pub fn need_sample(sent_tokens: u32, framing_tokens: u32, fit_ok: bool) -> Option<u32> {
    (fit_ok && sent_tokens > framing_tokens).then_some(sent_tokens)
}

/// What a lane must hold for her: the median of her recent need observations, raised to her
/// newest turn's irreducible window when that is larger (a turn that cannot be sent below it is a
/// need, not a wish). The median is the product-limit estimate over the exact samples with the
/// bounds as censored ones ([`product_limit`]): a bound moves its weight only to larger sizes, so
/// it can raise the median and never lower it, and a newer, smaller bound never undoes a larger
/// one while that one is in the ring (it ages out with the ring, so the need can fall again).
/// `AtLeast` when the bounds raised it past what the exact samples alone say, so a reader sizes
/// her target from it too. `None` until anything is measured; the allocator then gates on the wish.
pub fn need_tokens(d: &PersonaDemand) -> Option<NeedSample> {
    let observed = || d.need_recent.iter().copied().filter(|s| s.tokens() > 0);
    let exact: Vec<u32> = observed().filter(|s| !s.is_bound()).map(NeedSample::tokens).collect();
    let bounds: Vec<u32> = observed().filter(|s| s.is_bound()).map(NeedSample::tokens).collect();
    // The upper median, as `median_of` reads a ring: the share of turns past it falls below half.
    let median =
        |bounds: &[u32]| product_limit(&exact, bounds, |survival_num, survival_den| survival_num * 2 < survival_den);
    let held = match (median(&[]), median(&bounds)) {
        (Some(measured), Some(estimate)) if measured >= estimate => Some(NeedSample::Exact(measured)),
        (_, estimate) => estimate.map(|at_least| NeedSample::AtLeast { at_least }),
    };
    match (held, d.floor_tokens) {
        (Some(held), Some(floor)) if floor > held.tokens() => Some(NeedSample::Exact(floor)),
        (Some(held), _) => Some(held),
        (None, floor) => floor.map(NeedSample::Exact),
    }
}
