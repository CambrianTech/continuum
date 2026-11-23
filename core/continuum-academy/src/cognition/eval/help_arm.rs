//! The exam's help arm: a worker that hears the one feed for the exam's room, so a peer's words
//! reach the examinee's working memory as a labeled voice (perception through a channel, never a
//! memory write from outside); her own posts are skipped.
//!
//! The exam room is an airc room like every activity, so the arm hears what every resident hears,
//! from the same feed: no second fan-out of room speech. It lives as long as its [`Running`]:
//! dropped with the pass, it can never outlive the exam it serves.

use std::sync::Arc;

use continuum_sage::component::{Component, Ctx, Running, Wiring};
use continuum_sage::dataflow::{Event, OwnerId};
use continuum_sage::Qos;
use uuid::Uuid;

use crate::cognition::room::feed::{self, Arrival, Fed};
use crate::cognition::working_memory::{DispatchStatus, WorkingMemory};

pub(super) struct HelpArm {
    room: Uuid,
    examinee: Uuid,
    memory: Arc<WorkingMemory>,
}

pub(super) struct Heard(Event<Arrival>);

impl From<Event<Arrival>> for Heard {
    fn from(event: Event<Arrival>) -> Self {
        Heard(event)
    }
}

impl Component for HelpArm {
    type Input = Heard;
    const NAME: &'static str = "exam-help-arm";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        // An exam room's chatter while one pass runs: the window the old fan-out held (256).
        const WINDOW: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(256) {
            Some(window) => window,
            None => std::num::NonZeroUsize::MIN,
        };
        wiring.hear_on::<Arrival>(feed::node_bus(), feed::FEED, WINDOW);
    }

    async fn handle(&mut self, Heard(event): Heard, _ctx: &mut Ctx<Self>) {
        if event.value.fed != Fed::Speech {
            return;
        }
        let line = event.value.event.event();
        if line.room_id.as_uuid() != self.room {
            return;
        }
        let Ok(said) = crate::cognition::conversation::room_event::room_content_from_event(line) else { return };
        if said.sender == self.examinee {
            return;
        }
        let who = airc_core::shown_id::shown_form(said.sender);
        self.memory.record_dispatch_event(Uuid::new_v4(), &format!("voice:{who}"), &said.text, DispatchStatus::Done);
        continuum_base::probe!(
            class = "eval.help.voice_folded",
            room = %self.room,
            sender = %who,
            "help arm: a peer's words reached the examinee's perception"
        );
    }
}

/// Start the help arm for `examinee` in `room`, folding into `memory` until the returned
/// [`Running`] is dropped.
pub(super) fn listen(room: Uuid, examinee: Uuid, memory: Arc<WorkingMemory>) -> Running<HelpArm> {
    let epoch = continuum_base::time::EpochMs::now().millis();
    continuum_sage::component::start(HelpArm { room, examinee, memory }, feed::node_bus(), OwnerId(examinee), epoch)
}
