//! RoomDoctrineSource — reads the airc room operating doctrine and
//! packages it as a `[Room operating doctrine]` grounding block.
//!
//! ### Why this source exists (slice 2)
//!
//! Slice 1 ([`RoomRosterSource`](super::room_roster_source)) grounds a
//! persona in WHO else is present. This grounds it in WHAT KIND of room
//! it is — the activity's operating contract. Rooms are the universal
//! activity primitive (chat, dev-coordination, game, academy, help,
//! settings); a coordination room is not a free-for-all chat. Ivar's
//! *other* failure was over-participating in a coordination room because
//! it had no signal about the room's nature.
//!
//! airc already owns this: `Airc::room_doctrine` returns the latest
//! `RoomDoctrinePublished` for the current room — markdown the airc-core
//! docs explicitly say agents should "render verbatim / inject as a
//! system message into agent context." This source is that injection,
//! routed through the same RAG-grounding + capture/replay path as the
//! roster. Thin continuum: we read airc's doctrine, we don't invent a
//! room-nature concept. See
//! [[docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md]] §5 slice 2.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed/absent
//!   doctrine read returns an empty delivery — cognition stays up; a
//!   room with no published doctrine simply renders no block.
//! - Persona-scoped at construction (defense in depth, same as the
//!   roster + engram sources).
//! - Atomic unit = the doctrine body; no pagination (one current
//!   contract per room).

use std::sync::Arc;

use airc_core::doctrine::RoomDoctrinePublished;
use airc_core::PeerId;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — the service-loop projection routes this delivery
/// into system-prompt grounding (a `[Room operating doctrine]` block).
const SOURCE_ID: &str = "room-doctrine";

/// Rough chars/token estimate — same heuristic the other RAG sources
/// use. Real tokenizer integration lands in slice 12+.
fn estimate_tokens(content: &str) -> u32 {
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Abstract reader over the airc room operating doctrine. Production
/// rides on `airc_lib::Airc::room_doctrine`; tests stub it without a
/// daemon. Mirrors the `AircRosterReader` rail.
#[async_trait]
pub trait AircDoctrineReader: Send + Sync {
    /// The latest published operating doctrine for this persona's
    /// current room, or `None` if none has been published.
    async fn room_doctrine(&self) -> Result<Option<RoomDoctrinePublished>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule
/// OK — the trait is ours.
#[async_trait]
impl AircDoctrineReader for airc_lib::Airc {
    async fn room_doctrine(&self) -> Result<Option<RoomDoctrinePublished>, AircError> {
        airc_lib::Airc::room_doctrine(self).await
    }
}

/// RoomDoctrineSource — persona-bound, reads the room doctrine from any
/// `AircDoctrineReader`.
pub struct RoomDoctrineSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircDoctrineReader>,
}

impl RoomDoctrineSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircDoctrineReader>) -> Self {
        Self { persona_id, reader }
    }

    /// Fit the doctrine body to `budget` tokens. A doctrine is a single
    /// contract; if it doesn't fit we deliver a truncated prefix (with a
    /// marker) rather than dropping it entirely — partial guidance still
    /// grounds the persona in the room's nature. Returns `None` only for
    /// a zero budget.
    fn fit_body(body: &str, budget: u32) -> Option<String> {
        if budget == 0 {
            return None;
        }
        if estimate_tokens(body) <= budget {
            return Some(body.to_string());
        }
        // ~4 chars/token; leave room for the truncation marker.
        const MARKER: &str = "\n…[doctrine truncated]";
        let char_budget = (budget as usize).saturating_mul(4).saturating_sub(MARKER.len());
        let mut truncated: String = body.chars().take(char_budget).collect();
        truncated.push_str(MARKER);
        Some(truncated)
    }
}

#[async_trait]
impl RagSource for RoomDoctrineSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let empty = |res| RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: res,
        };

        // Persona-scoped (defense in depth, same shape as the roster).
        if ctx.persona_id != self.persona_id {
            return empty(ResolutionPreference::Placeholder);
        }

        let card = match self.reader.room_doctrine().await {
            Ok(Some(card)) => card,
            // No doctrine published for this room → no block (normal).
            Ok(None) => return empty(resolution),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "room_doctrine: read failed — empty delivery, cognition stays up"
                );
                return empty(ResolutionPreference::Placeholder);
            }
        };

        let Some(content) = Self::fit_body(&card.body, budget) else {
            return empty(resolution);
        };
        let tokens = estimate_tokens(&content);

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            version = %card.version,
            tokens,
            "room_doctrine: deliver"
        );

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: vec![RagItem {
                content,
                tokens,
                metadata: serde_json::json!({
                    "version": card.version,
                    "published_by": card.published_by.as_uuid().to_string(),
                    "published_at_ms": card.published_at_ms,
                }),
            }],
            tokens_used: tokens,
            // One current contract per room; no pagination.
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        // Doctrine is a single current snapshot; any cursor is stale.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn card(body: &str) -> RoomDoctrinePublished {
        RoomDoctrinePublished {
            room_id: airc_core::RoomId::new(),
            body: body.to_string(),
            version: "v1abc".to_string(),
            published_by: PeerId::new(),
            published_at_ms: 1_000_000,
        }
    }

    struct StubReader {
        doctrine: Option<RoomDoctrinePublished>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(doctrine: Option<RoomDoctrinePublished>) -> Self {
            Self {
                doctrine,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl AircDoctrineReader for StubReader {
        async fn room_doctrine(&self) -> Result<Option<RoomDoctrinePublished>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.doctrine.clone())
        }
    }

    // what this catches: a published doctrine surfaces as a delivery the
    // service loop can route into the [Room operating doctrine] grounding
    // block — the fix for a persona ignoring the room's nature.
    #[tokio::test]
    async fn published_doctrine_surfaces() {
        let reader = Arc::new(StubReader::new(Some(card(
            "This is a coordination room. Respond sparingly; do not chat.",
        ))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.items[0].content.contains("Respond sparingly"));
        assert_eq!(delivery.items[0].metadata["version"], "v1abc");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a room with NO published doctrine renders no
    // block (backwards-compatible; most rooms have none yet).
    #[tokio::test]
    async fn no_doctrine_delivers_nothing() {
        let reader = Arc::new(StubReader::new(None));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the doctrine subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(Some(card("body"))));
        reader.set_fail(true);
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(Some(card("body"))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(
                &RagContext::for_persona(alien, 1_000_000),
                1_000,
                ResolutionPreference::Raw,
            )
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: an over-budget doctrine is truncated (with a
    // marker) and never overspends — partial guidance beats dropping the
    // room's operating contract entirely.
    #[tokio::test]
    async fn oversized_doctrine_truncates_within_budget() {
        let big = "x".repeat(10_000);
        let reader = Arc::new(StubReader::new(Some(card(&big))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 20, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.tokens_used <= 20, "must not overspend the budget");
        assert!(delivery.items[0].content.contains("truncated"));
    }
}
