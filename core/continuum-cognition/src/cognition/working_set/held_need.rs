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
//! grant that censored it. Read as a bound, it is the honest measurement, and only that: an
//! overflow is first reducible (her view collapses older items to handles), so residency is
//! sized from what held ([`resident_need`]), never from a bound (ONE-ACT stage 3; the M5,
//! 2026-11-04 20:15Z: a bound handed one mind the pool, {Aiko: 114,592} of 125,696).

use super::need::product_limit;
use super::{PersonaDemand, RECENT_TURNS};

/// One turn's observation of her need, in window tokens, on the one censoring type below every
/// reader ([`crate::inference::planning::window_allocator::observed::Observed`]): `Exact` when the fit held the turn
/// at this size (`0` is an empty slot of the ring, as in every ring of [`PersonaDemand`]),
/// `AtLeast` when her view overflowed its grant (the window she was served plus what her view
/// lacked to hold this act and the next one's measured appends). Persisted untagged, so a slot
/// written before the bound existed reads exact.
pub type NeedSample = crate::inference::planning::window_allocator::observed::Observed<u32>;

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

/// What her view held or would have held, as measured: the larger of two reads of her ring, raised
/// to her newest turn's irreducible window when that is larger (a turn that cannot be sent below
/// it is a need, not a wish).
/// - Her typical turn: the median, the product-limit estimate over the exact samples with the
///   bounds as censored ones ([`product_limit`]), so a bound can raise it and never lower it.
/// - Her largest live bound: a bound is a PROOF that her working set exceeded that size within
///   the ring's turns, and the question a need answers is what her view must hold so it does not
///   turn over, not what a typical turn held. Read as a median alone, two overflows among fourteen
///   older exact samples changed nothing (Aris on the M5, ff7d318aa, 2026-11-04 17:47-18:16Z:
///   bounds of 75,911 and 71,852 recorded, her grant moved 55,672 -> 58,522, and she still turned
///   over on 3 of 4 acts).
///
/// A bound ages out of the 16-slot ring, so the need can fall again once her epochs stop
/// overflowing. `AtLeast` when a bound decided it. `None` until anything is measured.
///
/// This is the MEASUREMENT. It never authorizes residency: a reader that sizes a grant reads
/// [`resident_need`] (ONE-ACT stage 3). Its consumer is admission's typed growth decision (ONE-ACT
/// stage 4, Codex): only irreducible growth may request residency, never an overflow the view can
/// collapse.
pub fn need_tokens(d: &PersonaDemand) -> Option<NeedSample> {
    let (exact, bounds) = split_ring(d);
    let bounded = held_median(&exact, &bounds).max(bounds.iter().copied().max());
    let held = match (held_median(&exact, &[]), bounded) {
        (Some(measured), Some(estimate)) if measured >= estimate => Some(NeedSample::Exact(measured)),
        (_, estimate) => estimate.map(|at_least| NeedSample::AtLeast { at_least }),
    };
    match (held, d.floor_tokens) {
        (Some(held), Some(floor)) if floor > held.value() => Some(NeedSample::Exact(floor)),
        (Some(held), _) => Some(held),
        (None, floor) => floor.map(NeedSample::Exact),
    }
}

/// What her residency is sized from: the median of the turns the fit HELD, raised to her newest
/// turn's irreducible window. A censored bound is excluded (ONE-ACT stage 3): an overflow is first
/// REDUCIBLE, because her view collapses older items to handles, so a bound proves only that her
/// HISTORY outgrew the grant, never that her current view needs more. Read as residency, the
/// bound let one mind take the pool (the M5, 2026-11-04 20:15-20:35Z: pool 125,696 -> {Aiko
/// 114,592}, three residents dark 20 min). Irreducible growth is the floor, which stays exact.
/// `None` until a held turn or a floor is measured; the allocator then gates on the wish.
pub fn resident_need(d: &PersonaDemand) -> Option<u32> {
    let (exact, _) = split_ring(d);
    match (held_median(&exact, &[]), d.floor_tokens) {
        (Some(held), Some(floor)) => Some(held.max(floor)),
        (held, floor) => held.or(floor),
    }
}

/// Her need ring's observed slots, split into exact sizes and censored bounds.
fn split_ring(d: &PersonaDemand) -> (Vec<u32>, Vec<u32>) {
    let observed = d.need_recent.iter().copied().filter(|s| s.value() > 0);
    let (bounds, exact): (Vec<NeedSample>, Vec<NeedSample>) = observed.partition(NeedSample::is_bound);
    (exact.into_iter().map(NeedSample::value).collect(), bounds.into_iter().map(NeedSample::value).collect())
}

/// The upper median, as `median_of` reads a ring: the share of turns past it falls below half.
fn held_median(exact: &[u32], bounds: &[u32]) -> Option<u32> {
    product_limit(exact, bounds, |survival_num, survival_den| survival_num * 2 < survival_den)
}
