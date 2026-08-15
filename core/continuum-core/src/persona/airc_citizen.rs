//! `AircCitizen` — the substrate's universal handle on any actor's
//! airc presence.
//!
//! ## Why this trait exists
//!
//! Pre-slice-13.5 the substrate held the persona's airc handle as
//! `Arc<PersonaAircRuntime>` everywhere. That was honest for production
//! but it forced tests + the supervisor's intermediate seams to carry
//! `Option<Arc<PersonaAircRuntime>>` because constructing a real
//! `PersonaAircRuntime` requires standing up the airc daemon.
//!
//! Per Joel 2026-06-02: "Or base user with airc props… or airc struct
//! even better inside it. As property. Token identity stuff. Maybe dot
//! identity." Per [[personas-are-citizens-airc-is-identity-provider]]:
//! every actor (persona, human, browser) IS an airc citizen. The
//! substrate's calling convention should reflect that with a trait —
//! not a concrete runtime type — because the citizen abstraction
//! transcends "which actor type".
//!
//! `AircCitizen` is that trait. Production implementations
//! ([`PersonaAircRuntime`](super::airc_runtime::PersonaAircRuntime))
//! delegate to the live airc daemon; test fixtures
//! ([`StubAircCitizen`]) hold an in-memory state machine. The
//! [`PersonaContext`](super::supervisor::PersonaContext) field that
//! used to be `Option<Arc<PersonaAircRuntime>>` is now
//! `Arc<dyn AircCitizen>` — no Option, no `.expect("None is test-only")`.
//!
//! This is the first concrete step toward task #142's BaseUser
//! hierarchy. When BaseUser lands, `AircCitizen` is the airc-side
//! interface every BaseUser variant carries; the persona variant adds
//! cognition/genome on top, the human variant adds WebAuthn/session.
//!
//! ## Surface (minimum viable)
//!
//! The trait surfaces only what real consumers call:
//!
//! - `peer_id()` — the airc-side identity (used for self-filtering
//!   in the conversation projection + for tracing spans).
//! - `subscribe()` — open a live event stream on the citizen's room.
//! - `say(text)` — publish a text message under the citizen's
//!   identity in her default room.
//!
//! Plus [`AircTranscriptReader`] as supertrait — every citizen can
//! page recent transcript events, which is what the RAG layer needs.
//! Rust 1.86+ stabilized trait_upcasting so `Arc<dyn AircCitizen>`
//! coerces directly to `Arc<dyn AircTranscriptReader>` at the use
//! site; no helper method, no double indirection.
//!
//! ## What's NOT on the trait
//!
//! `agent_name`, `home`, `default_room`, `persona_id`, `source` —
//! these are persona-substrate metadata, not airc-citizen surface.
//! They live on [`PersonaInstanceInfo`](crate::modules::persona_instance_manager::PersonaInstanceInfo)
//! and on `PersonaContext.identity`. The substrate carries metadata
//! via the identity struct; AircCitizen carries the *live handle*.
//! Two concerns, two types.

use crate::persona::airc_source::AircTranscriptReader;
use airc_lib::{AircError, EventId, FilteredEventStream};
use async_trait::async_trait;
use uuid::Uuid;

