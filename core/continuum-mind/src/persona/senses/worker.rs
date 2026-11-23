//! The Senses worker: her perception region, owned, woken only by an input.

use std::sync::Arc;

use airc_core::{PeerId, TranscriptEvent};
use continuum_base::time::EpochMs;
use continuum_sage::dataflow::{Event, Publisher};
use continuum_sage::component::{Ctx, Key, Component, Wiring};
use continuum_sage::Qos;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::cognition::room::feed::{Arrival, Fed};
use crate::cognition::attention::{AttentionDial, Continuation};
use crate::cognition::conversation::persona_identity::PersonaIdentity;
use crate::cognition::conversation::salience::{BoardChange, SpeechLine};
use super::Calling;
use crate::cognition::conversation::Wake;
use crate::persona::mind_state::{ActivityCursor, MindStateError};
use crate::persona::perception_region::PerceptionRegion;

/// What wakes her senses: a room event, or one of her own acts.
pub enum Sensed {
    /// A room event from the one feed, classified once there.
    Heard(Event<Arrival>),
    /// Her memberships: the activities she is in.
    Joined(Vec<(Uuid, String)>),
    /// Where room events go on to her conversation's admission (replaced on every membership
    /// change, with the pump that reads it).
    Admit(mpsc::Sender<Arc<TranscriptEvent>>),
    /// Her turn took in `room` through `cursor`.
    Perceived { room: Uuid, cursor: ActivityCursor },
    /// Her act: what she works on next. Saved to her mind state in `dir`; the outcome is answered.
    Continuation { continuation: Option<Continuation>, dir: std::path::PathBuf, saved: oneshot::Sender<Result<(), MindStateError>> },
    /// Her act: how wide her door is.
    Dial(AttentionDial),
    /// She started a turn for this wake: it stops calling.
    Took(Wake),
    /// Her continuation's time came.
    Due,
}

impl From<Event<Arrival>> for Sensed {
    fn from(event: Event<Arrival>) -> Self {
        Sensed::Heard(event)
    }
}

/// Her senses: a part of her, a SAGE `Component`, so its worker, its subscription to the one feed,
/// its timer and the provenance of what it publishes are the engine's, not written here.
pub(super) struct Senses {
    pub(super) persona: Uuid,
    pub(super) peer: PeerId,
    pub(super) identity: PersonaIdentity,
    pub(super) region: PerceptionRegion,
    pub(super) admission: Option<mpsc::Sender<Arc<TranscriptEvent>>>,
    /// What calls her, on her mind's bus.
    pub(super) calling: Publisher<Calling>,
    pub(super) published: Calling,
    /// Her awareness strip, on her mind's bus.
    pub(super) strip: Publisher<crate::persona::awareness::AwarenessSnapshot>,
    /// The strip last published, without its clock: republished only when what she perceives moved.
    pub(super) strip_published: Option<crate::persona::awareness::AwarenessSnapshot>,
    pub(super) due_at: Option<u64>,
    /// A saved continuation calls her with `Resume` until she takes it.
    pub(super) resume_pending: bool,
    /// The due time of the continuation she already took a wake for: it stops calling until
    /// she sets a new one.
    pub(super) served_due: Option<u64>,
}

impl Component for Senses {
    type Input = Sensed;
    const NAME: &'static str = "senses";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        // Her window on the feed: as much as her room view keeps unperceived.
        const WINDOW: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(crate::persona::perception_region::UNPERCEIVED_CAP) {
            Some(window) => window,
            None => std::num::NonZeroUsize::MIN,
        };
        wiring.hear_on::<Arrival>(crate::cognition::room::feed::node_bus(), crate::cognition::room::feed::FEED, WINDOW);
    }

    async fn handle(&mut self, input: Sensed, ctx: &mut Ctx<Self>) {
        let now = EpochMs::now().millis();
        match input {
            Sensed::Heard(event) => self.hear(&event.value, now),
            Sensed::Joined(rooms) => {
                for (room, name) in rooms {
                    self.region.join(room, name);
                }
                crate::cognition::room::feed::attach_rooms(&self.region.activities());
            }
            Sensed::Admit(admission) => self.admission = Some(admission),
            Sensed::Perceived { room, cursor } => self.region.perceived(room, &cursor, now),
            Sensed::Continuation { continuation, dir, saved } => {
                self.region.set_continuation(continuation, now);
                self.served_due = None;
                let outcome = self.region.save(&dir, now);
                if saved.send(outcome).is_err() {
                    continuum_base::probe!(class = "mind.senses.unanswered", persona = %self.persona, "her continuation was set; the act that asked is gone");
                }
            }
            Sensed::Dial(dial) => self.region.set_dial(dial, now),
            Sensed::Took(wake) => match wake {
                Wake::Resume => self.resume_pending = false,
                Wake::Continuation => self.served_due = self.region.continuation_due_ms(),
                // A Perceive stops calling when her turn perceives the activity.
                Wake::Perceive { .. } => {}
            },
            Sensed::Due => {}
        }
        self.schedule_due(now, ctx);
        // Her strip first: a part woken by what calls her reads a strip at least as new.
        self.publish_strip(now, ctx);
        self.publish(now, ctx);
    }
}

