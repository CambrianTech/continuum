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
//!   changed mid-epoch re-prefills her whole prompt), her newest result WHOLE (goal 7) and her
//!   reply's measured need. A window no larger than that is the window as before
//!   ([`Priced::WindowBinds`]); an unmeasured act time prices nothing ([`Priced::Unmeasured`]).
//!
//! How it searches: ternary on the held window over the measured part of `[least, window]`, to the
//! token, each probe one plan ([`super::plan_at`]: microseconds; ~2 ms an act). The served window
//! is a candidate too wherever it is measured, so the answer is never below the window as before
//! in value per second there.

use super::{hard_limits, id, plan_at, ActClaims, ActPlan, Consumer, Head};
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
    hard_limits(claims)
        .saturating_add(held_above(&claims.framing))
        .saturating_add(held_above(&claims.tools))
        .saturating_add(claims.newest.want())
        .saturating_add(claims.reply.floor_tokens)
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

/// The plan of one act: at the window where her value per second peaks within `window`.
pub(super) fn hold(window: u32, claims: ActClaims) -> ActPlan {
    let ContextPrice::Measured(time) = claims.price else {
        return plan_at(window, claims);
    };
    let priced_claims = choosable(&claims);
    let floor = least(&priced_claims);
    if floor >= window {
        return ActPlan { priced: Priced::WindowBinds, ..plan_at(window, claims) };
    }
    let measured = time.measured();
    let served = plan_at(window, claims.clone());
    // The windows whose held context can lie inside the measured contexts: at least the smallest
    // measured context, at most the largest plus her reply's whole want.
    let lo = floor.max(measured.0);
    let hi = window.min(measured.1.saturating_add(claims.reply.max_tokens));
    let outside = ActPlan { priced: Priced::OutsideMeasured { measured }, ..served.clone() };
    let Some(least_value) = plan_at(floor, priced_claims.clone()).searched.map(|found| found.value) else {
        return outside;
    };
    if lo > hi {
        return outside;
    }
    let probe = |w: u32| {
        let plan = plan_at(w, priced_claims.clone());
        (rate(&plan, least_value, &time).unwrap_or(f64::NEG_INFINITY), plan) // unwrap_or: a window with no rate (outside the measured contexts) is never the peak
    };
    let (mut low, mut high) = (lo, hi);
    while high - low > 2 {
        let third = (high - low) / 3;
        let (m1, m2) = (low + third, high - third);
        if probe(m1).0 < probe(m2).0 {
            low = m1;
        } else {
            high = m2;
        }
    }
    // The served window is a candidate too wherever it is measured: the answer is then never
    // below the window as before.
    let at_window = rate(&served, least_value, &time);
    let (mut best, mut plan) = (at_window.unwrap_or(f64::NEG_INFINITY), served.clone()); // unwrap_or: an unmeasured served window competes with nothing
    for (rate, candidate) in (low..=high).map(probe) {
        if rate > best {
            (best, plan) = (rate, candidate);
        }
    }
    if !best.is_finite() {
        return outside;
    }
    let priced = Priced::Held {
        held: plan.window,
        seconds: time.seconds(f64::from(held_context(&plan))),
        value_per_second: best,
        at_window: at_window.map(|rate| (time.seconds(f64::from(held_context(&served))), rate)),
        measured,
    };
    // The hard limits a receipt and her need read are her claims' own, never the price's floor.
    ActPlan { priced, limits: served.limits, hard_limits: served.hard_limits, ..plan }
}

#[cfg(test)]
mod tests;
