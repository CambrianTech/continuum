//! Context has a price (card 7b7a696a): every token she holds slows every act (her measured
//! [`ActTime`]), so the allocation holds the window where her act's value per second peaks, never
//! simply the whole window it was served. Joel, 2026-11-07: the allocation is "an optimizer too. It
//! sort of searches to fit"; "constants never"; "a mind gets what she needs, never a division".
//!
//! What is maximized: `R(W) = (V(W) - V(least)) / T(W)`, the value her context above her least plan
//! adds per second of her act at a held window `W`: `V` is the search's own value
//! (`Σ priority x ln(1 + tokens shown)`, [`super::search`]) and `T(W)` her measured seconds per act
//! holding `W` less her reply's grant ([`ActTime::seconds`]).
//!
//! The exchange rate between value and seconds is no constant: at the peak `dV/dW = R x dT/dW`, so
//! the rate is `R` itself, her optional context's value per second at the plan she holds
//! (Dinkelbach's form: the peak of `V/T` is the plan maximizing `V - R x T` at that `R`); the one
//! measured quantity it comes from is her act time. Her least plan's value stands in every plan the
//! price can choose and buys nothing it trades, so the rate counts the value above it. Holding `W`
//! instead of a smaller window is a lower price on every token: allocating at the peak's window
//! reproduces the priced optimum up to the levels' steps, so every reader downstream (her view's
//! start, its room, the depth of a turnover) sees one consistent window.
//!
//! What a price choice never does:
//! - extrapolate: it chooses only windows whose held context lies inside the contexts her act time
//!   was measured over ([`ActTime::measured`]); where none does the price says nothing
//!   ([`Priced::OutsideMeasured`]) and the window is planned as before (the dyno on #5822: four in
//!   five priced windows fell below the smallest context ever measured, where the curve is a
//!   clamp, not a measurement);
//! - show what she acts through below its summary ([`Consumer::Actionable`]: her tools, her held
//!   card whole); a pointer stays reachable only through the hard-limit path, a window too small;
//! - price out what she must hold ([`least`]): every hard limit, her held head levels (a head
//!   changed mid-epoch re-prefills her whole prompt), her newest result WHOLE (goal 7), her
//!   reply's measured need, and what her live view already sent (resident: it costs no prefill). A window no larger than that is the window as before
//!   ([`Priced::WindowBinds`]); an unmeasured act time prices nothing ([`Priced::Unmeasured`]).
//!
//! How it searches: ternary on the held window over the measured part of `[least, window]`, to the
//! token, each probe one plan ([`super::plan_at`]: microseconds; ~2 ms an act). The served window
//! is a candidate too wherever it is measured, so the answer is never below the window as before
//! in value per second there.
//!
//! The same search names her need ([`super::need`]): the window it chose. When the choice is the
//! top of the served window, the peak may lie past it, so the search continues above the window up
//! to the model's trained context (inside the measured contexts): a peak found there is her need,
//! censored by her grant. A window no larger than what she must hold is censored at that least.

use super::{hard_limits, id, plan_at, ActClaims, ActNeed, ActPlan, Consumer, Head, TrainedContext};
use crate::price::{ActTime, ContextPrice};

/// What the time price of context decided for one act.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Priced {
    /// No measured act time: no cost term, the window filled as before (said in her receipt).
    #[default]
    Unmeasured,
    /// What she must hold (every hard limit, her newest result whole, her reply's need, what she
    /// acts through at its summary) already fills the window: nothing to price.
    WindowBinds,
    /// No window the price could hold lies inside the contexts her act time was measured over
    /// (`measured`, prompt tokens): no extrapolation, the window planned as before.
    OutsideMeasured { measured: (u32, u32) },
    /// The window her act holds, where her value per second peaks inside the measured contexts.
    Held {
        /// The window the plan shares (at most the served one).
        held: u32,
        /// Her predicted seconds per act holding it.
        seconds: f64,
        /// Her optional context's value per second at it: the exchange rate.
        value_per_second: f64,
        /// Seconds and value per second holding the whole served window, when that window is
        /// inside the measured contexts.
        at_window: Option<(f64, f64)>,
        measured: (u32, u32),
    },
}

