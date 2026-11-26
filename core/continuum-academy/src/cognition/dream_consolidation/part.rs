//! Her dream as a part of her mind (SAGE.md, "What moves onto it next"; card d56dc734): started on
//! her own bus beside her senses, it hears her experience land, watches whether she is acting, and
//! is asked by her settled cards, her accepted reviews and the dream-now command. Its need comes
//! from what waits for it, never a clock: Urgent while an accepted review waits to be consolidated,
//! Want while she rests with fresh experience or a taught lesson to fold in, Content for the rest
//! (belief review, fresh experience while she acts), and with nothing waiting it rests. Each
//! generation of a pass asks the Core at that need (`inference::need`), so a Content dream steps
//! only in the Core's slack.

use super::*;

use std::num::NonZeroUsize;
use std::time::Duration;

use continuum_sage::component::{Component, Ctx, Key, Urgency, Wiring};
use continuum_sage::dataflow::{Event, Snapshot, TopicName};
use continuum_sage::Qos;

use crate::cognition::admission_state::{Experienced, EXPERIENCE};
use crate::cognition::turn_ledger::presence::{Acting, ACTING};

/// What asks her dream for a pass (an event: none is lost while a pass runs).
pub const DREAM_ASKED: TopicName = TopicName("mind/dream_asked");

/// Who asked her dream, and for what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// `cognition/dream-now`: one pass now.
    Now,
    /// A card of hers settled: one bounded pass, not paged out by her next act.
    CardBoundary,
    /// A review of hers was accepted (the bound submission id): consolidate around it.
    Reviewed { acceptance: Uuid },
}

/// Asks a mind's dream for a pass: her resident dream in production (`senses::ResidentDreams`, on
/// her bus), her dream part itself in a test.
pub trait AskDream: Send + Sync {
    fn ask(&self, persona: Uuid, ask: Ask);
}

/// A test's dream, asked directly: the ask is taken into her state (an acceptance queued) with no
/// pass, as her worker takes it before its pass.
#[cfg(any(test, feature = "test-fixtures"))]
impl AskDream for parking_lot::Mutex<DreamPart> {
    fn ask(&self, persona: Uuid, ask: Ask) {
        let mut part = self.lock();
        if part.persona == persona {
            part.take(DreamInput::Asked(ask));
        }
    }
}

/// How many asks wait while a pass runs: asks are rare (a settled card, an acceptance, an
/// operator), so this never fills; past it the oldest coalesce, and an acceptance stays durable in
/// the credit store, recovered by her next pass.
const ASKS_WAITING: NonZeroUsize = NonZeroUsize::MIN.saturating_add(15);

/// The key of the drain that continues a backlog: one at a time.
const DRAIN: Key = Key::ONE;

/// One input of her dream.
pub enum DreamInput {
    Experienced,
    Acting(bool),
    Asked(Ask),
    /// Continue a backlog the last pass left.
    Drain,
}

impl From<Snapshot<Experienced>> for DreamInput {
    fn from(_: Snapshot<Experienced>) -> Self {
        DreamInput::Experienced
    }
}

impl From<Snapshot<Acting>> for DreamInput {
    fn from(said: Snapshot<Acting>) -> Self {
        DreamInput::Acting(said.value.0)
    }
}

impl From<Event<Ask>> for DreamInput {
    fn from(asked: Event<Ask>) -> Self {
        DreamInput::Asked(*asked.value)
    }
}

/// Whether she is acting a turn now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    Acting,
    Resting,
}

/// Whether what other agents taught her waits to be folded in at her next quiet pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Taught {
    Owed,
    Taken,
}

/// Her dream.
pub struct DreamPart {
    pub(super) persona: Uuid,
    pub(super) source: Arc<dyn PersonaReflectionSource>,
    pub(super) her: Her,
    /// Whether she is acting a turn now (her presence, `mind/acting`).
    turn: Turn,
    /// Whether a taught lesson is to be consolidated at her next quiet pass: owed when she comes to
    /// rest, and at her start.
    pub(super) taught: Taught,
}

impl DreamPart {
    /// Her dream, reading her through `source`.
    pub fn new(persona: Uuid, source: Arc<dyn PersonaReflectionSource>) -> Self {
        Self {
            persona,
            source,
            her: Her::default(),
            turn: Turn::Resting,
            taught: Taught::Owed,
        }
    }

    /// What her dream does with `input`, as its worker does, without a bus: the pass it asks for,
    /// at her need. A test drives her dream through this and reads what she learned.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub async fn step(&mut self, input: DreamInput) {
        let trigger = self.take(input);
        let urgency = self.urgency();
        crate::inference::need::with_urgency(urgency, self.pass(trigger)).await;
    }

    /// Whether the acceptance `review` waits for her next pass.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn review_boundary_pending(&self, review: Uuid) -> bool {
        self.her.boundaries.pending.contains(&review)
    }

    /// Take `input` into her state; the pass it asks for.
    pub(super) fn take(&mut self, input: DreamInput) -> DreamTrigger {
        match input {
            DreamInput::Acting(acting) => {
                let now = if acting { Turn::Acting } else { Turn::Resting };
                if self.turn == Turn::Acting && now == Turn::Resting {
                    self.taught = Taught::Owed; // she came to rest
                }
                self.turn = now;
                DreamTrigger::Idle
            }
            DreamInput::Asked(Ask::Reviewed { acceptance }) => {
                self.her.queue_boundary(self.persona, acceptance);
                DreamTrigger::CardBoundary
            }
            DreamInput::Asked(Ask::CardBoundary) => DreamTrigger::CardBoundary,
            DreamInput::Asked(Ask::Now) | DreamInput::Experienced | DreamInput::Drain => DreamTrigger::Idle,
        }
    }

    /// Her need from what waits for her dream now.
    pub(super) fn urgency(&self) -> Urgency {
        if !self.her.boundaries.pending.is_empty() {
            return Urgency::Urgent; // a reviewed correction waits to become what she knows
        }
        if self.turn == Turn::Acting {
            return Urgency::Content;
        }
        let fresh = self.taught == Taught::Owed || self.source.reflector_for(self.persona).is_some_and(|reflector| self.fresh_clusters(&reflector));
        if fresh { Urgency::Want } else { Urgency::Content }
    }
}

impl Component for DreamPart {
    type Input = DreamInput;
    const NAME: &'static str = "dream";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<Experienced>(EXPERIENCE, Key::ONE);
        wiring.watch::<Acting>(ACTING, Key::ONE);
        wiring.hear::<Ask>(DREAM_ASKED, ASKS_WAITING);
    }

    async fn handle(&mut self, input: DreamInput, ctx: &mut Ctx<Self>) {
        let trigger = self.take(input);
        let urgency = self.urgency();
        ctx.need(urgency);
        // A consolidation has nobody waiting on it (card 7496ed9d); its generations ask the Core at
        // her dream's need.
        let passed = crate::cognition::audience::with(
            crate::inference::prefill_rate::Audience::Unattended,
            crate::inference::need::with_urgency(urgency, self.pass(trigger)),
        )
        .await;
        if passed == (Passed::Ran { more: true }) {
            ctx.after(DRAIN, Duration::ZERO, DreamInput::Drain);
        }
        ctx.need(self.urgency());
    }
}
