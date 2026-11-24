//! Who steps next: the SAGE lane (`continuum_sage::Lane`, CONTINUUM-ENGINE 4.15), never a whole-turn
//! pick. Every mind with a turn publishes her current need and that she has decoding to do; at each step
//! boundary the lane reads those states, without waiting on anyone, and its slicer picks the mind that
//! steps, a control law climbing measured throughput under the wait bound (Joel, 2026-11-22: "no mind
//! ever waits more than 1-2 s, and the ideal is under a second"; "It's effectively ALL simultaneous ...
//! They don't notice"). A switch between seated minds moves nothing, so the lane may switch at any step.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::component::Need;
use continuum_sage::dataflow::{state, OwnerId, Provenance, Publisher, RunId, StageId, StageIncarnation, TopicName};
use continuum_sage::{Decoding, Lane, Next, Policy, Slicer, Turn};

use super::super::Incarnation;

/// What the lane is held to: no ready mind waits past `wait_bound` for her next step. `smoothing` is
/// the newest sample's weight in each measured mean, the slice module's own setting in its sim, its lane
/// and its tests, so live and replay follow measurements alike.
fn policy() -> Policy {
    Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 }
}

/// A mind's state as the lane reads it, published by the engine that holds her turns.
struct Presence {
    need: Publisher<Need>,
    decoding: Publisher<Decoding>,
}

/// The engine's lane and the state of every mind that has stepped on it.
pub(super) struct Lanes {
    lane: Lane<PersonaId>,
    minds: HashMap<PersonaId, Presence>,
    holder: Option<PersonaId>,
    /// The lane's run: every state it publishes names it.
    run: RunId,
}

impl Lanes {
    pub(super) fn new(incarnation: Incarnation) -> Self {
        let run = RunId { incarnation: StageIncarnation { stage: StageId::named("persona.lane"), owner: OwnerId(uuid::Uuid::nil()), epoch: incarnation.get() }, seq: 0 };
        // who holds the lane, published for whoever watches it; never waited on
        let (turn, _watchers) = state::<Turn<PersonaId>>(TopicName("persona/lane/turn"));
        Self { lane: Lane::new(Slicer::new(policy()), turn, by(run)), minds: HashMap::new(), holder: None, run }
    }

    /// She has `waiting` turns to run at `need`'s urgency: the lane counts her as ready.
    pub(super) fn ready(&mut self, mind: PersonaId, need: Need) {
        let run = self.run;
        let presence = self.minds.entry(mind).or_insert_with(|| {
            let (need, need_reader) = state::<Need>(TopicName("persona/lane/need"));
            let (decoding, decoding_reader) = state::<Decoding>(TopicName("persona/lane/decoding"));
            self.lane.seat(mind, need_reader, decoding_reader);
            Presence { need, decoding }
        });
        presence.need.publish(need, by(run));
        presence.decoding.publish(Decoding(true), by(run));
    }

    /// She leaves for good (a one-off whose only turn ended, however it ended): her seat, her state and
    /// what the slicer learned of her go, so the lane tracks only minds that may step again.
    pub(super) fn forget(&mut self, mind: PersonaId) {
        self.lane.unseat(&mind);
        self.minds.remove(&mind);
        if self.holder == Some(mind) {
            self.holder = None;
        }
    }

    /// Her last turn ended: she has nothing to decode until her next one.
    pub(super) fn rested(&mut self, mind: PersonaId) {
        if let Some(presence) = self.minds.get_mut(&mind) {
            presence.decoding.publish(Decoding(false), by(self.run));
        }
    }

    /// The mind that steps next, or `None` when no mind has a turn.
    pub(super) fn next(&mut self) -> Option<PersonaId> {
        match self.lane.next(Instant::now()) {
            Next::Idle => self.holder = None,
            Next::Switch(mind) => self.holder = Some(mind),
            Next::Continue => {}
        }
        self.holder
    }

    /// She stepped once on the engine; bringing her onto it took `switched` (zero when she was active).
    pub(super) fn stepped(&mut self, mind: PersonaId, took: Duration, switched: Option<Duration>) {
        if let Some(switched) = switched {
            self.lane.switched(&mind, switched);
        }
        self.lane.stepped(&mind, took);
    }

    /// Tell the lane exactly which minds have turns: each with her need, every other mind it knows as
    /// resting.
    fn sync(&mut self, ready: &HashMap<PersonaId, Need>) {
        let resting: Vec<PersonaId> = self.minds.keys().filter(|mind| !ready.contains_key(mind)).copied().collect();
        for mind in resting {
            self.rested(mind);
        }
        for (mind, need) in ready {
            self.ready(*mind, *need);
        }
    }
}

/// Her need across her waiting turns: the most urgent of them, and how many wait.
pub(super) fn need_of(mind: PersonaId, turns: &[super::Turn]) -> Need {
    turns.iter().filter(|turn| turn.id == mind).fold(Need::default(), |need, turn| Need { urgency: need.urgency.max(turn.req.urgency), waiting: need.waiting + 1 })
}

impl super::Driver<'_, '_> {
    /// The lane picked nobody, or a mind with no turn, while turns wait: it was not told what the turns
    /// are. Said loudly, and the lane is told every mind's state again, so the loop never spins on it.
    pub(super) fn lane_disagrees(&mut self, turns: &[super::Turn]) {
        let ready: HashMap<PersonaId, Need> = turns.iter().map(|turn| (turn.id, need_of(turn.id, turns))).collect();
        continuum_base::probe!(class = "persona.lane.disagrees", turns = turns.len() as u64, minds = ready.len() as u64, "the lane's states disagreed with the waiting turns; each mind was told again");
        self.lanes.sync(&ready);
    }
}

/// A publication by the lane's run, from no input it read.
fn by(run: RunId) -> Provenance {
    Provenance { by: run, from: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_sage::component::Urgency;

    // what this catches (Codex on #6258 at 717bf9eca2): every anonymous turn gets a fresh id, so a lane
    // that only marks a finished mind as resting keeps her seat, her state and the slicer's history of
    // her forever, and scans them all at every step. After a thousand one-offs come and go the lane
    // tracks nobody, and a resident asking after them still steps.
    #[test]
    fn one_offs_leave_nothing_behind_and_a_later_resident_still_steps() {
        let mut lanes = Lanes::new(Incarnation::next());
        let want = Need { urgency: Urgency::Want, waiting: 1 };
        for _ in 0..1_000 {
            let one_off = PersonaId(uuid::Uuid::new_v4());
            lanes.ready(one_off, want);
            assert_eq!(lanes.next(), Some(one_off), "the one-off steps");
            lanes.stepped(one_off, Duration::from_millis(20), Some(Duration::ZERO));
            lanes.forget(one_off);
        }
        assert_eq!((lanes.minds.len(), lanes.lane.seated(), lanes.lane.tracked()), (0, 0, 0), "nothing of them is tracked");
        let resident = PersonaId(uuid::Uuid::new_v4());
        lanes.ready(resident, want);
        assert_eq!(lanes.next(), Some(resident), "a later resident still steps");
    }
}