/// The substrate's universal airc handle. Implemented by
/// [`PersonaAircRuntime`](super::airc_runtime::PersonaAircRuntime) for
/// production and by [`StubAircCitizen`] for tests; future BaseUser
/// variants (human, browser) impl it via their own airc-lib wrappers.
///
/// `AircCitizen: AircTranscriptReader + AircRosterReader` — every
/// citizen can page her own transcript history AND read who else is
/// present in her room (airc `active_agents`). Rust 1.86+
/// trait_upcasting means `Arc<dyn AircCitizen>` coerces directly to
/// `Arc<dyn AircTranscriptReader>` or `Arc<dyn AircRosterReader>`; no
/// explicit conversion needed. The roster reader is what grounds a
/// persona in who is present (and who is NOT itself) — see
/// docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5.
#[async_trait]
pub trait AircCitizen:
    AircTranscriptReader
    + crate::persona::room_roster_source::AircRosterReader
    + crate::persona::room_doctrine_source::AircDoctrineReader
    + crate::persona::active_work_source::AircWorkReader
    + crate::persona::wall_source::WallReader
    + crate::persona::room_board_source::RoomBoardReader
{
    /// The airc-side peer identity (Ed25519 pubkey, formatted as Uuid).
    /// Cognition uses this for self-loop filtering; the supervisor uses
    /// it as part of the persona's tracing span.
    fn peer_id(&self) -> Uuid;

    /// Open a live event stream over EVERY room this citizen is
    /// subscribed to — deliberately NOT narrowed to her default.
    ///
    /// This is the substrate's ONE perception surface, and it is the
    /// non-narrowing one on purpose. airc-lib ships both shapes:
    /// `Airc::subscribe()` silently narrows to `current_room()` (a
    /// single channel), while `subscribe_subscribed_filtered()` is
    /// documented as *"the monitor/hook surface: no hidden narrowing
    /// to current room"*. Continuum called the narrowing one
    /// everywhere and the widening one nowhere.
    ///
    /// Measured 2026-08-08 on BigMama AND M5, the same shape on both:
    /// every citizen scope was subscribed to exactly ONE room while
    /// the operator scope held several (BigMama: citizens in
    /// `cambriantech`, operator in `cambriantech` + `general`; M5:
    /// citizens in `general`, operator across five). Same socket, one
    /// daemon — M5 falsified the scope/fan-out theories directly. So
    /// every operator message sent to any other room was dropped by
    /// the daemon's own `Filter::channel`, correctly, and a persona
    /// who was addressed all evening never perceived a word of it.
    /// Citizens heard EACH OTHER precisely because they shared one
    /// room, which is what made the failure look like inattention
    /// rather than a wire that structurally could not reach them.
    ///
    /// The stream yields every event the citizen sees — including her
    /// own echoes; consumers self-filter via `peer_id()`. Used by
    /// [`AircPersonaConversation::next_message`](super::airc_persona_conversation::AircPersonaConversation)
    /// to drive the service loop.
    async fn subscribe_all_rooms(&self) -> Result<FilteredEventStream, AircError>;

    /// Publish a text message under the citizen's identity INTO A
    /// SPECIFIC ROOM — normally the room the turn being answered
    /// arrived in ([`IncomingMessage::room_id`](super::service_loop::IncomingMessage),
    /// already carried since A.6).
    ///
    /// Answering is room-targeted for the same reason perception is
    /// non-narrowing, and the two are one fix, not two: widening
    /// perception while replies still went to `current_room()` would
    /// let a persona hear a question in one room and answer it in
    /// another — a strictly worse failure than silence, because it
    /// looks like a non-sequitur rather than a missing wire.
    ///
    /// `room_id` of `Uuid::nil()` means the source predates room
    /// stamping (scripted / test conversations, per the documented
    /// `IncomingMessage::room_id` contract) and routes to the
    /// citizen's default room. That is the existing typed contract
    /// being honoured, not a fallback for an unroutable id: a real
    /// wire event always carries its room, and a NAMED room that
    /// cannot be resolved fails loud out of airc-lib rather than
    /// quietly landing somewhere else.
    ///
    /// Returns the daemon-assigned event id so callers can correlate
    /// with the subscribe stream's echo (not required today, but the
    /// wire shape preserves it).
    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<EventId, AircError>;

    /// #170: publish ONE ephemeral streaming token chunk (typing-indicator
    /// class, NOT durable transcript) via airc-lib's `publish_stream_chunk`.
    /// The settled utterance is still published once via [`say`](Self::say);
    /// these chunks let subscribers (positron / TTS / avatar) render the answer
    /// progressively as it decodes (#169 proved the token rail; this makes it
    /// VISIBLE). Default no-op — only the production runtime streams; scripted /
    /// stub citizens don't. Returns `Ok(())` (the event id isn't needed by the
    /// forwarder).
    async fn publish_stream_chunk(
        &self,
        _chunk: &airc_lib::StreamChunk,
    ) -> Result<(), AircError> {
        Ok(())
    }
}

