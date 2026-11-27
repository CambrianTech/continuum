//! The in-place trainer as a SAGE component (card 54731a51): its own worker, woken by what changed,
//! never by a clock. It hears the serving state (`serving/snapshot`: an engine came or went) and the
//! trigger's arrivals (`trainer/arrivals`: an example was accepted). Each wake runs one recovery pass;
//! changes heard while a pass runs coalesce into the next, so passes never run beside each other.
//!
//! It replaces the 30 s ServiceModule tick and the hand-built serving wake (a task following a watch,
//! with its own mutex, alive flag and join). On the IntelMac (2026-11-23) the tick re-ran the pass
//! against an engine that admitted no step, 608 times in 40 minutes. A pass with no learning engine
//! says so once and the trainer rests until serving changes or an example arrives.
//!
//! What the engine and the authority say of its seats is heard by the learner (`learner.rs`), its own
//! component, never on this worker: a pass would hold every answer until it ends (card d1e5675c).
//!
//! Its life is its owner's: started on an `Owner`, stopped when the owner drops.

use std::sync::Arc;

use continuum_sage::component::{Bus, Component, Ctx, Key, Source, Taken, Urgency, Wiring};
use continuum_sage::dataflow::{Emitter, Event, Snapshot, TopicName};
use continuum_sage::Qos;

use super::cleaner::{Seated, SEATED};
use super::learner::Learner;
use super::on_engine::FollowUp;
use super::{OwnedKind, TrainingTriggerState};
use crate::inference::learning::{Owed, LEARNING_OWED};
use crate::inference::llama_server::{ServingSnapshot, SERVING_TOPIC};

/// The trigger's arrivals, as state: the sequence of the newest accepted example.
pub const ARRIVALS: TopicName = TopicName("trainer/arrivals");

/// The newest accepted example's sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrivals(pub u64);

/// What a seat's answer leaves for the trainer's worker, said by the learner (card d1e5675c).
pub(crate) const FOLLOW_UPS: TopicName = TopicName("trainer/follow-ups");

// derived-or-floor: each follow-up comes from one engine answer (at most 1,024 outstanding: the
// learner's Said window) or one bucket held for memory (residents times traits, tens), so 1,024 holds
// a burst of them. One past it is not lost work: the next pass seats every waiting bucket and sweeps
// every generation.
const FOLLOW_UP_WINDOW: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN.saturating_add(1_023);

/// What wakes the trainer.
pub enum Wake {
    /// Serving changed: an engine came, went or changed.
    Serving(Arc<ServingSnapshot>),
    /// An example was accepted.
    Arrived(u64),
    /// A seat's answer left a bucket's follow-up for this worker.
    FollowUp(FollowUp),
}

impl From<Snapshot<ServingSnapshot>> for Wake {
    fn from(snapshot: Snapshot<ServingSnapshot>) -> Self {
        Wake::Serving(snapshot.value)
    }
}

impl From<Snapshot<Arrivals>> for Wake {
    fn from(snapshot: Snapshot<Arrivals>) -> Self {
        Wake::Arrived(snapshot.value.0)
    }
}

impl From<Event<FollowUp>> for Wake {
    fn from(event: Event<FollowUp>) -> Self {
        Wake::FollowUp(event.value.as_ref().clone())
    }
}

/// What the trigger says on the trainer's bus, claimed when the trainer is built: its arrivals.
/// (It also says once that it holds no shadow file: in-process seats keep their shadows in the
/// engine's memory, so every shadow file in the train dir is a leftover the cleaner may release.)
pub(crate) struct TrainerOut {
    arrivals: Source<Arrivals>,
    /// The learner's hand-off to the trainer's worker.
    follow_ups: Emitter<FollowUp>,
    /// What every seat owes (`learning/owed`), and what it last said: said again only when it changes.
    owed: Source<Owed>,
    said_owed: Owed,
    /// Held for the trainer's life, so what it said (no shadow file held) stays said.
    _no_shadows: Source<Seated>,
}

