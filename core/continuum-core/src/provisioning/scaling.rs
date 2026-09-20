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

use super::model_catalog::PowerMode;

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

/// Decides the drive mode from live conditions. THE definable seam — swap in a user's or
/// an AI citizen's own policy (the "tuner"); the default auto-shifts like a car.
pub trait ScalingPolicy: Send + Sync {
    fn mode(&self, ctx: &DemandContext) -> PowerMode;
}

/// The default policy — the automatic transmission. It shifts by difficulty, but only as
/// far up as the box can currently afford, and drops to Eco when memory is starved (a
/// game is open):
/// - trivial ("thanks!") → Eco — never haul in a big model for pleasantries;
/// - everyday → Comfort — the economical 32-mpg default;
/// - hard + room → Sport — climb to a bigger model;
/// - out-of-its-league + plenty of room → Performance — floor it, the teacher's brain.
/// Adapt on the fly; the user never notices the shift.
pub struct DefaultScalingPolicy;

impl ScalingPolicy for DefaultScalingPolicy {
    fn mode(&self, ctx: &DemandContext) -> PowerMode {
        let avail_gib = ctx.available_bytes / (1 << 30);
        // Protect a starved box: little free memory → Eco no matter the difficulty. A
        // game ate the RAM; escalation would only thrash.
        if avail_gib < 8 {
            return PowerMode::Eco;
        }
        match ctx.difficulty {
            d if d >= 0.85 && avail_gib >= 20 => PowerMode::Performance,
            d if d >= 0.66 && avail_gib >= 12 => PowerMode::Sport,
            d if d < 0.2 => PowerMode::Eco, // trivial pleasantries
            _ => PowerMode::Comfort,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(difficulty: f32, available_gib: u64) -> PowerMode {
        DefaultScalingPolicy.mode(&DemandContext {
            difficulty,
            available_bytes: available_gib * (1 << 30),
        })
    }

    // what this catches: the automatic transmission shifts by difficulty but only as far
    // as the box can afford, and drops to Eco when a game has starved the RAM —
    //  - trivial "thanks!" → Eco (never a big model for pleasantries);
    //  - everyday → Comfort (the 32-mpg default);
    //  - hard + room → Sport; out-of-league + plenty of room → Performance;
    //  - the SAME hard turn on a game-starved box → Eco, not a thrash.
    #[test]
    fn auto_shifts_by_difficulty_bounded_by_free_memory() {
        assert_eq!(mode(0.1, 128), PowerMode::Eco); // trivial
        assert_eq!(mode(0.5, 60), PowerMode::Comfort); // everyday
        assert_eq!(mode(0.7, 60), PowerMode::Sport); // hard + room
        assert_eq!(mode(0.9, 60), PowerMode::Performance); // out of its league + room
        assert_eq!(mode(0.9, 5), PowerMode::Eco); // game ate the RAM → don't thrash
                                                  // Hard but only middling free memory → shift up only to Sport, not Performance.
        assert_eq!(mode(0.9, 14), PowerMode::Sport);
    }
}
