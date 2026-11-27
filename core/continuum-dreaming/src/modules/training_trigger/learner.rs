//! The learner: the trainer's ear on the node engine, a SAGE component on its own worker (card
//! d1e5675c). It hears what the engine said of each of the trainer's seats (`learning/said`) and what
//! the resource authority says is free (`resources/free`), and takes each seat's next act at once.
//!
//! It is its own component, not a wake of the trainer's, because a recovery pass seats every bucket
//! before it rests, and a worker runs one handler at a time: on one worker, every answer queued
//! behind the whole pass (the 5090, #6287's window at 327d4e5fa: 12 measured answers waited 7.7 min,
//! handled 50 ms after the pass's last decision).
//! The learner and the pass meet only at each bucket's submit gate and the seats lock, so an answer
//! waits for a pass only while that pass works on the answer's own bucket.
//!
//! What an act leaves that is long (seating a bucket again, which decides its competence; offering a
//! committed generation) is handed to the trainer's worker as a [`FollowUp`], never taken here
//! (Codex's review): the learner records the transition, the trainer does the work under the same gate.
//!
//! [`FollowUp`]: super::on_engine::FollowUp

use std::sync::Arc;

use continuum_sage::component::{Component, Ctx, Key, Urgency, Wiring};
use continuum_sage::dataflow::{Event, Snapshot};
use continuum_sage::Qos;

use super::{OwnedKind, TrainingTriggerState};
use crate::inference::learning::{Said, LEARNING_SAID};
use crate::resources::{Free, FREE};

// derived-or-floor: a seat has at most two answers outstanding (one seat state for its one seat
// command, one step or generation for its one lesson or checkpoint: seat_run tells no more), so 1,024
// holds every answer of 512 seats; a node seats one per learning bucket (residents times traits,
// tens). A dropped Said would leave a seat waiting, so the window must never be what bounds them.
const SAID_WINDOW: std::num::NonZeroUsize = std::num::NonZeroUsize::MIN.saturating_add(1_023);

/// What wakes the learner.
pub enum Heard {
    /// The node engine said what became of one of the trainer's seats.
    Said(Said),
    /// The resource authority said what is free: a bucket held for memory may seat now.
    Free(Arc<Free>),
}

impl From<Event<Said>> for Heard {
    fn from(event: Event<Said>) -> Self {
        Heard::Said(event.value.as_ref().clone())
    }
}

impl From<Snapshot<Free>> for Heard {
    fn from(snapshot: Snapshot<Free>) -> Self {
        Heard::Free(snapshot.value)
    }
}

/// The learner: each seat's next act, as soon as the engine or the authority says it may be taken.
pub struct Learner {
    pub(super) state: Arc<TrainingTriggerState>,
}

impl Component for Learner {
    type Input = Heard;
    const NAME: &'static str = "learner";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Free>(FREE, Key::ONE);
        // the engine says on the node bus; every Said is kept until taken, in order (one per command
        // the trainer told, so the window bounds nothing the trainer did not ask for)
        let node = continuum_sage::component::node();
        wiring.hear_on::<Said>(&node, LEARNING_SAID, SAID_WINDOW);
    }

    async fn handle(&mut self, heard: Heard, ctx: &mut Ctx<Self>) {
        ctx.need(Urgency::Want);
        let state = Arc::clone(&self.state);
        // owned, so shutdown's join waits for a seat's act in flight
        let followed = match heard {
            Heard::Said(said) => self.state.run_operation(OwnedKind::Recover, None, async move { state.heard_from_engine(said).await.into_iter().collect::<Vec<_>>() }).await,
            // only the buckets held for memory are looked at: never a whole pass per board change
            Heard::Free(free) => self.state.run_operation(OwnedKind::Recover, None, async move { state.heard_free(&free).await }).await,
        };
        match followed {
            // what the act left (a bucket to seat again, a candidate to offer) is long: the trainer's
            Ok(follow_ups) => {
                for follow_up in follow_ups {
                    self.state.say(|out| out.follow_up(follow_up));
                }
            }
            Err(refused) => continuum_base::probe!(class = "training.on_engine.refused", refused = %refused,
                "the engine or the authority spoke as the trigger stopped; its seat is settled at shutdown"),
        }
        ctx.need(Urgency::Content);
    }
}
