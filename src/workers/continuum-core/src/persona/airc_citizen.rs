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
use airc_lib::{AircError, EventId, EventStream};
use async_trait::async_trait;
use uuid::Uuid;

/// The substrate's universal airc handle. Implemented by
/// [`PersonaAircRuntime`](super::airc_runtime::PersonaAircRuntime) for
/// production and by [`StubAircCitizen`] for tests; future BaseUser
/// variants (human, browser) impl it via their own airc-lib wrappers.
///
/// `AircCitizen: AircTranscriptReader` — every citizen can page her
/// own transcript history. Rust 1.86+ trait_upcasting means
/// `Arc<dyn AircCitizen>` coerces directly to
/// `Arc<dyn AircTranscriptReader>`; no explicit conversion needed.
#[async_trait]
pub trait AircCitizen: AircTranscriptReader {
    /// The airc-side peer identity (Ed25519 pubkey, formatted as Uuid).
    /// Cognition uses this for self-loop filtering; the supervisor uses
    /// it as part of the persona's tracing span.
    fn peer_id(&self) -> Uuid;

    /// Open a live event stream on the citizen's default room. The
    /// stream yields every event the citizen sees — including her own
    /// echoes; consumers self-filter via `peer_id()`. Used by
    /// [`AircPersonaConversation::next_message`](super::airc_persona_conversation::AircPersonaConversation)
    /// to drive the service loop.
    async fn subscribe(&self) -> Result<EventStream, AircError>;

    /// Publish a text message under the citizen's identity in her
    /// default room. Returns the daemon-assigned event id so callers
    /// can correlate with the subscribe stream's echo (not required
    /// today, but the wire shape preserves it).
    async fn say(&self, text: &str) -> Result<EventId, AircError>;
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
impl AircCitizen for StubAircCitizen {
    fn peer_id(&self) -> Uuid {
        self.peer_id
    }

    async fn subscribe(&self) -> Result<EventStream, AircError> {
        // No service-loop test drives the stub's subscribe — the
        // service loop receives messages through StubConversation
        // directly, never through the citizen's stream. If a future
        // test ever wires the stub into the conversation, this panics
        // visibly per [[no-fallbacks-ever]] rather than silently
        // returning an empty stream or fabricating an AircError
        // variant that doesn't fit ("Transport"/"Route"/etc).
        unreachable!(
            "StubAircCitizen::subscribe must not be called — \
             service-loop tests should drive the loop through \
             StubConversation directly, not through the citizen handle"
        );
    }

    async fn say(&self, _text: &str) -> Result<EventId, AircError> {
        unreachable!(
            "StubAircCitizen::say must not be called — \
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

    #[tokio::test]
    #[should_panic(expected = "service-loop tests should drive the loop")]
    async fn stub_subscribe_panics_loudly() {
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let _ = stub.subscribe().await;
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
