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
//! Its life is its owner's: started on an `Owner`, stopped when the owner drops.

use std::sync::Arc;


use continuum_sage::component::{Bus, Component, Ctx, Key, Source, Taken, Urgency, Wiring};
use continuum_sage::dataflow::{Event, Snapshot, TopicName};
use continuum_sage::Qos;

use super::cleaner::{Seated, SEATED};
use super::{OwnedKind, TrainingTriggerState};
use crate::inference::learning::{Said, LEARNING_SAID};
use crate::inference::llama_server::{ServingSnapshot, SERVING_TOPIC};
use crate::resources::{Free, FREE};

/// The trigger's arrivals, as state: the sequence of the newest accepted example.
pub const ARRIVALS: TopicName = TopicName("trainer/arrivals");

/// The newest accepted example's sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrivals(pub u64);

// derived-or-floor: a seat has at most two answers outstanding (one seat state for its one seat
// command, one step or generation for its one lesson or checkpoint: seat_run tells no more), so 1,024
// holds every answer of 512 seats; a node seats one per learning bucket (residents times traits,
// tens). A dropped Said would leave a seat waiting, so the window must never be what bounds them.
const SAID_WINDOW: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN.saturating_add(1_023);

/// What wakes the trainer.
pub enum Wake {
    /// Serving changed: an engine came, went or changed.
    Serving(Arc<ServingSnapshot>),
    /// An example was accepted.
    Arrived(u64),
    /// The node engine said what became of one of the trainer's seats.
    Said(Said),
    /// The resource authority said what is free: a bucket held for memory may seat now.
    Free(Arc<Free>),
}

impl From<Snapshot<Free>> for Wake {
    fn from(snapshot: Snapshot<Free>) -> Self {
        Wake::Free(snapshot.value)
    }
}

impl From<Event<Said>> for Wake {
    fn from(event: Event<Said>) -> Self {
        Wake::Said(event.value.as_ref().clone())
    }
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

/// What the trigger says on the trainer's bus, claimed when the trainer is built: its arrivals.
/// (It also says once that it holds no shadow file: in-process seats keep their shadows in the
/// engine's memory, so every shadow file in the train dir is a leftover the cleaner may release.)
pub(crate) struct TrainerOut {
    arrivals: Source<Arrivals>,
    /// Held for the trainer's life, so what it said (no shadow file held) stays said.
    _no_shadows: Source<Seated>,
}

impl TrainerOut {
    /// Say that the newest accepted example is `sequence`.
    pub(crate) fn arrived(&mut self, sequence: u64) {
        self.arrivals.say(Arrivals(sequence));
    }
}

/// The trainer: one recovery pass per wake, on its own worker.
pub struct Trainer {
    state: Arc<TrainingTriggerState>,
}

impl Trainer {
    /// The trainer for `state` on `bus`. It claims the arrivals topic there and hands it to the
    /// trigger, which says the current sequence at once, so the first wake recovers whatever was
    /// already waiting at boot. A second trainer on one bus is refused.
    pub fn on(state: Arc<TrainingTriggerState>, bus: &Bus) -> Result<Self, Taken> {
        let mut no_shadows = Source::on(bus, SEATED, Key::ONE, "training-trigger")?;
        no_shadows.say(Seated(std::collections::BTreeSet::new()));
        let out = TrainerOut { arrivals: Source::on(bus, ARRIVALS, Key::ONE, "training-trigger")?, _no_shadows: no_shadows };
        state.install_out(out);
        Ok(Self { state })
    }
}

impl Component for Trainer {
    type Input = Wake;
    const NAME: &'static str = "trainer";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<ServingSnapshot>(SERVING_TOPIC, Key::ONE);
        wiring.watch::<Arrivals>(ARRIVALS, Key::ONE);
        wiring.watch::<Free>(FREE, Key::ONE);
        // the engine says on the node bus; every Said is kept until taken, in order (one per command
        // the trainer told, so the window bounds nothing the trainer did not ask for)
        let node = continuum_sage::component::node();
        wiring.hear_on::<Said>(&node, LEARNING_SAID, SAID_WINDOW);
    }

    async fn handle(&mut self, wake: Wake, ctx: &mut Ctx<Self>) {
        ctx.need(Urgency::Want);
        let woke_by = match wake {
            Wake::Serving(_) => "serving",
            Wake::Arrived(_) => "arrival",
            Wake::Said(said) => {
                let state = Arc::clone(&self.state);
                // owned, so shutdown's join waits for a seat's act in flight
                if let Err(refused) = self.state.run_operation(OwnedKind::Settle, None, async move { state.heard_from_engine(said).await }).await {
                    continuum_base::probe!(class = "training.on_engine.refused", refused = %refused, "a Said arrived as the trigger stopped; its seat is settled at shutdown");
                }
                ctx.need(Urgency::Content);
                return;
            }
            Wake::Free(free) => {
                let state = Arc::clone(&self.state);
                // only the buckets held for memory are looked at: never a whole pass per board change
                if let Err(refused) = self.state.run_operation(OwnedKind::Replan, None, async move { state.heard_free(&free).await }).await {
                    continuum_base::probe!(class = "training.on_engine.refused", refused = %refused, "a free-memory change arrived as the trigger stopped");
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

    /// The trainer's handled count once it reaches `at_least`: the first count at or past it, so a
    /// pass nothing asked for shows as a count past the one expected.
    async fn handled(bus: &Bus, at_least: u64) -> u64 {
        let mut telemetry = bus.reader::<Telemetry>(TELEMETRY, presence_key(Trainer::NAME));
        loop {
            if let Some(now) = telemetry.latest().filter(|t| t.value.handled >= at_least) {
                return now.value.handled;
            }
            telemetry.changed().await.expect("test: the bus outlives the trainer");
        }
    }

    // what this catches (the IntelMac 2026-11-23: a 30 s tick re-ran the pass 608 times in 40 min
    // against an engine that admitted no step): a trainer that runs passes nothing asked for, that
    // misses work already waiting at boot, that a second trainer can join on one bus, or that
    // outlives its owner.
    #[tokio::test]
    async fn the_trainer_runs_a_pass_per_change_and_none_on_a_clock() {
        let (trigger, executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let bus = Arc::new(Bus::new());
        let mut owner = Owner::with_bus(Arc::clone(&bus), OwnerId(uuid::Uuid::new_v4()), 0);
        let teller = owner.start(trigger.trainer(&bus).expect("test: the first trainer on this bus"));
        assert!(trigger.trainer(&bus).is_err(), "a second trainer on one bus is refused");

        assert_eq!(handled(&bus, 1).await, 1, "one pass at boot, for whatever was already waiting");

        let params = serde_json::json!({
            "personaId": uuid::Uuid::new_v4(), "personaName": "trainer-fixture", "baseModel": "qwen", "traitKind": "geography",
            "examples": [crate::commands::training_trigger::test_support::ex("What is the capital of Kansas?", "Topeka.")], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes");
        assert_eq!(answer["success"], true, "test: the arrival is accepted: {answer}");
        assert_eq!(handled(&bus, 2).await, 2, "an accepted example wakes exactly one pass");

        let _serving = crate::inference::llama_server::ServingState::on(&bus, ServingSnapshot::empty()).expect("test: the serving state on this bus");
        assert_eq!(handled(&bus, 3).await, 3, "a serving change wakes exactly one pass");

        drop(owner);
        assert!(!teller.rest().await, "a dropped owner's trainer has stopped and takes nothing more");
    }
}
