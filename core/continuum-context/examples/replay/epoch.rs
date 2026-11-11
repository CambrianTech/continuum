//! The epoch `replay --allocate` carries per resident, so the dyno prices her acts the way the live
//! core does. Live, her view is an append-only epoch: what her last prompt carried is resident in
//! the engine, so each view source's claim has its basis (`min_tokens`) at what the epoch already
//! sent (`view::Claim::held`, folded in by the faculty's `view_claims`), and the price treats that
//! as must-hold (`allocation::priced`). A grant below it is a turnover: the view re-renders and is
//! prefilled again.
//!
//! Per act: [`Epoch::hold`] sets each view claim's basis to what the epoch kept of that source on
//! her previous act (at least its floor); after the plan, [`Epoch::record`] keeps what the plan
//! granted each source as the next basis, counts a turnover when any source was granted less than
//! its basis, and predicts the act's fresh tokens:
//! - her first act, or an act whose heads (framing, tools) changed: the whole prompt (cold);
//! - a turnover: everything after the heads (the view and her act loop re-prefill);
//! - otherwise the new tail: the prompt less the heads, each view source's basis (at most its
//!   grant), and, inside one turn (the same ask), her act loop as the previous act held it.

use std::collections::HashMap;

use continuum_context::allocation::{id, ActClaims, ActPlan};

/// The parts of her act loop: resident from act to act while her turn (its ask) lasts.
const LOOP: [&str; 4] = [id::REQUIRED, id::LAST_MOVE, id::NEWEST, id::HISTORY];

/// What the plan holds for her prompt: every grant but the reply's.
pub fn kept(plan: &ActPlan) -> u32 {
    plan.claims().iter().filter(|claim| claim.source_id != id::REPLY).map(|claim| plan.granted(&claim.source_id)).fold(0u32, u32::saturating_add)
}

/// One resident's epoch between acts.
#[derive(Default)]
pub struct Epoch {
    /// Each view source's basis: what the plan kept of it on her previous act.
    held: HashMap<String, u32>,
    /// The heads' grants (framing, tools) her cache holds; `None` before her first act.
    heads: Option<(u32, u32)>,
    /// Her turn (a key of its ask) and what her act loop held on the previous act of it.
    turn: Option<(u64, u32)>,
}

/// One act as the epoch saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// What the plan holds for her prompt (at most what was sent).
    pub kept: u32,
    /// A view source was granted less than the epoch held of it: the view re-prefills.
    pub turnover: bool,
    /// Predicted prompt tokens prefilled this act.
    pub fresh: u32,
}

impl Epoch {
    /// Set each view claim's basis to what the epoch holds of it (at least its floor) and its want
    /// to at least that, as the core does; returns the tokens the epoch already sent of her view.
    pub fn hold(&self, claims: &mut ActClaims) -> u32 {
        let mut sent = 0u32;
        for view in &mut claims.view {
            let claim = &mut view.claim;
            let Some(&held) = self.held.get(&claim.source_id) else {
                continue;
            };
            claim.min_tokens = claim.floor_tokens.max(held);
            claim.max_tokens = claim.max_tokens.max(held);
            sent = sent.saturating_add(held);
        }
        sent
    }

    /// Fold in the plan of her act (`ask`: the key of its turn; `sent`: its billed prompt) and
    /// predict what it prefilled.
    pub fn record(&mut self, plan: &ActPlan, ask: u64, sent: u32) -> Step {
        let kept = kept(plan).min(sent);
        let heads = (plan.granted(id::FRAMING), plan.granted(id::TOOLS));
        let warm = self.heads == Some(heads);
        let (mut turnover, mut view_resident) = (false, 0u32);
        let mut held = HashMap::new();
        for claim in plan.claims().iter().filter(|claim| claim.source_id.starts_with(id::VIEW)) {
            let granted = plan.granted(&claim.source_id);
            if let Some(&basis) = self.held.get(&claim.source_id) {
                turnover |= granted < basis;
                view_resident = view_resident.saturating_add(basis.min(granted));
            }
            held.insert(claim.source_id.clone(), granted);
        }
        let looped = LOOP.iter().map(|part| plan.granted(part)).fold(0u32, u32::saturating_add);
        let carried = match self.turn {
            Some((key, before)) if key == ask => before.min(looped),
            _ => 0,
        };
        let head_tokens = heads.0.saturating_add(heads.1);
        let resident = match (warm, turnover) {
            (false, _) => 0,
            (true, true) => head_tokens,
            (true, false) => head_tokens.saturating_add(view_resident).saturating_add(carried),
        };
        self.held = held;
        self.heads = Some(heads);
        self.turn = Some((ask, looped));
        Step { kept, turnover, fresh: kept.saturating_sub(resident) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_context::allocation::{allocate, Head, Leveled, Newest, Rungs, Span, TrainedContext, ViewClaim};
    use continuum_context::flex::FlexClaim;
    use continuum_context::price::ContextPrice;

    fn claim(source_id: &str, floor: u32, want: u32, required: bool) -> FlexClaim {
        FlexClaim { source_id: source_id.into(), priority: 5, floor_tokens: floor, min_tokens: floor, max_tokens: want, required }
    }

    /// One act: a 50-token system prompt and ask, a 100-token reply, and one view source wanting
    /// `view` tokens above its 100-token floor.
    fn act(view: u32) -> ActClaims {
        let fixed = || Leveled::one(Rungs::whole(50));
        ActClaims {
            framing: Head::free(fixed()),
            required: fixed(),
            newest: Newest::Absent,
            tools: Head::free(Leveled::default()),
            grounding: Leveled::default(),
            last_move: Leveled::default(),
            history: Span { floor: 0, min: 0, want: 0 },
            reply: claim(id::REPLY, 100, 100, true),
            view: vec![ViewClaim::from(claim("view/chat", 100, view, false))],
            price: ContextPrice::Unmeasured,
            trained: TrainedContext::Unreported,
        }
    }

    // what this catches: the dyno pricing each act as if nothing were resident (no basis carried),
    // a turnover that goes uncounted when a small window grants the view below what it sent, or a
    // turnover act predicted to prefill only its tail instead of re-prefilling the whole view.
    #[test]
    fn the_epoch_carries_its_basis_and_counts_a_turnover_when_a_grant_falls_below_it() {
        let mut epoch = Epoch::default();
        let first = epoch.record(&allocate(10_000, act(1_000)), 1, u32::MAX);
        assert!(!first.turnover);
        assert_eq!(first.fresh, first.kept, "her first act is cold");

        let mut grown = act(1_200);
        assert_eq!(epoch.hold(&mut grown), 1_000);
        assert_eq!(grown.view[0].claim.min_tokens, 1_000, "the view's basis is what the epoch sent");
        let warm = epoch.record(&allocate(10_000, grown), 1, u32::MAX);
        assert!(!warm.turnover);
        assert_eq!(warm.fresh, warm.kept - 50 - 1_000 - 50, "a warm act prefills only its tail");

        let mut tight = act(1_200);
        epoch.hold(&mut tight);
        let over = epoch.record(&allocate(700, tight), 1, u32::MAX);
        assert!(over.turnover, "a window below the basis turns the epoch over");
        assert_eq!(over.fresh, over.kept - 50, "a turnover re-prefills everything after the heads");
    }
}
