//! The one feed: the core's single inbound seam reaches every resident's senses.
//!
//! EVENT-MIND.md §1b and the 2026-10-04 one-feed decision (Cormac, BigMama, Fable): the core holds
//! one attach per room; that feed is the one truth and her perception renders it. Every room's
//! attach task hands its events to [`hear`]; the Feed part classifies each event once and says
//! it on the node bus's feed topic, which every resident's senses hear (object model 5.7: many producers
//! into a worker, fanned out by handle). An emit never waits: each resident's senses has its own
//! window (as much as her room view keeps, `UNPERCEIVED_CAP`), so one resident's slow senses never
//! holds the feed or anyone else's perception; one that falls a window behind sees the gap in the
//! revisions, and the transcript stays the one truth to page from.

use std::sync::OnceLock;

use airc_core::TranscriptEvent;
use continuum_airc_protocol::ConversationEvent;
use continuum_sage::dataflow::{Emitter, OwnerId, TopicName};
use continuum_sage::component::{start, Bus, Ctx, Component, Running, Wiring};
use continuum_sage::Qos;

use crate::cognition::conversation::room_event::{room_work_from_event, RoomWork};
use crate::cognition::conversation::salience::BoardChange;

/// What one event is, classified once for every resident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fed {
    Speech,
    Board(BoardChange),
    /// A work fact the mind does not yet render as a board change.
    OtherWork(String),
    NotPerception(&'static str),
}

/// A room event as every resident's senses receive it: the event and what it is.
#[derive(Debug)]
pub struct Arrival {
    pub event: ConversationEvent,
    pub fed: Fed,
}

/// Classify once: a typed work fact first, else speech if the event is a room line, else not
/// perception. Pure.
pub fn classify(event: &TranscriptEvent) -> Fed {
    match room_work_from_event(event) {
        Ok(Some(RoomWork::Reviewed { card_id, outcome, reviewer, review })) => {
            Fed::Board(BoardChange::Reviewed { card_id, outcome, reviewer, review })
        }
        Ok(Some(RoomWork::StateChanged { card_id, by, .. })) => Fed::Board(BoardChange::Moved { card_id, by }),
        Ok(Some(RoomWork::Claimed { card_id, owner, .. })) => Fed::Board(BoardChange::Moved { card_id, by: owner }),
        Ok(Some(RoomWork::Submitted { card_id, publisher, .. })) => Fed::Board(BoardChange::Moved { card_id, by: publisher }),
        Ok(Some(RoomWork::Other { kind })) => Fed::OtherWork(kind),
        Err(reason) => Fed::NotPerception(reason),
        Ok(None) => match crate::cognition::conversation::room_event::room_content_from_event(event) {
            Ok(_) => Fed::Speech,
            Err(reason) => Fed::NotPerception(reason),
        },
    }
}

/// Where every resident's senses hear room events: a topic on the node's bus.
pub(super) const FEED: TopicName = TopicName("node/feed");

/// The one feed, a component of the node: it classifies each event once and says it on [`FEED`].
struct Feed {
    says: Emitter<Arrival>,
}

impl Component for Feed {
    type Input = ConversationEvent;
    const NAME: &'static str = "feed";
    const QOS: Qos = Qos::Interactive;

    fn wire(&self, _: &mut Wiring<'_, Self>) {}

    async fn handle(&mut self, event: ConversationEvent, ctx: &mut Ctx<Self>) {
        let fed = classify(event.event());
        ctx.emit(&mut self.says, Arrival { event, fed });
    }
}

static FEED_WORKER: OnceLock<Option<Running<Feed>>> = OnceLock::new();

/// The one feed's worker on the node's bus, started on first use (its topic is unclaimed until then).
fn feed() -> &'static Option<Running<Feed>> {
    FEED_WORKER.get_or_init(|| {
        let bus = continuum_sage::component::node();
        bus.emitter::<Arrival>(FEED).ok().map(|says| start(Feed { says }, bus, OwnerId(uuid::Uuid::nil()), 0))
    })
}

/// The node's bus: topics every resident shares (the one feed).
pub(super) fn node_bus() -> &'static Bus {
    feed();
    continuum_sage::component::node()
}

/// One room event, from the core's one inbound seam. Only the header classifier
/// (`continuum_airc_protocol::classify_inbound`) constructs a [`ConversationEvent`], so a peer
/// command or protocol frame cannot reach a mind (2026-10-30). Waits for the feed's room.
pub async fn hear(event: ConversationEvent) {
    let heard = match feed() {
        Some(feed) => feed.tell(event).await,
        None => false,
    };
    if !heard {
        continuum_base::probe!(class = "mind.feed.stopped", "the one feed is not running; no resident hears this event");
    }
}

/// Bridges one room's transcript into this core (at most once per room). The airc layer owns
/// the attach streams and installs this when it stands them up.
pub type RoomAttacher = fn(airc_core::RoomId);

static ROOM_ATTACHER: parking_lot::RwLock<Option<RoomAttacher>> = parking_lot::RwLock::new(None);

/// Install the one room attacher. A second install replaces the first.
pub fn install_room_attacher(attacher: RoomAttacher) {
    *ROOM_ATTACHER.write() = Some(attacher);
}

/// Under one feed she perceives only rooms the core has attached, so every room she is in is
/// attached (Cormac, #4731: a room she joined herself never goes silent for her).
pub(super) fn attach_rooms(rooms: &[uuid::Uuid]) {
    let Some(attach) = *ROOM_ATTACHER.read() else { return }; // no transport attached in this process
    for room in rooms {
        attach(airc_core::RoomId::from_uuid(*room));
    }
}
