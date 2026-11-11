//! Her held NEED: the window her act's allocation CHOSE (`continuum_context::allocation::need`,
//! the priced optimum, never past the model's trained context), observed per held turn either
//! EXACTLY (the optimum lay inside her grant) or AT LEAST (her grant censored it: the optimum lay
//! past the window she was served). What her view lacked uncollapsed is never a need: it is
//! collapsed (levels, navigation pointers).
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
/// reader ([`crate::inference::planning::window_allocator::observed::Observed`]): `Exact` when her
/// act's priced optimum lay inside her grant (`0` is an empty slot of the ring, as in every ring of
/// [`PersonaDemand`]), `AtLeast` when it lay past the window she was served. Persisted untagged, so
/// a slot written before the bound existed reads exact.
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

/// What her residency is sized from: her need as [`need_tokens`] reads it (the product-limit median
/// of her samples, a censored one, her act's priced window past her grant, counted as at least its
/// bound; raised to her newest turn's irreducible window), granted as its value.
/// Dropping censored samples left a mind whose own price chose more than her grant pinned at that
/// grant forever, with 87% of the pool idle (the M5, 2026-11-09: Benchy's censored needs of
/// 27-44k against a ~26k grant; every move down needs a way back up). The pool's contention is
/// the allocator's, which shares what remains in proportion to what each mind still needs, so no
/// mind's need leaves another dark (`capacity::window_grant`). `None` until a sample or a floor is
/// measured; the allocator then gates on the wish.
pub fn resident_need(d: &PersonaDemand) -> Option<crate::inference::planning::window_allocator::observed::Observed<crate::inference::planning::window_allocator::Tokens>> {
    use crate::inference::planning::window_allocator::Tokens;
    // A bound stays a bound over her floor (`Observed::over`): admission reads it through
    // `Observed::within`, which collapses the bound into what exists and never the floor. Read as
    // exact, a bound past the ceiling was never granted, never held and never measured again: every
    // act parked for good (BigMama, Sahar on dc01d9563, 2026-11-11 09:19-11:59Z: 'NeedAboveCeiling
    // 116240/58112' at fit 29,056; Codex traced it here, and found the floor case on #5931).
    need_tokens(d)?.try_map(Tokens::new).map(|need| need.over(d.floor_tokens.and_then(Tokens::new)))
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
