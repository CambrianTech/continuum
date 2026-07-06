//! Scaling policy — the definable seam that decides how hard to push a request.
//!
//! The misfit box is a moving target, like any good OS: a game opens and RAM vanishes, a
//! game closes and it returns; a turn is "thanks!" one moment and a substantial refactor
//! the next. The system adapts on the fly so the user never notices — they've "never seen
//! low latency." This is where the [[model-selection-is-a-dynamic-gas-pedal]] pedal gets
//! PRESSED: from live conditions + detected difficulty, not a static choice.
//!
//! Two inputs, both real signals we already have (never a prompt-keyword guess
//! [[no-hardcoded-heuristics-to-steer-cognition]]):
//! - `difficulty` — from the persona's own confidence / deliberation (should_respond,
//!   rate_proposals). "When the model realizes it's out of its league, it can escalate."
//! - `available_bytes` — `SystemResourceMonitor::memory().available_bytes`, the LIVE free
//!   memory that shrinks under a game and returns when it closes.
//!
//! `ScalingPolicy` is the seam a user OR a first-class AI citizen can define their own of;
//! `DefaultScalingPolicy` is the sensible default (mundane stays small, escalate only when
//! genuinely needed AND the box can spare it).

use super::model_catalog::QualityTarget;

/// What a scaling decision sees: how hard the work looks + what the machine can spare
/// right now. Fed from persona signals + the live resource monitor.
#[derive(Debug, Clone, Copy)]
pub struct DemandContext {
    /// 0.0 (mundane — "thanks!") … 1.0 (clearly out of the persona's league). Sourced
    /// from the persona's confidence / deliberation, never a keyword heuristic.
    pub difficulty: f32,
    /// Memory free RIGHT NOW — shrinks when a game opens, grows when it closes.
    pub available_bytes: u64,
}

/// Decides the gear from live conditions. THE definable seam — swap in a user's or an AI
/// citizen's own policy; the default is conservative-with-escalation.
pub trait ScalingPolicy: Send + Sync {
    fn target(&self, ctx: &DemandContext) -> QualityTarget;
}

/// The default policy: stay `Balanced` for the mundane — never haul in a 32B for a
/// please/thank-you — and escalate to `Maximum` only when the persona is genuinely
/// struggling AND the box can currently spare the horsepower. Adapt on the fly; the user
/// never notices the gear change.
pub struct DefaultScalingPolicy;

impl ScalingPolicy for DefaultScalingPolicy {
    fn target(&self, ctx: &DemandContext) -> QualityTarget {
        /// Above this the persona is clearly out of its league.
        const HARD: f32 = 0.66;
        /// Don't floor it on a box that can't currently host the big model — a game is
        /// open, memory is tight. Balanced still serves; escalation would just thrash.
        const FLOOR_NEEDS_BYTES: u64 = 20 * (1 << 30);

        if ctx.difficulty >= HARD && ctx.available_bytes >= FLOOR_NEEDS_BYTES {
            QualityTarget::Maximum
        } else {
            QualityTarget::Balanced
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(difficulty: f32, available_gib: u64) -> QualityTarget {
        DefaultScalingPolicy.target(&DemandContext {
            difficulty,
            available_bytes: available_gib * (1 << 30),
        })
    }

    // what this catches: the whole adaptive intent in one policy —
    //  - mundane stays cheap no matter how much RAM is free (no 32B for "thanks!");
    //  - a genuinely hard turn escalates WHEN the box can spare it;
    //  - the SAME hard turn stays Balanced when a game has eaten the memory (adapt to
    //    live conditions instead of thrashing).
    #[test]
    fn mundane_stays_cheap_hard_escalates_only_when_affordable() {
        // Mundane — Balanced even on a huge idle box.
        assert_eq!(target(0.1, 128), QualityTarget::Balanced);
        // Hard + plenty free → floor it.
        assert_eq!(target(0.9, 60), QualityTarget::Maximum);
        // Hard but a game ate the RAM (5 GiB free) → stay Balanced, don't thrash.
        assert_eq!(target(0.9, 5), QualityTarget::Balanced);
        // Right at the difficulty edge below the threshold → still cheap.
        assert_eq!(target(0.5, 60), QualityTarget::Balanced);
    }
}