/// THE implementation of [`AircCitizen::subscribe_all_rooms`] over a
/// raw airc handle — every impl that wraps an `Airc` calls this one
/// rather than re-deciding which subscribe shape to use.
///
/// The whole defect this fixes was two shapes existing and continuum
/// picking the narrowing one at every call site independently. Making
/// the choice ONCE is what stops it drifting back, so a future citizen
/// impl inherits the right answer instead of re-choosing.
pub(crate) async fn subscribe_every_room(
    airc: &airc_lib::Airc,
) -> Result<FilteredEventStream, AircError> {
    airc.subscribe_subscribed_filtered(airc_lib::EventFilter::default())
        .await
}

/// Where a reply for `room_id` should be published — the ONE place
/// that decision is made, extracted so it can be asserted without an
/// airc daemon.
///
/// Nil = the documented "source predates room stamping" contract
/// (scripted / test conversations) → her default room. A real wire
/// turn always carries its room.
///
/// Everything else routes by the channel id itself: `RoomByName`
/// resolves a channel NAME *or* the channel id a caller already holds,
/// and refuses to auto-join. That refusal is the safety property —
/// answering can never silently change which rooms she belongs to, and
/// a room she cannot address fails loud instead of the reply landing
/// somewhere she was never spoken to.
pub(crate) fn publish_target_for(room_id: Uuid) -> airc_lib::PublishTarget {
    if room_id.is_nil() {
        airc_lib::PublishTarget::CurrentRoom
    } else {
        airc_lib::PublishTarget::RoomByName(room_id.to_string())
    }
}

/// THE implementation of [`AircCitizen::say_in`] over a raw airc
/// handle. Same reason as [`subscribe_every_room`]: the nil-room
/// contract and the no-auto-join publish target are decided once.
pub(crate) async fn publish_text_in_room(
    airc: &airc_lib::Airc,
    room_id: Uuid,
    text: &str,
) -> Result<EventId, AircError> {
    airc.publish(
        publish_target_for(room_id),
        airc_protocol::FrameKind::Message,
        airc_core::Body::text(text),
        airc_core::Headers::default(),
    )
    .await
    .map(|receipt| receipt.event_id)
}

/// Test fixture implementing [`AircCitizen`] without standing up the
/// airc daemon. Holds the peer_id the test wants to project; subscribe
/// and say resolve to errors (the service-loop tests don't drive
/// either path — they use [`StubConversation`](super::service_loop)
/// instead). `page_recent` returns empty so RAG runs through cleanly.
///
/// This is the substrate's answer to "why was runtime an Option" —
/// instead of leaking the Option into production, tests get a typed
/// stub that satisfies the same interface. Per [[no-fallbacks-ever]]
/// — no Option, no expect, no silent substitution.
pub struct StubAircCitizen {
    peer_id: Uuid,
}

impl StubAircCitizen {
    /// Build a stub with the given peer_id. Tests usually want this to
    /// match the `PersonaInstanceInfo::peer_id` on the same hosted
    /// persona so cognition's self-filter behaves consistently.
    pub fn new(peer_id: Uuid) -> Self {
        Self { peer_id }
    }

    /// Convenience: a `runtime_lookup` closure for
    /// `materialize_adapters` that returns a fresh stub for every
    /// persona_id queried. Substrate-level helper per
    /// [[test-fixtures-are-system-primitives]] — every supervisor
    /// test that exercises materialize_adapters without a real airc
    /// daemon leases this closure shape.
    pub fn fresh_lookup(
    ) -> impl Fn(Uuid) -> Option<std::sync::Arc<dyn AircCitizen>> + Clone {
        |_pid| {
            Some(std::sync::Arc::new(Self::new(Uuid::new_v4()))
                as std::sync::Arc<dyn AircCitizen>)
        }
    }
}

#[async_trait]
impl AircTranscriptReader for StubAircCitizen {
    async fn page_recent(
        &self,
        _limit: usize,
    ) -> Result<Vec<airc_lib::TranscriptEvent>, AircError> {
        Ok(vec![])
    }
}