/// `claims` as a price choice may show them: each actionable head no epoch holds without its
/// pointers ([`Consumer::choosable`]), each actionable view source whole and required. A held
/// head keeps its held levels: they change only at its epoch's start.
fn choosable(claims: &ActClaims) -> ActClaims {
    let head = |head: &Head| match head.held {
        Some(_) => head.clone(),
        None => Head { leveled: head.consumer.choosable(&head.leveled), ..head.clone() },
    };
    let mut priced = claims.clone();
    priced.framing = head(&claims.framing);
    priced.tools = head(&claims.tools);
    for view in priced.view.iter_mut().filter(|view| view.consumer == Consumer::Actionable) {
        view.claim.floor_tokens = view.claim.max_tokens.max(view.claim.floor_tokens);
        view.claim.min_tokens = view.claim.floor_tokens;
        view.claim.required = true;
    }
    priced
}

/// The least window the price may hold: every hard limit (of the choosable claims) with her held
/// head levels in place of her head's hard limits, her newest result whole, and her reply's
/// measured need.
fn least(claims: &ActClaims) -> u32 {
    let held_above = |head: &Head| head.holding().map_or(0, |held| held.cost.saturating_sub(head.leveled.hard_limit())); // map_or: a head no epoch holds adds nothing above its hard limit
    // Her live view's sent content (each source's flex basis above its floor): computed once and
    // resident, so it adds no prefill to her act; a price that held less turned her epoch over and
    // re-prefilled all of it (the M5, 2026-11-10: views granted 7.5k-13.5k of 41k-49k windows, an
    // epoch every second act, her act time measured on those re-prefills priced the window down).
    let view_held = view_sent(claims);
    hard_limits(claims)
        .saturating_add(held_above(&claims.framing))
        .saturating_add(held_above(&claims.tools))
        .saturating_add(claims.newest.whole())
        .saturating_add(claims.reply.floor_tokens)
        .saturating_add(view_held)
}

/// `claims` with her view's held basis released to its floors: what a turnover would start from.
fn released(claims: &ActClaims) -> ActClaims {
    let mut released = claims.clone();
    for view in &mut released.view {
        view.claim.min_tokens = view.claim.floor_tokens;
    }
    released
}

/// What her live view already sent above its floors (each source's flex basis): resident in the
/// engine.
fn view_sent(claims: &ActClaims) -> u32 {
    claims
        .view
        .iter()
        .map(|view| {
            let claim = &view.claim;
            let counted = if claim.required { claim.floor_tokens } else { 0 };
            if claim.min_tokens > claim.floor_tokens { claim.min_tokens.saturating_sub(counted) } else { 0 }
        })
        .fold(0u32, u32::saturating_add)
}

/// The context `plan` holds: its window less her reply's grant.
fn held_context(plan: &ActPlan) -> u32 {
    plan.window.saturating_sub(plan.granted(id::REPLY))
}

/// The value her context above her least plan adds per second of her act, for `plan`: `None` when
/// it has no search answer, its held context lies outside the measured contexts, or its predicted
/// seconds are not positive.
fn rate(plan: &ActPlan, least_value: f64, time: &ActTime) -> Option<f64> {
    let value = plan.searched.as_ref()?.value;
    let (low, high) = time.measured();
    let held = held_context(plan);
    if held < low || held > high {
        return None;
    }
    let seconds = time.seconds(f64::from(held));
    (seconds > 0.0).then(|| (value - least_value) / seconds)
}

/// The window in `[low, high]` where `probe`'s rate peaks, by ternary search to within two tokens
/// and then every window left: its rate and plan (`NEG_INFINITY` and `None` when no window there
/// has a rate).
fn peak(low: u32, high: u32, probe: impl Fn(u32) -> (f64, ActPlan)) -> (f64, Option<ActPlan>) {
    let (mut low, mut high) = (low, high);
    while high - low > 2 {
        let third = (high - low) / 3;
        let (m1, m2) = (low + third, high - third);
        if probe(m1).0 < probe(m2).0 {
            low = m1;
        } else {
            high = m2;
        }
    }
    let mut best = (f64::NEG_INFINITY, None);
    for (rate, candidate) in (low..=high).map(&probe) {
        if rate > best.0 {
            best = (rate, Some(candidate));
        }
    }
    best
}