impl TrainerOut {
    /// Say that the newest accepted example is `sequence`.
    pub(crate) fn arrived(&mut self, sequence: u64) {
        self.arrivals.say(Arrivals(sequence));
    }

    /// Hand `follow_up` to the trainer's worker.
    pub(crate) fn follow_up(&mut self, follow_up: FollowUp) {
        self.follow_ups.emit(follow_up);
    }

    /// Say what every seat owes, when it differs from what was last said.
    pub(crate) fn owe(&mut self, owed: Owed) {
        if owed != self.said_owed {
            self.owed.say(owed.clone());
            self.said_owed = owed;
        }
    }
}

/// The node's in-place learning parts, each started on the node's owner on its own worker: the
/// trainer's passes and the learner's acts never wait on each other's worker.
pub struct LearningParts {
    pub trainer: Trainer,
    pub learner: Learner,
}

/// The trainer: one recovery pass per wake, and each follow-up the learner hands it, on its own worker.
pub struct Trainer {
    state: Arc<TrainingTriggerState>,
}

impl Trainer {
    /// The trainer and its learner for `state` on `bus`. It claims the arrivals topic there and hands
    /// it to the trigger, which says the current sequence at once, so the first wake recovers whatever
    /// was already waiting at boot. A second trainer on one bus is refused.
    pub fn on(state: Arc<TrainingTriggerState>, bus: &Bus) -> Result<LearningParts, Taken> {
        let mut no_shadows = Source::on(bus, SEATED, Key::ONE, "training-trigger")?;
        no_shadows.say(Seated(std::collections::BTreeSet::new()));
        let mut owed = Source::on(bus, LEARNING_OWED, Key::ONE, "training-trigger")?;
        owed.say(Owed::default()); // nothing owed until a seat says otherwise
        let arrivals = Source::on(bus, ARRIVALS, Key::ONE, "training-trigger")?;
        let out = TrainerOut { arrivals, follow_ups: bus.emitter::<FollowUp>(FOLLOW_UPS)?, owed, said_owed: Owed::default(), _no_shadows: no_shadows };
        state.install_out(out);
        Ok(LearningParts { learner: Learner { state: Arc::clone(&state) }, trainer: Self { state } })
    }
}