#[async_trait]
impl crate::persona::room_roster_source::AircRosterReader for StubAircCitizen {
    fn self_peer_id(&self) -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(self.peer_id)
    }

    async fn room_roster(
        &self,
        _within: std::time::Duration,
        _window: usize,
    ) -> Result<Vec<airc_lib::RoomMember>, AircError> {
        // No daemon in tests → no presence. RAG runs through cleanly
        // with an empty roster (no [Present in this room] block).
        Ok(vec![])
    }
}

#[async_trait]
impl crate::persona::room_doctrine_source::AircDoctrineReader for StubAircCitizen {
    async fn room_doctrine(
        &self,
    ) -> Result<Option<airc_core::doctrine::RoomDoctrinePublished>, AircError> {
        // No daemon in tests → no published doctrine. Cognition runs
        // through cleanly with no [Room operating doctrine] block.
        Ok(None)
    }
}

#[async_trait]
impl crate::persona::active_work_source::AircWorkReader for StubAircCitizen {
    async fn active_claims(&self) -> Result<Vec<airc_lib::WorkCard>, AircError> {
        // No daemon in tests → no claimed work. Cognition runs through cleanly
        // with no [active-work] grounding block.
        Ok(vec![])
    }
}

#[async_trait]
impl crate::persona::wall_source::WallReader for StubAircCitizen {
    async fn wall_posts(
        &self,
    ) -> Result<Vec<airc_core::doctrine::WallPostPublished>, AircError> {
        // No daemon in tests → no pinned wall posts. Cognition runs through
        // cleanly with no [room-board] grounding block.
        Ok(vec![])
    }
}

#[async_trait]
impl crate::persona::room_board_source::RoomBoardReader for StubAircCitizen {
    async fn work_board(
        &self,
        _room: Option<uuid::Uuid>,
    ) -> Result<airc_work::BoardSnapshot, AircError> {
        // No daemon in tests → an empty board. Cognition runs through cleanly
        // with no [room-kanban] grounding block.
        Ok(airc_work::BoardSnapshot {
            cards: Vec::new(),
            lanes: Vec::new(),
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        })
    }

    /// No daemon in tests → no alias store. The board is empty here anyway, so
    /// there is nothing to name; an owner that did appear would render as its
    /// short id, which is honest and still addressable.
    async fn peer_names(
        &self,
        _peers: &[airc_core::PeerId],
    ) -> std::collections::HashMap<airc_core::PeerId, String> {
        std::collections::HashMap::new()
    }
}

#[async_trait]
impl AircCitizen for StubAircCitizen {
    fn peer_id(&self) -> Uuid {
        self.peer_id
    }

    async fn subscribe_all_rooms(&self) -> Result<FilteredEventStream, AircError> {
        // A stub citizen HAS no transport, and says so.
        //
        // This was `unreachable!()`, on the stated premise that "no
        // service-loop test drives the stub's subscribe". That premise was
        // true when written and stopped being true when the supervisor grew
        // its doctrine/wall cache: `supervisor.rs` now subscribes to wire a
        // publish-invalidator, so five supervisor tests — which are about
        // adapter materialization and warmup, and care nothing about event
        // streams — reached this line and aborted. `cargo test -p
        // continuum-core --lib` has been red on every canary push since,
        // which is what makes "canary is green" mean nothing to everyone
        // else in the repo.
        //
        // The guard was right to refuse an EMPTY STREAM: that would hand the
        // supervisor a wire that never fires, so the cache would go stale
        // silently and the tests would pass while proving nothing. That is
        // the masking the original comment correctly rejected, and this does
        // not do it.
        //
        // Returning `Transport` is not the "variant that doesn't fit" the
        // old comment feared. It is a free-form transport-side variant and
        // "this citizen has no transport" is a transport-side fact. The call
        // site already has the matching branch: on `Err` the supervisor
        // serves raw doctrine/wall sources and logs `cache UNWIRED … slow but
        // never stale`. So the absence stays LOUD and correct — it travels
        // the path designed for it instead of killing the process.
        Err(AircError::Transport(
            "StubAircCitizen has no transport — nothing subscribes, nothing publishes. \
             Drive service-loop tests through StubConversation; callers that need a live \
             stream must use a real citizen."
                .to_string(),
        ))
    }

