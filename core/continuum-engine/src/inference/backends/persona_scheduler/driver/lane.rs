//! The lane on the Core (THE-LANE-ON-SAGE.md): which turn takes the next step, decided at every step
//! by a lane algorithm created by name from `continuum_sage::decide::cases::lane::algorithms` (the
//! scheduler's configuration names it), over the lane's history, which this driver feeds as the
//! executor: every switch, step, turn end, reuse and residency it measures. The envelope steps any
//! mind past her wait bound, whatever the algorithm prefers. It replaces `frame.rs`'s whole-turn rule
//! (the M5, 2026-11-25 17:02Z: Benchy 38 asked, 0 completed, four minds on one Core).
//! hot-path: the lane decides at every Core step.

use std::path::Path;
use std::time::{Duration, Instant};

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::decide::cases::lane::{self, Lane, Mind, SLOTS};
use continuum_sage::decide::cases::lane_history::{LaneHistory, Observation, Views};
use continuum_sage::decide::{DecisionId, Decided, Decider, Fraction, Measured, Unknown};
use continuum_saved_state::{OwnedFile, Rebuildable, Retention, SavedState, Store};

use super::{Driver, Phase, Turn};

/// What the lane's algorithm learned, kept across restarts through the one saved-state contract
/// (SAVED-STATE.md): its own bytes, valid only for the algorithm and the served model that wrote them,
/// so what it learned belongs to that model's class. Rebuildable: a
/// node with nothing saved, or another algorithm's or model's state, starts unlearned (the algorithm seeds
/// itself from its first measurements) and learns again.
pub(super) struct LearnedLane;

/// The facts a learned state holds under: the algorithm that learned it, on which model.
#[derive(serde::Serialize)]
pub(super) struct LaneContract {
    algorithm: &'static str,
    model: String,
}

impl SavedState for LearnedLane {
    const KIND: &'static str = "lane-learned";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one per model this node has served" };
    /// The served model's class: a v5 uuid of its name, so each model keeps what was learned on it.
    type Key = uuid::Uuid;
    type Contract = LaneContract;
    type Value = Vec<u8>;
    type Medium = OwnedFile;
}

impl Rebuildable for LearnedLane {}

/// The lane's state, owned by the driver: the algorithm's decider, the history it reads, and the
/// situation and slot map refilled in place at every step (no allocation past their high-water mark).
pub(super) struct Lanes {
    decider: Decider<Lane<PersonaId>>,
    history: LaneHistory<PersonaId>,
    situation: lane::Situation<PersonaId>,
    /// Ready slot `i` is turn `slots[i]`; the running mind's turn is `running`.
    slots: Vec<usize>,
    running: Option<usize>,
    /// The last decision, when it was made, and the tokens the mind it stepped had left in her phase
    /// then, closed with its outcome at the next decision.
    open: Option<(DecisionId, Instant, Measured<u64>)>,
    /// Tokens the Core moved since the open decision: its outcome's throughput, the reward a learner reads.
    moved: u64,
    /// Of the time since the open decision, what bringing a mind in took: its outcome's transition time.
    switching: Duration,
    /// How many tokens the step just picked may move (`lane::step_tokens`): a prefill chunk is no
    /// longer than every other mind's slack allows.
    tokens: u64,
    /// Where what the algorithm learned is kept, once the node's home is known, and under what.
    learned: Option<(Store<LearnedLane>, uuid::Uuid, LaneContract)>,
    algorithm: &'static str,
}

impl Lanes {
    /// The lane running the algorithm registered as `algorithm`; refused when no such one exists.
    pub(super) fn new(algorithm: &'static str) -> Result<Self, Unknown> {
        let policy = lane::algorithms::<PersonaId>()?.create(algorithm)?;
        Ok(Self {
            decider: Decider::new(policy, lane::DECISIONS), // cold: the lane is built once, with the driver
            history: LaneHistory::new(Views::default()),
            situation: lane::Situation { running: None, ready: Vec::with_capacity(SLOTS), batch_width: 1 }, // cold: built once; refilled in place
            slots: Vec::with_capacity(SLOTS), // cold: built once; refilled in place
            running: None,
            open: None,
            moved: 0,
            switching: Duration::ZERO,
            tokens: 1,
            learned: None,
            algorithm,
        })
    }

