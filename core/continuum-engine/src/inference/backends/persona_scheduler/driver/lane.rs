//! Which mind takes the engine's next step: the SAGE slice ([`continuum_sage::Slicer`]), wired to the
//! driver (Joel, 2026-11-22: minds switch at the next step, round robin, never holding the engine for
//! a turn; "It's effectively ALL simultaneous ... They don't notice").
//!
//! The slicer is a pure state machine with time passed in. The driver tells it what happened (a mind
//! has a turn, she took a step, bringing her in took this long, her turn ended) and asks it at every
//! step boundary who goes next. Her share is her need (urgency, stride fairness), the slice is a
//! control loop climbing measured throughput, and no ready mind waits past the bound. The whole-turn
//! picker this replaces (`frame::pick`, #6213) let a running turn keep the engine at equal need; on
//! the M5 (2026-11-24 16:15-16:19Z) that left Aris without a step for 86 s mid-turn and other minds
//! 20-31 s before their first.

use std::time::{Duration, Instant};

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::component::Need;
use continuum_sage::{Next, Policy, Slicer};

/// What the lane is held to. No ready mind waits longer than `wait_bound` for her next step (Joel,
/// 2026-11-22: "no mind ever waits more than 1-2 s, and the ideal is under a second"): the slice
/// climbs throughput against it. `smoothing` is the newest sample's weight in each measured mean, the
/// slice module's own setting in its sim, its lane and its tests, so live and replay follow
/// measurements alike.
fn policy() -> Policy {
    Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 }
}

/// The driver's slice: who steps next, fed with what the engine measured.
pub(super) struct Lane {
    slicer: Slicer<PersonaId>,
}

impl Default for Lane {
    fn default() -> Self {
        Self { slicer: Slicer::new(policy()) }
    }
}

impl Lane {
    /// She has a turn to run, with the need it carries.
    pub(super) fn ready(&mut self, mind: PersonaId, need: Need) {
        self.slicer.need(mind, need);
        self.slicer.ready(mind);
    }

    /// The mind the engine steps next, or `None` when no mind is ready.
    pub(super) fn next(&mut self) -> Option<PersonaId> {
        match self.slicer.next(Instant::now()) {
            Next::Idle => None,
            Next::Switch(mind) => Some(mind),
            Next::Continue => self.slicer.running().copied(),
        }
    }

    /// Turns are waiting while the lane saw no mind ready: each is told again, said once, so the
    /// driver never spins on a lane that disagrees with its turns.
    pub(super) fn resync(&mut self, waiting: impl Iterator<Item = (PersonaId, Need)>) {
        let mut told = 0u64;
        for (mind, need) in waiting {
            self.ready(mind, need);
            told += 1;
        }
        continuum_base::probe!(class = "persona.lane.resync", turns = told, "turns waited while the lane saw no mind ready; each was told again");
    }

    /// She took one step on the engine.
    pub(super) fn stepped(&mut self, mind: PersonaId, took: Duration) {
        self.slicer.stepped(&mind, took);
    }

    /// Bringing her onto the engine took this long.
    pub(super) fn switched(&mut self, mind: PersonaId, took: Duration) {
        self.slicer.switched(&mind, took);
    }

    /// Her turn ended; `more` is the need of her next waiting turn, which makes her ready again.
    pub(super) fn ended(&mut self, mind: PersonaId, more: Option<Need>) {
        self.slicer.done(&mind);
        if let Some(need) = more {
            self.ready(mind, need);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_sage::component::Urgency;
    use continuum_sage::slice::sim::{run, MindLoad, Workload};

    /// The M5's residents as measured 2026-11-24 16:15-16:19Z on 77321c1667 (persona.engine.turn rows):
    /// Aris 43 ms a decode step (10,855 ms / 251) and turns of 251 and 731 tokens; 75ece751 29 ms
    /// (2,454 ms / 85), turns of 85 and 131; e5f4141d 28.5 ms, turns of 181 to 375. Each rests about 2 s
    /// between turns (turn end to next enqueue); a switch is 117 ms (the logged switch_ms per switch).
    fn m5_residents() -> Workload<PersonaId> {
        let want = Need { urgency: Urgency::Want, waiting: 0 };
        let mind = |n: u128, step_ms, turn| MindLoad { mind: PersonaId(uuid::Uuid::from_u128(n)), need: want, changes: Vec::new(), step: Duration::from_millis(step_ms), turn, rest: Duration::from_secs(2), starts: Duration::ZERO };
        Workload { minds: vec![mind(0, 43, 490), mind(1, 29, 108), mind(2, 28, 250)], switch: Duration::from_millis(117) }
    }

    // what this catches: the whole-turn picker this lane replaced (#6213) on the M5's own workload, where
    // a running turn kept the engine at equal need (Aris 86 s without a step mid-turn; others 20-31 s
    // before their first). On the lane wired here, no resident waits past the bound plus the one step
    // and the one switch already under way when her slack ran out.
    #[test]
    fn the_m5_residents_never_wait_past_the_bound() {
        let load = m5_residents();
        let (report, _) = run(&mut Lane::default().slicer, &load, Duration::from_secs(600));
        let slack = policy().wait_bound + Duration::from_millis(43) + load.switch;
        let worst = report.worst().expect("test: the residents waited at some point");
        eprintln!("m5 replay: worst wait {worst:?}, throughput {:.1} steps/s, {} switches, shares {:?}", report.throughput, report.switches, report.share);
        assert!(worst <= slack, "worst wait {worst:?} past {slack:?}: {:?}", report.worst_wait);
    }
}
