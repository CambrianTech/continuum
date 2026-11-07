//! Context has a price (card 7b7a696a): every token she holds slows every act (her measured
//! [`ActTime`]), so the allocation holds the window where her act's value per second peaks, never
//! simply the whole window it was served. Joel, 2026-11-07: the allocation is "an optimizer too. It
//! sort of searches to fit"; "constants never"; "a mind gets what she needs, never a division".
//!
//! What is maximized: `R(W) = V(W) / T(W)`, her act's value per second at a held window `W`, where
//! `V(W)` is the search's own value at `W` (`Σ priority x ln(1 + tokens shown)`, [`super::search`])
//! and `T(W)` her measured seconds per act holding `W` less her reply's grant
//! ([`ActTime::seconds`]).
//!
//! The exchange rate between value and seconds is no constant: at the peak `dV/dW = R x dT/dW`, so
//! a token is kept exactly while the value it adds, over her whole act's value, exceeds the seconds
//! it adds, over her whole act's seconds. The rate is `R` itself, her own value per second at the
//! plan she holds (Dinkelbach's form: the peak of `V/T` is the plan maximizing `V - R x T` at that
//! `R`); the one measured quantity it comes from is her act time. Holding `W` instead of a smaller
//! window is equivalent to a lower price on every token: allocating at the peak's window reproduces
//! the priced optimum up to the levels' steps, so every reader downstream (her view's start, its
//! room, the depth of a turnover) sees one consistent window and nothing else changes.
//!
//! Never priced out, by construction (the search's lower bound [`least`]): every hard limit, her
//! held head levels (a head level changed mid-epoch re-prefills the whole prompt), her newest
//! result WHOLE (goal 7) and her reply's measured need. A window that holds no more than that is
//! the window as before ([`Priced::WindowBinds`]); an unmeasured act time prices nothing
//! ([`Priced::Unmeasured`]).
//!
//! How it searches: ternary on the held window over `[least, window]`, to the token, each probe one
//! plan ([`super::plan_at`]: microseconds); `R` is unimodal where the levels are continuous and a
//! sawtooth of level steps elsewhere, so the served window is probed too and the better kept: the
//! answer is never below the window as before in value per second.

use super::{hard_limits, id, plan_at, ActClaims, ActPlan};
use crate::price::{ActTime, ContextPrice};

/// What the time price of context decided for one act.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Priced {
    /// No measured act time: no cost term, the window filled as before (said in her receipt).
    #[default]
    Unmeasured,
    /// What she must hold (every hard limit, her newest result whole, her reply's need) already
    /// fills the window: nothing to price.
    WindowBinds,
    /// The window her act holds, where her value per second peaks.
    Held {
        /// The window the plan shares (at most the served one).
        held: u32,
        /// Her predicted seconds per act holding it, and holding the whole served window.
        seconds: f64,
        seconds_at_window: f64,
        /// Her value per second at it (the exchange rate), and at the whole served window.
        value_per_second: f64,
        value_per_second_at_window: f64,
    },
}

/// The least window the price may hold: every hard limit with her held head levels in place of
/// her head's hard limits, her newest result whole, and her reply's measured need.
fn least(claims: &ActClaims) -> u32 {
    let held_above = |head: &super::Head| head.holding().map_or(0, |held| held.cost.saturating_sub(head.leveled.hard_limit())); // map_or: a head no epoch holds adds nothing above its hard limit
    hard_limits(claims)
        .saturating_add(held_above(&claims.framing))
        .saturating_add(held_above(&claims.tools))
        .saturating_add(claims.newest.want())
        .saturating_add(claims.reply.floor_tokens)
}

/// The value her context above her least plan adds per second of her act, for `plan`: `None` when
/// it has no search answer or its predicted seconds are not positive (an act time extrapolated
/// outside its range says nothing there).
fn rate(plan: &ActPlan, least_value: f64, time: &ActTime) -> Option<f64> {
    let value = plan.searched.as_ref()?.value;
    let held = plan.window.saturating_sub(plan.granted(id::REPLY));
    let seconds = time.seconds(f64::from(held));
    (seconds > 0.0).then(|| (value - least_value) / seconds)
}

/// The plan of one act: at the window where her value per second peaks within `window`.
pub(super) fn hold(window: u32, claims: ActClaims) -> ActPlan {
    let ContextPrice::Measured(time) = claims.price else {
        return plan_at(window, claims);
    };
    let floor = least(&claims);
    if floor >= window {
        return ActPlan { priced: Priced::WindowBinds, ..plan_at(window, claims) };
    }
    // What her least plan shows is in every plan the price can choose: its value buys nothing the
    // price trades, so the rate counts the value above it.
    let Some(least_value) = plan_at(floor, claims.clone()).searched.map(|found| found.value) else {
        return plan_at(window, claims);
    };
    let probe = |w: u32| {
        let plan = plan_at(w, claims.clone());
        (rate(&plan, least_value, &time).unwrap_or(f64::NEG_INFINITY), plan) // unwrap_or: a window with no rate is never the peak
    };
    let (at_window, served) = probe(window);
    let (mut lo, mut hi) = (floor, window);
    while hi - lo > 2 {
        let third = (hi - lo) / 3;
        let (m1, m2) = (lo + third, hi - third);
        if probe(m1).0 < probe(m2).0 {
            lo = m1;
        } else {
            hi = m2;
        }
    }
    // The served window is a candidate too: the answer is never below the window as before.
    let (mut best, mut plan) = (at_window, served.clone());
    for (rate, candidate) in (lo..=hi).map(probe) {
        if rate > best {
            (best, plan) = (rate, candidate);
        }
    }
    if !best.is_finite() {
        return ActPlan { priced: Priced::Unmeasured, ..served };
    }
    let seconds_of = |p: &ActPlan| time.seconds(f64::from(p.window.saturating_sub(p.granted(id::REPLY))));
    let priced = Priced::Held {
        held: plan.window,
        seconds: seconds_of(&plan),
        seconds_at_window: seconds_of(&served),
        value_per_second: best,
        value_per_second_at_window: at_window,
    };
    ActPlan { priced, ..plan }
}

#[cfg(test)]
mod tests;
