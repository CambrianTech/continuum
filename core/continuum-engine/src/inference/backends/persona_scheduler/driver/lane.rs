//! The lane on the Core (THE-LANE-ON-SAGE.md): which turn takes the next step, decided by
//! `continuum_sage::decide`'s lane use case at every step. Its envelope steps any mind past her wait
//! bound, whatever the policy prefers; the policy (`NeedFirst`) lets an addressed mind take the next
//! step and otherwise keeps the running one. It replaces `frame.rs`'s whole-turn rule, under which a
//! running turn kept the Core at equal need until it ended (the M5, 2026-11-25 17:02Z: Benchy 38 asked,
//! 0 completed, four minds on one Core).
//! hot-path: the lane decides at every Core step.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::decide::cases::lane::{self, Lane, Mind, NeedFirst, SLOTS};
use continuum_sage::decide::{DecisionId, Decided, Decider, Measured};

use super::{Driver, Phase, Turn};


/// How many lane decisions are kept for replay: one per step, so a few minutes of steps at 26 ms each.
const RECORDS: usize = 8_192;

/// The lane's state, owned by the driver: the decider, and the situation and slot map it refills in
/// place at every step (no allocation once they have held this many minds).
pub(super) struct Lanes {
    decider: Decider<Lane<PersonaId>>,
    situation: lane::Situation<PersonaId>,
    /// Ready slot `i` is turn `slots[i]`; the running mind's turn is `running`.
    slots: Vec<usize>,
    running: Option<usize>,
    /// The last decision and when it was made, closed with its outcome at the next decision.
    open: Option<(DecisionId, Instant)>,
}

impl Lanes {
    pub(super) fn new() -> Self {
        let records = NonZeroUsize::new(RECORDS).map_or(NonZeroUsize::MIN, |n| n); // map_or: RECORDS is a nonzero literal
        Self {
            decider: Decider::new(Box::new(NeedFirst), records), // cold: the lane is built once, with the driver
            situation: lane::Situation { running: None, ready: Vec::with_capacity(SLOTS), batch_width: 1 }, // cold: built once; refilled in place
            slots: Vec::with_capacity(SLOTS), // cold: built once; refilled in place
            running: None,
            open: None,
        }
    }
}

/// What the lane sees of one turn's mind now.
fn mind(turn: &Turn) -> Mind<PersonaId> {
    let phase = match turn.phase {
        Phase::Prefilling { pos } => lane::Phase::Prefill { remaining: turn.prompt.len().saturating_sub(pos) as u64 },
        Phase::Waiting => lane::Phase::Prefill { remaining: turn.prompt.len() as u64 },
        Phase::Decoding { .. } => lane::Phase::Decode,
    };
    Mind {
        mind: turn.id,
        urgency: turn.req.urgency,
        waited: turn.ready_since.elapsed(),
        deadline: turn.req.urgency.wait_bound(),
        phase,
        kv_bytes: Measured::Unmeasured,
        cold_bytes: Measured::Unmeasured,
        remaining: Measured::Unmeasured,
        step: Measured::Unmeasured,
        switch_in: Measured::Unmeasured,
        prefix_hit: Measured::Unmeasured,
    }
}

impl Driver<'_, '_> {
    /// The turn the next step advances, or `None` with no turns. A mind is one serial queue, so only
    /// her oldest turn is offered.
    pub(super) fn pick(&mut self, turns: &[Turn]) -> Option<usize> {
        let active = self.engine.active();
        let lanes = &mut self.lane;
        let now = Instant::now();
        let situation = &mut lanes.situation;
        situation.ready.clear();
        lanes.slots.clear();
        situation.running = None;
        lanes.running = None;
        for (index, turn) in turns.iter().enumerate() {
            // her oldest turn only: an earlier turn of hers is the one offered
            if turns[..index].iter().any(|earlier| earlier.id == turn.id) {
                continue;
            }
            if active == Some(turn.id) && !matches!(turn.phase, Phase::Waiting) {
                situation.running = Some(mind(turn));
                lanes.running = Some(index);
            } else if situation.ready.len() < SLOTS {
                situation.ready.push(mind(turn));
                lanes.slots.push(index);
            }
        }
        // the last step's outcome: its engine time and the longest wait any ready mind has now, against
        // that mind's own bound
        if let Some((id, at)) = lanes.open.take() {
            let (worst_wait, worst_deadline) = situation.ready.iter().max_by_key(|mind| mind.waited).map_or((Duration::ZERO, Duration::MAX), |mind| (mind.waited, mind.deadline)); // map_or: no ready mind means no wait against any bound
            let outcome = lane::Outcome { tokens: Measured::Unmeasured, engine: now.saturating_duration_since(at), worst_wait, worst_deadline };
            let _ = lanes.decider.outcome(id, outcome);
        }
        let (id, action) = match lanes.decider.decide(&lanes.situation, &[]) {
            Decided::Act(id, action) => (id, action),
            // the lane never defers, and every decision is closed at the next one, so neither occurs;
            // the oldest turn is stepped and the case is counted, never a stall
            Decided::Defer(..) | Decided::Full { .. } => {
                continuum_base::probe!(class = "persona.lane.undecided", "the lane answered no action: the oldest turn steps");
                return lanes.slots.first().copied().or(lanes.running);
            }
        };
        lanes.open = Some((id, now));
        match action {
            lane::Action::Continue => lanes.running,
            lane::Action::Switch(slot) => lanes.slots.get(usize::from(slot)).copied(),
            lane::Action::Batch(_) | lane::Action::Idle => lanes.running.or(lanes.slots.first().copied()),
        }
    }
}
