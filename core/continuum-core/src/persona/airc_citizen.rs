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

    /// Watch receiver whose value increments whenever this citizen's room
    /// membership GROWS at runtime (a join after spawn) — the perception
    /// stream's rebuild cue (P0 20b44763). airc-lib's
    /// `subscribe_subscribed_filtered` snapshots the subscribed-channel
    /// list ONCE at subscribe time, so a room joined later (benchmark
    /// dispatch moving assignees into a fresh run room) never enters an
    /// existing stream — the run room is born deaf. Consumers select on
    /// this beside the stream and re-open it (via
    /// [`subscribe_all_rooms`](Self::subscribe_all_rooms)) when the epoch
    /// moves. The sibling of the 2026-08-08 narrowing bug documented on
    /// `subscribe_all_rooms` above: that one snapshotted the WRONG SET,
    /// this one snapshots the right set at the WRONG TIME.
    ///
    /// Default: a receiver whose sender is already dropped — membership
    /// never changes for stubs/fixtures. Consumers MUST treat a closed
    /// channel as "never fires" (park on `pending()`), never as an event,
    /// or a stub conversation would busy-loop on `changed() == Err`.
    fn membership_epoch(&self) -> tokio::sync::watch::Receiver<u64> {
        tokio::sync::watch::channel(0u64).1
    }

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
    async fn publish_stream_chunk(&self, _chunk: &airc_lib::StreamChunk) -> Result<(), AircError> {
        Ok(())
    }

    /// Write a held card's lifecycle state — the WRITE half of the work board
    /// (the read supertraits only observe). This is what lets the deterministic
    /// held-work settle edge conclude a card the moment she passes it "done",
    /// through the SAME change+emit path the `work/state` verb uses, so the
    /// owning recipe's outcome reactor fires exactly as it would for the verb.
    /// Default is a hard error, not a silent no-op: a citizen with no
    /// board-write capability advancing a card would be an invisible loss
    /// ([[fallbacks-are-illegal-fail-loud]]); only the production runtime
    /// overrides this. Recipe-general — the card may be a benchmark, a code
    /// task, or any other activity's work item.
    async fn advance_card_to(
        &self,
        _card_id: airc_lib::WorkCardId,
        _state: airc_lib::CardState,
    ) -> Result<(), String> {
        Err("this citizen has no work-board write capability".to_string())
    }

    /// PULL a card off the shared team deck — claim it as this citizen so her
    /// held-work loop then works it. Kanban pull (an idle member grabs the next
    /// Open card) rather than push (a fixed pile pre-assigned at dispatch). The
    /// claim is deterministic (the substrate pulls when she is free), NOT an LLM
    /// tool call, so it can't be skipped. Default errors (no board-write
    /// capability); the real runtime routes through airc's claim path. Returns
    /// `Ok(false)` when the card was already taken by a teammate (a lost race is
    /// normal on a shared deck — try the next one), `Ok(true)` when SHE now holds
    /// it.
    async fn claim_card(&self, _card_id: airc_lib::WorkCardId) -> Result<bool, String> {
        Err("this citizen has no work-board write capability".to_string())
    }

    /// The rooms this citizen is RESIDENT in (her durable subscription set). This is
    /// the pull-eligibility source: a card is content of the room it was posted in,
    /// and a resident may pull it — never a dispatch-time team or assignee list.
    /// Default is an empty set (a citizen standing nowhere pulls nothing), which is
    /// honest for read-only stand-ins; the production runtime reads airc.
    async fn subscribed_rooms(&self) -> Result<Vec<Uuid>, AircError> {
        Ok(Vec::new())
    }

    /// The cards on `room`'s board that are takeable RIGHT NOW (Open, or held on a
    /// lapsed lease) per the ONE claimability decision
    /// ([`crate::persona::card_holder::claimable_now`]). The pull reads THIS, not the
    /// round tracker: the tracker never records a claim, so without board truth every
    /// idle resident would retry the first already-claimed card forever. Default is
    /// empty (nothing offered); the production runtime folds the room's board.
    async fn claimable_cards_in(&self, _room: Uuid, _now_ms: u64) -> Result<Vec<Uuid>, AircError> {
        Ok(Vec::new())
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
    // NO router-side header filter (2026-09-04). The `Not(Has(heartbeat))`
    // filter this used to send (#445) arrived at the daemon INVERTED: every
    // citizen received ONLY the presence heartbeats and never a message —
    // measured in the daemon store (message rows lack the header, heartbeat
    // rows carry it) and in the inbound probes (100% of live inbound =
    // heartbeats, 0 messages) while an unfiltered `airc join` client on the
    // same daemon got every message. Citizens had heard rooms only through
    // the store page at turn time since the filter landed. Heartbeats are
    // dropped on RECEIVE instead ([`is_heartbeat`]) — a header check per
    // event, the same receive-side shape as the stream-chunk guard, correct
    // regardless of how the daemon treats a negated filter.
    // NO delivery filter either (2026-09-04, second cut): with `Some([Durable])`
    // citizens received `event`-kind frames live but never a `message` — while an
    // `airc join` client on the same daemon (no delivery filter) received every
    // message. Subscribe exactly as the CLI does; stream chunks and heartbeats
    // are dropped on receive (`is_stream_chunk`, `is_heartbeat`), where the
    // guard is correct regardless of what the daemon does with a filter.
    let filter = airc_lib::EventFilter::default();
    airc.subscribe_subscribed_delivery(filter, None).await
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
/// A presence heartbeat (`airc.heartbeat.kind` header) — never a room turn.
/// The receive-side twin of the subscribe-time exclusion the daemon inverted;
/// consumers skip these BEFORE paying for a decode.
pub(crate) fn is_heartbeat(event: &airc_core::TranscriptEvent) -> bool {
    event.headers.get(airc_lib::HEADER_HEARTBEAT_KIND).is_some()
}

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

/// Resolve the channel NAME for `room_id` from ANY runtime in this process
/// that already subscribes to it — the core hosts every citizen runtime, so a
/// room at least one citizen belongs to is always nameable, even though the
/// channel-id → name mapping is one-way (ids are hashes of names).
///
/// Exists for the operator/agent JOIN-ON-SEND heal (chat/send): after the
/// 2026-09-01 reboot marathon, the self-peers' scopes had no subscription to
/// rooms the citizens lived in, so every CLI send stored + live-fed but the
/// daemon say was refused — the operator asked questions into a void, and
/// join-by-uuid is structurally impossible from outside. Failure-path only
/// (one subscription-set read per runtime, and only after a refused publish),
/// so it costs the hot path nothing.
pub(crate) async fn room_name_by_id(room_id: Uuid) -> Option<String> {
    let reg = crate::persona::PersonaAircRuntimeRegistry::try_global()?;
    let runtimes: Vec<_> = reg.iter().collect();
    for rt in runtimes {
        if let Ok(set) = rt.airc().subscription_set().await {
            if let Some(sub) = set
                .all()
                .find(|s| s.as_room().channel.as_uuid() == room_id)
            {
                return Some(sub.name.to_string());
            }
        }
    }
    None
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
    /// Rooms the stub reports as resident in (see [`AircCitizen::subscribed_rooms`]).
    rooms: Vec<Uuid>,
    /// Cards the stub reports as claimable on ANY room's board
    /// (see [`AircCitizen::claimable_cards_in`]).
    claimable: Vec<Uuid>,
    /// Cards this stub reports as its active claims — empty by default. A test
    /// that exercises the held-work path seeds one so `active_claims()` returns
    /// held work (the held-work gate fires only on a Claimed/InProgress card).
    held: Vec<airc_lib::WorkCard>,
    /// Records every `advance_card_to` call so a test can assert the held-work
    /// completion edge concluded the right card with the right state — the
    /// WRITE half the read supertraits can't observe. Shared `Arc` so the test
    /// holds a clone and reads it after driving the turn.
    advanced: std::sync::Arc<std::sync::Mutex<Vec<(airc_lib::WorkCardId, airc_lib::CardState)>>>,
    /// Records every `claim_card` (pull) call — the kanban-pull half, so a test
    /// can assert an idle citizen pulled the right Open card off the deck.
    claimed: std::sync::Arc<std::sync::Mutex<Vec<airc_lib::WorkCardId>>>,
}

impl StubAircCitizen {
    /// Build a stub with the given peer_id. Tests usually want this to
    /// match the `PersonaInstanceInfo::peer_id` on the same hosted
    /// persona so cognition's self-filter behaves consistently.
    pub fn new(peer_id: Uuid) -> Self {
        Self {
            peer_id,
            rooms: Vec::new(),
            claimable: Vec::new(),
            held: Vec::new(),
            advanced: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            claimed: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// A handle to the `claim_card` (pull) recording — clone before moving the
    /// stub behind an `Arc<dyn AircCitizen>`, then read which cards she pulled.
    pub fn claim_recorder(&self) -> std::sync::Arc<std::sync::Mutex<Vec<airc_lib::WorkCardId>>> {
        self.claimed.clone()
    }

    /// Seed the cards this stub reports as active claims — so the held-work
    /// gate sees held work.
    pub fn with_claims(mut self, cards: Vec<airc_lib::WorkCard>) -> Self {
        self.held = cards;
        self
    }

    /// Stand this stub in `rooms` — what [`AircCitizen::subscribed_rooms`] reports.
    pub fn with_rooms(mut self, rooms: Vec<Uuid>) -> Self {
        self.rooms = rooms;
        self
    }

    /// Offer `cards` as claimable on every room's board
    /// (what [`AircCitizen::claimable_cards_in`] reports).
    pub fn with_claimable(mut self, cards: Vec<Uuid>) -> Self {
        self.claimable = cards;
        self
    }

    /// A handle to the `advance_card_to` recording — clone before moving the
    /// stub behind an `Arc<dyn AircCitizen>`, then read after driving the turn.
    pub fn advance_recorder(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<Vec<(airc_lib::WorkCardId, airc_lib::CardState)>>> {
        self.advanced.clone()
    }

    /// Convenience: a `runtime_lookup` closure for
    /// `materialize_adapters` that returns a fresh stub for every
    /// persona_id queried. Substrate-level helper per
    /// [[test-fixtures-are-system-primitives]] — every supervisor
    /// test that exercises materialize_adapters without a real airc
    /// daemon leases this closure shape.
    pub fn fresh_lookup() -> impl Fn(Uuid) -> Option<std::sync::Arc<dyn AircCitizen>> + Clone {
        |_pid| {
            Some(std::sync::Arc::new(Self::new(Uuid::new_v4())) as std::sync::Arc<dyn AircCitizen>)
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
        _room: Option<uuid::Uuid>,
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
        _room: Option<uuid::Uuid>,
    ) -> Result<Option<airc_core::doctrine::RoomDoctrinePublished>, AircError> {
        // No daemon in tests → no published doctrine. Cognition runs
        // through cleanly with no [Room operating doctrine] block.
        Ok(None)
    }
}

#[async_trait]
impl crate::persona::active_work_source::AircWorkReader for StubAircCitizen {
    async fn active_claims(&self) -> Result<Vec<airc_lib::WorkCard>, AircError> {
        // Default: no daemon → no claimed work. A test may seed `with_claims` to
        // exercise the held-work path.
        Ok(self.held.clone())
    }
}

#[async_trait]
impl crate::persona::wall_source::WallReader for StubAircCitizen {
    async fn wall_posts(&self) -> Result<Vec<airc_core::doctrine::WallPostPublished>, AircError> {
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
        // This USED to `unreachable!()` on the premise that nothing drives the
        // stub's subscribe — true when it was written, false since #398 slice 3
        // (bf11a66a7) gave `PersonaSupervisor::materialize` a subscribe call to
        // wire the doctrine/wall cache invalidators. Five supervisor tests have
        // been panicking here ever since; the assertion outlived its premise.
        //
        // Returning `Transport` rather than an empty stream is the honest answer
        // and NOT a fallback: a stub has no transport, and that is exactly the
        // condition the caller already handles explicitly — it keeps both sources
        // uncached ("correct, just slow") and logs loud. So the supervisor tests
        // now exercise the real degradation branch instead of dying, and a stub
        // still never pretends to carry a live stream.
        Err(AircError::Transport(
            "StubAircCitizen has no transport — no event stream to subscribe to".to_string(),
        ))
    }

    async fn say_in(&self, _room_id: Uuid, _text: &str) -> Result<EventId, AircError> {
        unreachable!(
            "StubAircCitizen::say_in must not be called — \
             service-loop tests should reply through StubConversation, \
             not through the citizen handle"
        );
    }

    /// Record the transition instead of hitting a daemon, so a test can assert
    /// the held-work completion edge concluded the right card.
    async fn advance_card_to(
        &self,
        card_id: airc_lib::WorkCardId,
        state: airc_lib::CardState,
    ) -> Result<(), String> {
        self.advanced
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((card_id, state));
        Ok(())
    }

    async fn claim_card(&self, card_id: airc_lib::WorkCardId) -> Result<bool, String> {
        self.claimed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(card_id);
        Ok(true)
    }

    async fn subscribed_rooms(&self) -> Result<Vec<Uuid>, AircError> {
        Ok(self.rooms.clone())
    }

    async fn claimable_cards_in(&self, _room: Uuid, _now_ms: u64) -> Result<Vec<Uuid>, AircError> {
        Ok(self.claimable.clone())
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

    // what this catches: the stub must REFUSE to subscribe, and must refuse in the
    // shape the caller already handles. It used to panic, on the premise that nothing
    // called it — false since #398 slice 3 gave PersonaSupervisor::materialize a
    // subscribe call, which killed 5 supervisor tests for as long as that premise
    // stood. An `Err` keeps the refusal honest AND lets the caller take its documented
    // degradation path (sources stay uncached, logged loud). What must never happen is
    // an Ok(empty stream): that would look like a live subscription that silently never
    // invalidates — the actual fallback.
    // what this catches: the default `membership_epoch` contract (P0 20b44763) —
    // a CLOSED receiver (sender already dropped). The conversation's select loop
    // parks on `pending()` when `changed()` errs; if a future default swapped to
    // a live-but-never-moving channel (or a consumer treated Err as an event),
    // stub-driven conversations would either deadlock waiting on a phantom
    // membership change or busy-loop resubscribing. Closed = "membership never
    // changes", and this pins that both ways.
    #[tokio::test]
    async fn default_membership_epoch_is_a_closed_channel() {
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let mut rx = stub.membership_epoch();
        assert!(
            rx.changed().await.is_err(),
            "default epoch sender must be dropped — consumers park, never poll"
        );
    }

    #[tokio::test]
    async fn stub_subscribe_refuses_rather_than_faking_a_stream() {
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        // `FilteredEventStream` is not Debug, so match rather than `expect_err`.
        match stub.subscribe_all_rooms().await {
            Err(AircError::Transport(_)) => {}
            Err(other) => {
                panic!("refusal must be Transport (what the caller branches on), got: {other:?}")
            }
            Ok(_) => panic!("a stub has no transport — it must not hand back a stream"),
        }
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