/// The plan of one act: at the window where her value per second peaks within `window`, with her
/// need the window that peak chose ([`ActNeed`]).
pub(super) fn hold(window: u32, claims: ActClaims) -> ActPlan {
    let ContextPrice::Measured(time) = claims.price else {
        return plan_at(window, claims);
    };
    let trained = claims.trained;
    let priced_claims = choosable(&claims);
    let floor = least(&priced_claims);
    if floor >= window {
        // What she must hold fills her grant: her need is at least it.
        let need = ActNeed::at(floor, true, trained).grown(view_sent(&claims), trained);
        return ActPlan { priced: Priced::WindowBinds, need, ..plan_at(window, claims) };
    }
    let measured = time.measured();
    let served = plan_at(window, claims.clone());
    // The windows whose held context can lie inside the measured contexts: at least the smallest
    // measured context, at most the largest plus her reply's whole want.
    let lo = floor.max(measured.0);
    let hi = window.min(measured.1.saturating_add(claims.reply.max_tokens));
    let outside = ActPlan { priced: Priced::OutsideMeasured { measured }, ..served.clone() };
    if lo > hi {
        // What her view already sent lies past every context measured: holding it cannot be
        // priced, and serving the whole window let it grow to the ceiling (the M5, 2026-11-11). Her
        // view's held basis is flex, never a floor: release it, and the act is priced inside the
        // measured contexts; her view turns over into it, oldest first.
        let released = released(&claims);
        if least(&choosable(&released)) < floor {
            return hold(window, released);
        }
        return outside;
    }
    let Some(least_value) = plan_at(floor, priced_claims.clone()).searched.map(|found| found.value) else {
        return outside;
    };
    let probe = |w: u32| {
        let plan = plan_at(w, priced_claims.clone());
        (rate(&plan, least_value, &time).unwrap_or(f64::NEG_INFINITY), plan) // unwrap_or: a window with no rate (outside the measured contexts) is never the peak
    };
    // The served window is a candidate too wherever it is measured: the answer is then never
    // below the window as before.
    let at_window = rate(&served, least_value, &time);
    let (mut best, mut plan) = (at_window.unwrap_or(f64::NEG_INFINITY), served.clone()); // unwrap_or: an unmeasured served window competes with nothing
    if let (rate, Some(candidate)) = peak(lo, hi, probe) {
        if rate > best {
            (best, plan) = (rate, candidate);
        }
    }
    if !best.is_finite() {
        return outside;
    }
    // Her need carries her view's growth: a reserve as large as what it already sent, as her
    // view's start leaves one, so her next acts append into a granted window instead of turning
    // her epoch over (the M5, 2026-11-10: Aris granted ~45k with her view at its edge, an epoch
    // every second act, while the grants used 109k-168k of a 524k pool).
    let need = need_past(window, &plan, best, beyond(window, &time, &claims), &probe, trained).grown(view_sent(&claims), trained);
    let priced = Priced::Held {
        held: plan.window,
        seconds: time.seconds(f64::from(held_context(&plan))),
        value_per_second: best,
        at_window: at_window.map(|rate| (time.seconds(f64::from(held_context(&served))), rate)),
        measured,
    };
    // The hard limits a receipt and her irreducible window read are her claims' own, never the
    // price's floor.
    ActPlan { priced, need, limits: served.limits, hard_limits: served.hard_limits, ..plan }
}

/// The windows past `window` her need may reach: above the served window, up to the largest
/// measured context with her reply's want and never past the model's trained context. `None` when
/// that range is empty, or the trained context is unreported (her need then stops at her grant,
/// censored).
fn beyond(window: u32, time: &ActTime, claims: &ActClaims) -> Option<(u32, u32)> {
    let TrainedContext::Reported(trained) = claims.trained else {
        return None;
    };
    let high = trained.get().min(time.measured().1.saturating_add(claims.reply.max_tokens));
    (high > window).then_some((window.saturating_add(1), high))
}

/// Her need from the plan the price chose (`plan`, at rate `best`): that window, unless the choice
/// is the top of her grant, where the peak may lie past it. Then the search continues over
/// `beyond` (the windows past her grant): a higher rate there is her need, censored by the grant;
/// none is the grant itself. An unreported trained context leaves the grant as a lower bound.
fn need_past(
    window: u32,
    plan: &ActPlan,
    best: f64,
    beyond: Option<(u32, u32)>,
    probe: &impl Fn(u32) -> (f64, ActPlan),
    trained: TrainedContext,
) -> ActNeed {
    // Within the ternary search's resolution of the top: the peak may lie at or past the grant.
    let at_top = plan.window.saturating_add(2) >= window;
    if !at_top {
        return ActNeed::at(plan.window, false, trained);
    }
    if trained == TrainedContext::Unreported {
        return ActNeed::at(window, true, trained);
    }
    match beyond.map(|(low, high)| peak(low, high, probe)) {
        Some((rate, Some(past))) if rate > best => ActNeed::at(past.window, true, trained),
        _ => ActNeed::at(plan.window, false, trained),
    }
}

#[cfg(test)]
mod tests;
