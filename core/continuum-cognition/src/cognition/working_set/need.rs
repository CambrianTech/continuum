//! What one turn of a mind needs to generate, per channel: the estimate the reply claim sizes
//! from. Landed turns are measurements; a turn cut inside its think is evidence too, but only
//! of a lower bound ("at least this much"), so it is read as a CENSORED sample, never as a
//! measurement (Benchy on the M5, 2026-10-31: his reasoning need read 520 while every think
//! past ~600 was cut, and a cut was not recorded at all, so the need could never grow).

/// How many recent turns the reasoning / answer NEED is read from — a p90 over this
/// many is the mind's measured demand per channel; a peak alone is one loud turn.
pub(super) const NEED_SAMPLES: usize = 16;
/// What ONE turn of this mind needs to generate, per channel: her measured reasoning
/// (the `<think>` / `reasoning_content` channel) and her measured answer — each the p90
/// of recent turns, as measured: no headroom here (the reply claim grows its grant above this
/// floor by priority, which is the headroom, derived). Measured on the M5 (2026-09-20, 20:58–21:14Z) because
/// #4194 sized the allowance in TIME alone and two turns ended inside the reasoning
/// channel (`persona.act.think_only reasoning_len=2730`, `2112`): no answer, no act. An
/// allowance that does not hold the think never reaches the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputNeed {
    /// Tokens of reasoning a turn needs: the p90 over the turns that landed.
    pub reasoning: u32,
    /// Tokens of answer a turn needs: the p90.
    pub answer: u32,
    /// Turns the need was read from — how much to trust it (the claim decides; one sample's
    /// p90 is that sample).
    pub turns: u32,
    /// Her reasoning need corrected for her recent cut thinks, where that is HIGHER than
    /// `reasoning`; `None` when no cut in her last [`NEED_SAMPLES`] turns raises it. A lower
    /// bound, never a measurement: a thinking model fills whatever room it is given, so
    /// granting it unbounded walks every mind to the reserve (8k → 184k in 20 simulated
    /// turns, `EmissionStop::ThinkBudgetHit`). The reply claim may test upward toward it
    /// only under a measured ceiling (the turn's time term); nothing here grants it.
    pub reasoning_at_least: Option<u32>,
}

impl OutputNeed {
    /// The whole turn: think, then answer.
    pub fn total(self) -> u32 {
        self.reasoning.saturating_add(self.answer)
    }

    /// What one ACT needs to write, as ONE claim: her think (raised to its censored bound after a
    /// recent cut, `reasoning_at_least`) plus her answer (the tool call and its serialization).
    /// Never the answer alone: the think and the call share `max_tokens`, and an act granted
    /// her answer's room spent it thinking (the M5, 2026-11-04: caps of 72-89 tokens, every
    /// one spent on reasoning, 302 of 414 acts cut at their first sentence).
    pub fn act_room(self) -> u32 {
        let think = match self.reasoning_at_least {
            Some(bound) => bound.max(self.reasoning),
            None => self.reasoning,
        };
        think.saturating_add(self.answer)
    }
}

/// An act's output room as her measurements know it. Unknown is a state, never a number: a cold
/// mind has no measured need, so nothing is reserved in her name and nothing is refused for it;
/// her first act measures her.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActRoom {
    /// No act of hers has landed yet.
    Unmeasured,
    /// Her measured need ([`OutputNeed::act_room`]) and the answer share inside it.
    Needs { room: std::num::NonZeroU32, answer: u32 },
}

impl ActRoom {
    pub fn of(need: Option<OutputNeed>) -> Self {
        match need.and_then(|n| std::num::NonZeroU32::new(n.act_room()).map(|room| (room, n.answer))) {
            Some((room, answer)) => Self::Needs { room, answer },
            None => Self::Unmeasured,
        }
    }

    /// The tokens her act must be able to write; `None` when unmeasured.
    pub fn needed(self) -> Option<u32> {
        match self {
            Self::Unmeasured => None,
            Self::Needs { room, .. } => Some(room.get()),
        }
    }
}

/// The p90 of `samples` (ascending sort, the value at the 90th percentile position).
/// Empty = 0.
pub(super) fn p90(samples: &[u32]) -> u32 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let idx = (n * 9).div_ceil(10).saturating_sub(1);
    sorted[idx.min(n - 1)]
}

/// The reasoning p90 over `landed` measurements and `cut` lower bounds ([`product_limit`]):
/// `Some` only where it is higher than `landed_need`, so with no cuts it is `None`.
pub(super) fn reasoning_at_least(landed: &[u32], cut: &[u32], landed_need: u32) -> Option<u32> {
    if cut.is_empty() {
        return None;
    }
    // The p90: the share of turns still running past it falls to a tenth.
    product_limit(landed, cut, |survival_num, survival_den| survival_num * 10 <= survival_den)
        .filter(|&need| need > landed_need)
}

/// A quantile over `landed` measurements and `cut` lower bounds, by the product-limit
/// (Kaplan–Meier) estimate: the smallest landed size at which the estimated share of turns still
/// running past it satisfies `reached(survival_num, survival_den)`. A cut's mass moves to the
/// turns that ran longer than it, never to shorter ones, so with no cuts this is the plain
/// quantile, and cuts can only raise it. When the cuts hold the whole tail (the survival never
/// reaches the quantile), the quantile lies past every landed turn, and the largest cut bound is
/// what is known of it. Exact integer ratios, so two nodes agree. `None` with no sample at all.
/// Shared by the reply's reasoning need (p90) and the window's held need (median,
/// `held_need::need_tokens`): one estimator for every censored observation of a mind.
pub(super) fn product_limit(landed: &[u32], cut: &[u32], reached: impl Fn(u128, u128) -> bool) -> Option<u32> {
    // Ascending; at equal tokens a landed turn counts before a cut (it was at risk at that size).
    let mut turns: Vec<(u32, bool)> =
        landed.iter().map(|&t| (t, false)).chain(cut.iter().map(|&t| (t, true))).collect();
    turns.sort_unstable();
    let (mut survival_num, mut survival_den) = (1u128, 1u128);
    let mut at_risk = turns.len() as u128;
    for (tokens, censored) in turns {
        if !censored {
            survival_num *= at_risk - 1;
            survival_den *= at_risk;
            if reached(survival_num, survival_den) {
                return Some(tokens);
            }
        }
        at_risk -= 1;
    }
    cut.iter().copied().max()
}