    /// Resume what the algorithm learned on this node for `model`, kept in `home`. Nothing saved, or
    /// another algorithm's or model's state, starts unlearned (the store has probed what it
    /// found); bytes the algorithm refuses are probed and it starts there too.
    pub(super) fn resume(&mut self, home: &Path, model: &str) {
        let store = Store::<LearnedLane>::at(home.join(LearnedLane::KIND)); // cold: once, at start
        let class = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, model.as_bytes());
        let contract = LaneContract { algorithm: self.algorithm, model: model.to_owned() }; // cold: once, at start
        let saved = store.load_rebuildable(&class, &contract);
        if let Err(refused) = self.decider.load(&saved) {
            continuum_base::probe!(class = "persona.lane.learned_refused", algorithm = self.algorithm, refused = ?refused, "the lane's saved state was refused; it learns again from its start");
        }
        self.learned = Some((store, class, contract));
    }

    /// Keep what the algorithm has learned, at the stop. A failed save is probed by the store and the
    /// next stop saves afresh.
    pub(super) fn save(&self) {
        let Some((store, class, contract)) = &self.learned else { return };
        let mut state = Vec::new(); // cold: once, at the stop
        self.decider.save(&mut state);
        store.save_rebuildable(class, contract, &state);
    }

    /// How many tokens the step the lane just picked may move.
    pub(super) fn step_tokens(&self) -> usize {
        usize::try_from(self.tokens).unwrap_or(usize::MAX) // unwrap_or: a count past the address space is no limit
    }

    /// What the driver measured, as the lane's executor.
    pub(super) fn observe(&mut self, observation: Observation<PersonaId>) {
        match observation {
            Observation::Stepped { tokens, .. } => self.moved += tokens,
            Observation::SwitchedIn { took, .. } => self.switching += took,
            _ => {}
        }
        self.history.observe(observation);
    }
}

/// What the lane sees of one turn's mind now: her turn's own facts, then her history's measurements.
fn mind(turn: &Turn, history: &LaneHistory<PersonaId>) -> Mind<PersonaId> {
    let phase = match turn.phase {
        Phase::Prefilling { pos } => lane::Phase::Prefill { remaining: turn.prompt.len().saturating_sub(pos) as u64 },
        Phase::Waiting => lane::Phase::Prefill { remaining: turn.prompt.len() as u64 },
        Phase::Decoding { .. } => lane::Phase::Decode,
    };
    let mut mind = Mind {
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
    };
    history.fill(&mut mind, Fraction::HALF);
    mind
}

impl Driver<'_, '_> {
    /// The turn the next step advances, or `None` with no turns. A mind is one serial queue, so only
    /// her oldest turn is offered.
    pub(super) fn pick(&mut self, turns: &[Turn]) -> Option<usize> {
        let active = self.engine.active();
        let most = self.batch.get() as u64;
        let lanes = &mut self.lane;
        lanes.tokens = most;
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
                situation.running = Some(mind(turn, &lanes.history));
                lanes.running = Some(index);
            } else if situation.ready.len() < SLOTS {
                situation.ready.push(mind(turn, &lanes.history));
                lanes.slots.push(index);
            }
        }
        // the last step's outcome: its engine time and the longest wait any ready mind has now, against
        // that mind's own bound
        if let Some((id, at, left)) = lanes.open.take() {
            let (worst_wait, worst_deadline) = situation.ready.iter().max_by_key(|mind| mind.waited).map_or((Duration::ZERO, Duration::MAX), |mind| (mind.waited, mind.deadline)); // map_or: no ready mind means no wait against any bound
            let moved = std::mem::take(&mut lanes.moved);
            let outcome = lane::Outcome { tokens: Measured::Value(moved), engine: now.saturating_duration_since(at), worst_wait, worst_deadline, switching: std::mem::take(&mut lanes.switching), served: lane::served(moved, left) };
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
        let stepped = match action {
            lane::Action::Continue => lanes.situation.running.as_ref(),
            lane::Action::Switch(slot) => lanes.situation.ready.get(usize::from(slot)),
            lane::Action::Batch(_) | lane::Action::Idle => None,
        };
        lanes.open = Some((id, now, stepped.map_or(Measured::Unmeasured, |mind| mind.remaining))); // map_or: a batch or idle steps no one mind
        if let Some(mind) = stepped {
            lanes.tokens = lane::step_tokens(&lanes.situation, mind, most);
        }
        match action {
            lane::Action::Continue => lanes.running,
            lane::Action::Switch(slot) => lanes.slots.get(usize::from(slot)).copied(),
            lane::Action::Batch(_) | lane::Action::Idle => lanes.running.or(lanes.slots.first().copied()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the lane's learned state not keeping the saved-state contract (absent, matched,
    // another model's state, corrupt), so a restart would resume what was learned on another model.
    #[test]
    fn the_learned_lane_keeps_every_saved_state_outcome() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let live = LaneContract { algorithm: "cost-model", model: "qwen3.6-35b-a3b".to_owned() };
        let other = LaneContract { algorithm: "cost-model", model: "qwen2.5-0.5b".to_owned() };
        continuum_saved_state::round_trip::every_outcome::<LearnedLane>(dir.path(), uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, live.model.as_bytes()), live, Some(other), vec![1, 2, 3]);
    }
}