impl Component for Trainer {
    type Input = Wake;
    const NAME: &'static str = "trainer";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<ServingSnapshot>(SERVING_TOPIC, Key::ONE);
        wiring.watch::<Arrivals>(ARRIVALS, Key::ONE);
        wiring.hear::<FollowUp>(FOLLOW_UPS, FOLLOW_UP_WINDOW);
    }

    async fn handle(&mut self, wake: Wake, ctx: &mut Ctx<Self>) {
        ctx.need(Urgency::Want);
        let woke_by = match wake {
            Wake::Serving(_) => "serving",
            Wake::Arrived(_) => "arrival",
            Wake::FollowUp(follow_up) => {
                let state = Arc::clone(&self.state);
                let persona = match &follow_up {
                    FollowUp::Seat(key) | FollowUp::Sweep(key) => key.persona_id,
                };
                // owned, so shutdown's join waits for a seating or a sweep in flight, and names it
                let taken = self.state.run_operation(OwnedKind::FollowUp, Some(persona), async move {
                    match super::on_engine::port() {
                        Some(port) => state.follow_up(&port, follow_up).await,
                        None => continuum_base::probe!(class = "training.on_engine.no_port", follow_up = ?follow_up,
                            "a follow-up arrived with no node engine installed: the next pass takes it"),
                    }
                });
                if let Err(refused) = taken.await {
                    continuum_base::probe!(class = "training.on_engine.refused", refused = %refused, "a follow-up arrived as the trigger stopped; the next start's pass takes it");
                }
                ctx.need(Urgency::Content);
                return;
            }
        };
        let state = Arc::clone(&self.state);
        // an owned operation, so shutdown's join waits for a pass in flight and refuses one after it
        let pass = self.state.run_operation(OwnedKind::Recover, None, async move { state.recover_pass().await }).await;
        match pass {
            Ok(Ok(())) => {}
            Ok(Err(error)) => continuum_base::probe!(class = "training.pass.failed", woke_by, error = %error,
                "a recovery pass failed; durable work stays owned and the next wake tries again"),
            Err(refused) => continuum_base::probe!(class = "training.pass.refused", woke_by, refused = %refused,
                "a recovery pass was not admitted or lost its result; the next wake tries again"),
        }
        ctx.need(Urgency::Content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_sage::component::{presence_key, Owner, Telemetry, TELEMETRY};
    use continuum_sage::dataflow::OwnerId;

    /// Component `name`'s handled count once it reaches `at_least`: the first count at or past it, so
    /// a wake nothing asked for shows as a count past the one expected.
    async fn handled(bus: &Bus, name: &'static str, at_least: u64) -> u64 {
        let mut telemetry = bus.reader::<Telemetry>(TELEMETRY, presence_key(name));
        loop {
            if let Some(now) = telemetry.latest().filter(|t| t.value.handled >= at_least) {
                return now.value.handled;
            }
            telemetry.changed().await.expect("test: the bus outlives the component");
        }
    }

    // what this catches (the IntelMac 2026-11-23: a 30 s tick re-ran the pass 608 times in 40 min
    // against an engine that admitted no step; the 5090 2026-11-27, card d1e5675c: 12 engine answers
    // waited 7.7 min behind one pass on the trainer's worker): a trainer that runs passes nothing asked
    // for, that misses work already waiting at boot, that a second trainer can join on one bus, or that
    // outlives its owner; and an engine answer handled on the trainer's worker instead of the learner's.
    #[tokio::test]
    async fn the_trainer_runs_a_pass_per_change_and_none_on_a_clock() {
        let (trigger, executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let bus = Arc::new(Bus::new());
        let mut owner = Owner::with_bus(Arc::clone(&bus), OwnerId(uuid::Uuid::new_v4()), 0);
        let learning = trigger.trainer(&bus).expect("test: the first trainer on this bus");
        let teller = owner.start(learning.trainer);
        let _learner = owner.start(learning.learner);
        assert!(trigger.trainer(&bus).is_err(), "a second trainer on one bus is refused");

        assert_eq!(handled(&bus, Trainer::NAME, 1).await, 1, "one pass at boot, for whatever was already waiting");

        let params = serde_json::json!({
            "personaId": uuid::Uuid::new_v4(), "personaName": "trainer-fixture", "baseModel": "qwen", "traitKind": "geography",
            "examples": [crate::commands::training_trigger::test_support::ex("What is the capital of Kansas?", "Topeka.")], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes");
        assert_eq!(answer["success"], true, "test: the arrival is accepted: {answer}");
        assert_eq!(handled(&bus, Trainer::NAME, 2).await, 2, "an accepted example wakes exactly one pass");

        let _serving = crate::inference::llama_server::ServingState::on(&bus, ServingSnapshot::empty()).expect("test: the serving state on this bus");
        assert_eq!(handled(&bus, Trainer::NAME, 3).await, 3, "a serving change wakes exactly one pass");

        // the engine answers on the node bus: the learner hears it, the trainer's worker never does
        let mut engine = continuum_sage::component::node().emitter::<crate::inference::learning::Said>(crate::inference::learning::LEARNING_SAID).expect("test: the one learning/said producer");
        let seat = crate::inference::learning::SeatId(uuid::Uuid::new_v4());
        engine.emit(crate::inference::learning::Said::Seat { seat, state: crate::inference::learning::SeatState::Released });
        // waits on the learner's own count: the answer reaches it, or this never returns
        assert_eq!(handled(&bus, Learner::NAME, 1).await, 1, "the learner takes the engine's answer on its own worker");
        let trainer_now = bus.reader::<Telemetry>(TELEMETRY, presence_key(Trainer::NAME)).latest().map(|t| t.value.handled);
        assert_eq!(trainer_now, Some(3), "the engine's answer woke no pass");

        drop(owner);
        assert!(!teller.rest().await, "a dropped owner's trainer has stopped and takes nothing more");
    }
}