    async fn say_in(&self, _room_id: Uuid, _text: &str) -> Result<EventId, AircError> {
        unreachable!(
            "StubAircCitizen::say_in must not be called — \
             service-loop tests should reply through StubConversation, \
             not through the citizen handle"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn stub_returns_peer_id_and_empty_transcript() {
        let peer = Uuid::new_v4();
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(peer));
        assert_eq!(stub.peer_id(), peer);
        let events = stub.page_recent(10).await.expect("empty page_recent ok");
        assert!(events.is_empty());
    }

    /// what this catches: the stub handing back a wire it does not have.
    ///
    /// This asserted a PANIC until 2026-08-13, on the premise that nothing
    /// would ever call subscribe on a stub. The supervisor's doctrine/wall
    /// cache then did exactly that, and five supervisor tests — about adapter
    /// materialization, not events — died on it, keeping `cargo test -p
    /// continuum-core --lib` red on every canary push.
    ///
    /// The invariant that actually matters survives, and is what this now
    /// pins: the stub must report an ERROR, never `Ok` with an empty stream.
    /// An empty stream is a wire that never fires, so the supervisor's cache
    /// would go stale in silence and this test would pass while proving
    /// nothing. Err travels the branch built for it — raw sources, loud warn.
    #[tokio::test]
    async fn stub_subscribe_reports_no_transport_rather_than_faking_a_wire() {
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let error = stub
            .subscribe_all_rooms()
            .await
            .err()
            .expect("stub must REFUSE to subscribe — an Ok here is an empty stream nobody fires");
        assert!(
            matches!(error, AircError::Transport(_)),
            "the refusal must name the missing transport, got: {error}"
        );
    }

    // what this catches: a reply addressed to the room that asked, rather than
    // to the citizen's ambient default. Measured 2026-08-08 on BigMama AND M5:
    // citizens were subscribed to exactly ONE room while the operator held
    // several, so operator messages were dropped by the daemon's own channel
    // filter and personas looked inattentive when they were structurally deaf.
    // Widening perception without this half is WORSE than the original bug —
    // she would hear a question in one room and answer it to a different
    // audience, which reads as a non-sequitur rather than a missing wire.
    // A regression that collapsed this back to `CurrentRoom` would be invisible
    // on a single-room box and only surface once citizens are convened (#54).
    #[test]
    fn a_reply_targets_the_room_that_asked_not_the_default() {
        let asked_in = Uuid::new_v4();
        assert_eq!(
            publish_target_for(asked_in),
            airc_lib::PublishTarget::RoomByName(asked_in.to_string()),
            "a stamped arrival room must route the reply BY THAT ROOM"
        );
    }

    // what this catches: the nil-room contract. `IncomingMessage::room_id` is
    // nil only for sources that predate room stamping (scripted / test
    // conversations); that means "no room was stated", which resolves to her
    // default. It must NOT become `RoomByName("00000000-0000-…")`, a room that
    // exists nowhere — that would turn every scripted turn into a publish error
    // instead of an ordinary reply, and it is the exact shape a "simplify the
    // branch away" refactor produces.
    #[test]
    fn a_nil_room_means_unstated_not_a_room_named_nil() {
        assert_eq!(
            publish_target_for(Uuid::nil()),
            airc_lib::PublishTarget::CurrentRoom,
            "nil is the absence of a stated room, never a room whose name is nil"
        );
    }

    /// The whole point of this refactor: `Arc<dyn AircCitizen>` should
    /// coerce to `Arc<dyn AircTranscriptReader>` via trait_upcasting.
    /// If this stops compiling, the substrate has regressed to the
    /// pre-1.86 Rust pattern (manual conversion methods).
    #[tokio::test]
    async fn citizen_arc_upcoerces_to_transcript_reader() {
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let reader: Arc<dyn AircTranscriptReader> = stub.clone();
        let events = reader.page_recent(0).await.expect("page_recent");
        assert!(events.is_empty());
    }
}
