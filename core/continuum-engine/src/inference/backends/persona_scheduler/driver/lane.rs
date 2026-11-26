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
/// The decision awaiting its outcome: its id, when it was made, the tokens the mind it stepped had
/// left in her phase then, and whether that mind was background work.
struct Open {
    id: DecisionId,
    at: Instant,
    left: Measured<u64>,
    background: bool,
}

pub(super) struct Lanes {
    decider: Decider<Lane<PersonaId>>,
    history: LaneHistory<PersonaId>,
    situation: lane::Situation<PersonaId>,
    /// Ready slot `i` is turn `slots[i]`; the running mind's turn is `running`.
    slots: Vec<usize>,
    running: Option<usize>,
    /// The last decision, closed with its outcome at the next decision.
    open: Option<Open>,
    /// Tokens the Core moved since the open decision: its outcome's throughput, the reward a learner reads.
    moved: u64,
    /// Of the time since the open decision, what bringing a mind in took: its outcome's transition time.
    switching: Duration,
    /// How many tokens the step just picked may move (`lane::step_tokens`): a prefill chunk is no
    /// longer than every other mind's slack allows. Never more than the engine's batch, a `usize`.
    tokens: usize,
    /// Her learning jobs' Core time, a job (Measure, Load, Seat, a step): the background class's cost of
    /// one, from the jobs it ran.
    learning_job: continuum_sage::decide::Ewma,
    /// Core time learning jobs took since the last decision: the background class's service.
    learning_served: Duration,
    /// A learning job may run now although turns wait: the background class is owed, and the queue's
    /// room holds one job's measured time (so no act is carried past her bound by it).
    learning_owed: bool,
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
            situation: lane::Situation { running: None, ready: Vec::with_capacity(SLOTS), batch_width: 1, background_owed: false }, // cold: built once; refilled in place
            slots: Vec::with_capacity(SLOTS), // cold: built once; refilled in place
            running: None,
            open: None,
            moved: 0,
            switching: Duration::ZERO,
            tokens: 1,
            learning_job: continuum_sage::decide::Ewma::new(Fraction::QUARTER),
            learning_served: Duration::ZERO,
            learning_owed: false,
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
    /// A learning job may run now (the background class is owed and the queue has room for one).
    pub(super) fn learning_owed(&self) -> bool {
        self.learning_owed
    }

    /// A learning job ran for `took`: the background class's service, and its measured cost a job.
    pub(super) fn learned(&mut self, took: Duration) {
        self.learning_served += took;
        self.learning_job.observe(took.as_secs_f64());
    }

    pub(super) fn step_tokens(&self) -> usize {
        self.tokens
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
        overhead: Measured::Unmeasured,
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
        let most = std::num::NonZeroU64::MIN.saturating_add(self.batch.get() as u64 - 1); // the batch is non-zero, so a step moves at least one token
        let learning_waiting = self.learning.waiting_jobs();
        let lanes = &mut self.lane;
        lanes.tokens = self.batch.get();
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
        // the last step's outcome: its engine time and the longest wait any bounded mind has now,
        // against her own bound; and the background class's account (its backlog now, and the Core
        // time it got under that decision)
        let mut served_background = Duration::ZERO;
        if let Some(Open { id, at, left, background }) = lanes.open.take() {
            let worst = situation.ready.iter().filter_map(|mind| mind.deadline.map(|bound| (mind.waited, bound))).max_by_key(|&(waited, _)| waited);
            let (worst_wait, worst_deadline) = worst.unwrap_or((Duration::ZERO, Duration::MAX)); // unwrap_or: no bounded mind waiting is no wait against any bound
            let moved = std::mem::take(&mut lanes.moved);
            let engine = now.saturating_duration_since(at);
            if background {
                served_background = engine;
            }
            let outcome = lane::Outcome { tokens: Measured::Value(moved), engine, worst_wait, worst_deadline, switching: std::mem::take(&mut lanes.switching), served: lane::served(moved, left) };
            let _ = lanes.decider.outcome(id, outcome);
        }
        // the background class: her dreams' predicted remaining Core time, and the learning jobs waiting at
        // their measured cost a job; served, what each took since the last decision
        let job = lanes.learning_job.mean().value().and_then(|secs| Duration::try_from_secs_f64(secs).ok());
        let learning_backlog = job.map_or(Duration::ZERO, |job| job.mul_f64(learning_waiting as f64)); // map_or: a job not yet measured is a cost not yet seen
        let dreams: Duration = situation.running.iter().chain(situation.ready.iter()).filter(|mind| mind.deadline.is_none()).filter_map(|mind| mind.remaining.value().zip(mind.step.value()).map(|(left, step)| step.mul_f64(left as f64))).sum();
        let backlog = dreams + learning_backlog;
        served_background += std::mem::take(&mut lanes.learning_served);
        situation.background_owed = lanes.history.background(backlog, served_background);
        // learning first in its class (her learning through the day; dreams are the occasional part),
        // when the queue's room holds one job so no act is carried past her bound by it
        // (a job not yet measured is a cost not yet seen: the first runs as soon as it waits, and measures it)
        let fits = lane::room(situation, situation.running.as_ref().map(|running| &running.mind)).is_none_or(|room| room >= job.unwrap_or(Duration::ZERO)); // unwrap_or: a job not yet measured is a cost not yet seen
        let owed = learning_waiting > 0 && (situation.background_owed || job.is_none()) && fits;
        if owed && !lanes.learning_owed && !(situation.ready.is_empty() && situation.running.is_none()) {
            continuum_base::probe!(class = "persona.lane.learning_owed", waiting_jobs = learning_waiting as u64, backlog_ms = backlog.as_millis() as u64,
                job_ms = ?job.map(|job| job.as_millis()), "learning is owed a step while turns wait: its queue would grow"); // job_ms: None until a job is measured
        }
        lanes.learning_owed = owed;
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
        lanes.open = Some(Open { id, at: now, left: stepped.map_or(Measured::Unmeasured, |mind| mind.remaining), background: stepped.is_some_and(|mind| mind.deadline.is_none()) }); // map_or: a batch or idle steps no one mind
        if let Some(mind) = stepped {
            lanes.tokens = lane::step_tokens(&lanes.situation, mind, most) as usize; // lossless: at most `most`, the batch, itself a usize
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