impl Senses {
    /// One room event: her region learns it if she is in that room, and her conversation's
    /// admission gets it.
    fn hear(&mut self, arrival: &Arrival, now: u64) {
        let event = arrival.event.event();
        let room = event.room_id.as_uuid();
        if !self.region.is_in(room) {
            return;
        }
        match &arrival.fed {
            Fed::Speech => {
                self.region.observe_speech(room, SpeechLine::from_event_for(event, &self.identity), now);
            }
            Fed::Board(change) => {
                // Her own act's echo is hers, not news (the same rule as her words).
                let actor = match change {
                    BoardChange::Reviewed { reviewer, .. } => *reviewer,
                    BoardChange::Moved { by, .. } => *by,
                    BoardChange::Blocked { .. } | BoardChange::WaitingOnMe { .. } => Uuid::nil(),
                };
                if actor != self.peer.as_uuid() {
                    self.region.observe_board(room, vec![change.clone()], now);
                }
            }
            Fed::OtherWork(_) | Fed::NotPerception(_) => {}
        }
        if matches!(arrival.fed, Fed::NotPerception(_)) {
            return;
        }
        if let Some(admission) = &self.admission {
            if let Err(mpsc::error::TrySendError::Full(_)) = admission.try_send(Arc::clone(event)) {
                continuum_base::probe!(
                    class = "mind.feed.event_dropped",
                    persona = %self.persona,
                    "her admission channel is full; the one truth still holds the event for her next page"
                );
            }
        }
    }

    /// Her one timer, her continuation's deadline: armed through the engine, replaced when she
    /// sets a new one, dropped when she clears it.
    fn schedule_due(&mut self, now: u64, ctx: &mut Ctx<Self>) {
        let due = self.region.continuation_due_ms().filter(|due| *due > now);
        if due == self.due_at {
            return;
        }
        self.due_at = due;
        match due {
            Some(due) => ctx.after(Key::ONE, std::time::Duration::from_millis(due - now), Sensed::Due),
            None => ctx.cancel(Key::ONE),
        }
    }

    /// What calls her now, loudest first, from her region's own answer: `Resume` until she
    /// takes it, then an activity loud enough for her dial, then her continuation once due and
    /// until she takes it.
    fn calling(&mut self, now: u64) -> Option<Wake> {
        if self.resume_pending {
            return Some(Wake::Resume);
        }
        match self.region.wake_for(now) {
            Some(Wake::Continuation) if self.served_due.is_some() && self.served_due == self.region.continuation_due_ms() => None,
            other => other,
        }
    }

    /// Every activity loud enough to have woken her that she has not perceived yet.
    fn pending(&self) -> std::collections::BTreeSet<Uuid> {
        self.region.activities().into_iter().filter(|activity| self.region.is_pending(*activity)).collect()
    }

    /// Publish her awareness strip, when what she perceives moved (its clock alone is not a move).
    fn publish_strip(&mut self, now: u64, ctx: &Ctx<Self>) {
        let mut strip = self.region.snapshot(now);
        let at_ms = strip.at_ms;
        strip.at_ms = 0;
        if self.strip_published.as_ref() == Some(&strip) {
            return;
        }
        self.strip_published = Some(strip.clone());
        strip.at_ms = at_ms;
        ctx.publish(&mut self.strip, strip);
    }

    /// Publish what calls her, when it changed.
    fn publish(&mut self, now: u64, ctx: &Ctx<Self>) {
        let calling = Calling { wake: self.calling(now), pending: self.pending() };
        if calling == self.published {
            return;
        }
        self.published = calling.clone();
        ctx.publish(&mut self.calling, calling);
    }
}
