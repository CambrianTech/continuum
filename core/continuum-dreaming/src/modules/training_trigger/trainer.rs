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

use std::collections::BTreeSet;
use std::path::PathBuf;

use continuum_sage::component::{Bus, Component, Ctx, Key, Source, Taken, Urgency, Wiring};
use continuum_sage::dataflow::{Emitter, Snapshot, TopicName};
use continuum_sage::Qos;

use super::cleaner::{Seated, Settled, SEATED, SETTLED};
use super::{OwnedKind, TrainingTriggerState};
use crate::inference::llama_server::{ServingSnapshot, SERVING_TOPIC};

/// The trigger's arrivals, as state: the sequence of the newest accepted example.
pub const ARRIVALS: TopicName = TopicName("trainer/arrivals");

/// The newest accepted example's sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrivals(pub u64);

/// What wakes the trainer.
pub enum Wake {
    /// Serving changed: an engine came, went or changed.
    Serving(Arc<ServingSnapshot>),
    /// An example was accepted.
    Arrived(u64),
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

/// What the trigger says on the trainer's bus, claimed when the trainer is built: arrivals, the
/// shadows live seats hold, and each page-out.
pub(crate) struct TrainerOut {
    arrivals: Source<Arrivals>,
    seated: Source<Seated>,
    settled: Emitter<Settled>,
    held: BTreeSet<PathBuf>,
}

impl TrainerOut {
    /// Say the shadows live seats hold now: the one place the held set is said.
    fn say_held(&mut self) {
        self.seated.say(Seated(self.held.clone()));
    }

    /// Say that the newest accepted example is `sequence`.
    pub(crate) fn arrived(&mut self, sequence: u64) {
        self.arrivals.say(Arrivals(sequence));
    }

    /// A seat now holds `shadow`.
    pub(crate) fn seated(&mut self, shadow: PathBuf) {
        self.held.insert(shadow);
        self.say_held();
    }

    /// A seat let `shadow` go outside a page-out (a quarantine): no seat holds it now.
    pub(crate) fn released(&mut self, shadow: PathBuf) {
        self.held.remove(&shadow);
        self.say_held();
    }

    /// A seat let `shadow` go at page-out, having learned `learned` examples and published or not.
    pub(crate) fn settled(&mut self, shadow: PathBuf, learned: u32, published: bool) {
        self.held.remove(&shadow);
        self.say_held();
        self.settled.emit(Settled { shadow, learned, published });
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
        let out = TrainerOut {
            arrivals: Source::on(bus, ARRIVALS, Key::ONE, "training-trigger")?,
            seated: Source::on(bus, SEATED, Key::ONE, "training-trigger")?,
            settled: bus.emitter::<Settled>(SETTLED)?,
            held: BTreeSet::new(),
        };
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
    }

    async fn handle(&mut self, wake: Wake, ctx: &mut Ctx<Self>) {
        ctx.need(Urgency::Want);
        let woke_by = match &wake {
            Wake::Serving(_) => "serving",
            Wake::Arrived(_) => "arrival",
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
