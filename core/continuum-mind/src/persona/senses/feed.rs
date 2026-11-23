//! The one feed: the core's single inbound seam reaches every resident's senses.
//!
//! EVENT-MIND.md §1b and the 2026-10-04 one-feed decision (Cormac, BigMama, Fable): the core holds
//! one attach per room; that feed is the one truth and her perception renders it. Every room's
//! attach task hands its events to [`hear`]; the Feed part classifies each event once and says
//! it on the node bus's feed topic, which every resident's senses hear (object model 5.7: many producers
//! into a worker, fanned out by handle). An emit waits for each subscriber's room, and senses
//! never wait on anything slow, so nothing is dropped and no lock is held per event.

use std::sync::OnceLock;

use airc_core::TranscriptEvent;
use continuum_airc_protocol::ConversationEvent;
use continuum_sage::dataflow::{Emitter, TopicName};
use continuum_sage::component::Bus;

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

/// The one feed's emitter on the node's bus, claimed on first use. No worker: the feed only
/// classifies an event and says it, so the hearing task does that itself. A worker of its own was a
/// task on whichever runtime first heard an event, and when that runtime ended (a test's) every later
/// event went nowhere: CI's core suite hung for an hour on a resident that never woke (2026-11-23).
/// The lock is tokio's, held across the emit, which no runtime owns.
static FEED_EMITTER: OnceLock<Option<tokio::sync::Mutex<Emitter<Arrival>>>> = OnceLock::new();

fn feed() -> &'static Option<tokio::sync::Mutex<Emitter<Arrival>>> {
    FEED_EMITTER.get_or_init(|| continuum_sage::component::node().emitter::<Arrival>(FEED).ok().map(tokio::sync::Mutex::new))
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
    match feed() {
        Some(feed) => {
            let fed = classify(event.event());
            feed.lock().await.emit(Arrival { event, fed }).await;
        }
        None => continuum_base::probe!(class = "mind.feed.stopped", "the one feed's topic is claimed elsewhere; no resident hears this event"),
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
