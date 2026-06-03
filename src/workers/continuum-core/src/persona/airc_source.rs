//! AircRagSource — reads real airc TranscriptEvents from the persona's
//! current room and packages them as RagItems for the L1 budget
//! allocator.
//!
//! Per Joel (2026-05-31): "see how a real rag from airc would look."
//!
//! ### Architecture
//!
//! Abstracts an `AircTranscriptReader` trait that exposes the single
//! `page_recent(limit)` operation. The real implementation rides on
//! `airc_lib::Airc::page_recent`; test doubles stub it out so unit
//! tests don't need a running airc daemon. This is the same
//! polymorphism rails per [[organization-purity-as-we-migrate]] —
//! adapter-first methodology: ship the trait + one heuristic
//! implementation + a stub for tests.
//!
//! ### Why it matters
//!
//! `EngramSource` proves the trait against the in-process engram
//! store. `AircRagSource` proves it against actual airc message
//! data the persona is hosting on the substrate. Together they
//! demonstrate the trait shape composes against multiple real-
//! world backing stores without source changes to either the
//! allocator or the assembly layer. This is the substrate's
//! "every base model includable + every data source pluggable"
//! thesis in code form (per
//! [[docs/architecture/EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md]]).
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: errors from the
//!   reader return an empty delivery + tracing::warn — cognition
//!   stays up even when airc subsystem is degraded
//! - [[RTOS-brain-no-region-on-hot-path]]: page_recent goes through
//!   the reader trait; production impl handles its own async I/O;
//!   the cognition hot path doesn't block on airc
//! - Persona-scoped at construction: cross-persona ctx returns
//!   empty (defense in depth, same shape as EngramSource)

use std::sync::Arc;

use airc_core::TranscriptEvent;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — used by budget presets, telemetry, cursor
/// scope checks.
const SOURCE_ID: &str = "airc";

/// Rough chars/token estimate — same heuristic EngramSource uses.
/// Real tokenizer integration lands in slice 12+.
fn estimate_tokens(content: &str) -> u32 {
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Abstract reader over airc transcript events. Production impl
/// rides on `airc_lib::Airc`; tests use a stub that returns canned
/// events without needing a daemon.
#[async_trait]
pub trait AircTranscriptReader: Send + Sync {
    /// Return up to `limit` most-recent transcript events, newest-
    /// first per airc convention.
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly via its
/// existing `page_recent` method. Orphan rule OK — the trait is
/// ours (defined in this crate).
#[async_trait]
impl AircTranscriptReader for airc_lib::Airc {
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent(self, limit).await
    }
}

/// AircRagSource — persona-bound, reads from any `AircTranscriptReader`.
pub struct AircRagSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircTranscriptReader>,
    /// Maximum events to fetch per deliver call. Production default
    /// = 100; tests can configure smaller. The L1 budget allocator
    /// determines how many of these get included in the prompt; the
    /// fetch cap is a separate concern (don't hammer airc for 10k
    /// events when the budget only fits 20).
    fetch_limit: usize,
}

impl AircRagSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircTranscriptReader>) -> Self {
        Self {
            persona_id,
            reader,
            fetch_limit: 100,
        }
    }

    pub fn with_fetch_limit(mut self, fetch_limit: usize) -> Self {
        self.fetch_limit = fetch_limit;
        self
    }

    /// Extract a text representation from a TranscriptEvent's body.
    /// Returns `None` for events without a text body — they're
    /// skipped (non-text events don't belong in a text-only prompt
    /// at slice 10.6 fidelity; future slices may add multimodal
    /// items).
    fn extract_text(event: &TranscriptEvent) -> Option<String> {
        let body = event.body.as_ref()?;
        body.as_text().map(|s| s.to_string())
    }

    /// Format one event as RagItem content. Slice 10.6 uses just the
    /// text body. Future slices may add structured prefixes (peer
    /// alias, room nick, timestamp) as the prompt-assembly contract
    /// firms up.
    fn format_item(event: &TranscriptEvent, text: String, score: f32) -> RagItem {
        let tokens = estimate_tokens(&text);
        RagItem {
            content: text,
            tokens,
            metadata: serde_json::json!({
                "event_id": event.event_id.as_uuid().to_string(),
                "room_id": event.room_id.as_uuid().to_string(),
                "peer_id": event.peer_id.as_uuid().to_string(),
                "occurred_at_ms": event.occurred_at_ms,
                "lamport": event.lamport,
                "score": score,
            }),
        }
    }

    /// Pack ranked events into RagItems within budget. Returns
    /// (items, tokens_used, last_lamport_consumed). The last_lamport
    /// is what the continuation cursor carries for resume.
    /// Pack as many of the newest events as fit in `budget`.
    ///
    /// `page_recent(limit)` returns the newest N events in chronological
    /// (lamport-ascending) order — events[0] is the OLDEST of the N
    /// newest, events[N-1] is the very newest. If we packed
    /// oldest-first we'd drop the newest events on budget overflow —
    /// catastrophic for cognition, which exists to respond to the
    /// MOST RECENT message. Per [[no-fallbacks-ever]] the substrate
    /// makes the right choice deterministically: walk backwards from
    /// the newest accumulating tokens, stop when budget would be
    /// exceeded, then re-emit in chronological order so the LLM sees
    /// the conversation as humans wrote it.
    ///
    /// `start_rank` is kept for the continuation cursor surface but
    /// in the cognition hot path (continuation not used) it's 0 →
    /// the function "tails" the newest budget-worth.
    fn pack_within_budget(
        events: &[TranscriptEvent],
        start_rank: usize,
        budget: u32,
    ) -> (Vec<RagItem>, u32, usize) {
        let scope = &events[start_rank.min(events.len())..];

        // Walk backwards (newest first), collecting indices that fit.
        let mut keep_indices: Vec<usize> = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut oldest_kept = scope.len();
        for (offset, event) in scope.iter().enumerate().rev() {
            let Some(text) = Self::extract_text(event) else {
                continue;
            };
            let tokens = estimate_tokens(&text);
            if tokens_used.saturating_add(tokens) > budget {
                break;
            }
            tokens_used += tokens;
            keep_indices.push(offset);
            oldest_kept = offset;
        }
        // Reverse → chronological order (oldest first within the kept
        // window). The model's chat-template-built prompt reads the
        // user turns in order, so this is the order they should be
        // appended.
        keep_indices.reverse();

        let mut items = Vec::with_capacity(keep_indices.len());
        for offset in keep_indices {
            let event = &scope[offset];
            let text = Self::extract_text(event).expect("non-text events filtered above");
            // Recency-only scoring: each event gets its 1/(rank+1)
            // score where rank is its position in the ORIGINAL
            // events slice (newer == lower rank == higher score).
            let absolute_idx = start_rank + offset;
            let score = 1.0 / (absolute_idx as f32 + 1.0);
            items.push(Self::format_item(event, text, score));
        }
        // `next_rank` is the absolute index of the oldest event we
        // KEPT. Continuation (when reused) pages backwards from
        // there — older messages, same persona/source. For the
        // cognition hot path the continuation cursor is unused.
        let next_rank = start_rank + oldest_kept;
        (items, tokens_used, next_rank)
    }
}

#[async_trait]
impl RagSource for AircRagSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: SOURCE_ID.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }
        let events = match self.reader.page_recent(self.fetch_limit).await {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "airc rag: page_recent failed — returning empty delivery, cognition stays up"
                );
                return RagDelivery {
                    source_id: SOURCE_ID.to_string(),
                    items: Vec::new(),
                    tokens_used: 0,
                    continuation: None,
                    resolution_used: ResolutionPreference::Placeholder,
                };
            }
        };
        let (items, tokens_used, next_rank) = Self::pack_within_budget(&events, 0, budget);

        tracing::info!(
            persona_id = %self.persona_id,
            fetch_limit = self.fetch_limit,
            events_returned = events.len(),
            budget,
            items_packed = items.len(),
            tokens_used,
            "airc_rag: deliver — diagnostic for items_count=0 mystery"
        );

        let continuation = if next_rank < events.len() {
            Some(ContinuationCursor {
                persona_id: self.persona_id,
                source_id: SOURCE_ID.to_string(),
                opaque: serde_json::json!({ "next_rank": next_rank }),
            })
        } else {
            None
        };
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            continuation,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        ctx: &RagContext,
        cursor: ContinuationCursor,
        budget: u32,
    ) -> Option<RagDelivery> {
        if ctx.persona_id != self.persona_id {
            return None;
        }
        if cursor.persona_id != self.persona_id {
            return None;
        }
        if cursor.source_id != SOURCE_ID {
            return None;
        }
        let next_rank: usize = cursor.opaque.get("next_rank")?.as_u64()? as usize;
        let events = match self.reader.page_recent(self.fetch_limit).await {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "airc rag: page_recent failed during continuation"
                );
                return None;
            }
        };
        if next_rank >= events.len() {
            return None;
        }
        let (items, tokens_used, new_next_rank) =
            Self::pack_within_budget(&events, next_rank, budget);
        let continuation = if new_next_rank < events.len() {
            Some(ContinuationCursor {
                persona_id: self.persona_id,
                source_id: SOURCE_ID.to_string(),
                opaque: serde_json::json!({ "next_rank": new_next_rank }),
            })
        } else {
            None
        };
        Some(RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            continuation,
            resolution_used: ResolutionPreference::Raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    /// Test double — returns pre-canned events. Optionally returns an
    /// error to simulate airc subsystem failure.
    struct StubReader {
        events: Vec<TranscriptEvent>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            if *self.fail.lock().unwrap() {
                // AircError doesn't have a Custom variant; use any
                // trivially-constructable variant to simulate failure.
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    fn make_event(text: Option<&str>, lamport: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_000_000 + lamport,
            lamport,
            target: MentionTarget::Room(RoomId::new()),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // ---- TDD tests ----

    #[tokio::test]
    async fn empty_room_delivers_nothing() {
        let reader = Arc::new(StubReader::new(vec![]));
        let source = AircRagSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(delivery.continuation.is_none());
    }

    #[tokio::test]
    async fn single_text_message_surfaces() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("hello world"), 1)]));
        let source = AircRagSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(delivery.items[0].content, "hello world");
        assert!(delivery.items[0].metadata.get("event_id").is_some());
    }

    #[tokio::test]
    async fn non_text_events_dropped() {
        // Two events: one with no body (skip), one with text (keep).
        let reader = Arc::new(StubReader::new(vec![
            make_event(None, 1),
            make_event(Some("kept"), 2),
        ]));
        let source = AircRagSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(delivery.items[0].content, "kept");
    }

    #[tokio::test]
    async fn budget_overflow_returns_continuation() {
        // Three messages, budget too small for all three.
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("aaaaa"), 1), // ~2 tokens
            make_event(Some("bbbbb"), 2), // ~2 tokens
            make_event(Some("ccccc"), 3), // ~2 tokens
        ]));
        let source = AircRagSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 4, ResolutionPreference::Raw).await;
        // First fits, second fits (cumulative 4), third doesn't.
        assert_eq!(delivery.items.len(), 2);
        assert!(delivery.continuation.is_some());
    }

    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("secret"), 1)]));
        let source = AircRagSource::new(persona(), reader);
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(
                &RagContext::for_persona(other, 1_000_000),
                1_000,
                ResolutionPreference::Raw,
            )
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    #[tokio::test]
    async fn cross_persona_cursor_refused() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("a"), 1)]));
        let source = AircRagSource::new(persona(), reader);
        let other = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let alien_cursor = ContinuationCursor {
            persona_id: other,
            source_id: SOURCE_ID.to_string(),
            opaque: serde_json::json!({ "next_rank": 0 }),
        };
        let result = source.deliver_continuation(&ctx(), alien_cursor, 1_000).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn wrong_source_id_cursor_refused() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("a"), 1)]));
        let source = AircRagSource::new(persona(), reader);
        let alien_cursor = ContinuationCursor {
            persona_id: persona(),
            source_id: "memories".to_string(),
            opaque: serde_json::json!({ "next_rank": 0 }),
        };
        let result = source.deliver_continuation(&ctx(), alien_cursor, 1_000).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn reader_error_returns_empty_with_no_panic() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("won't be served"), 1)]));
        reader.set_fail(true);
        let source = AircRagSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        // No panic — substrate stays a good citizen even when airc is
        // degraded.
    }

    #[tokio::test]
    async fn continuation_resumes_from_next_rank() {
        // 5-char items so each is ~2 tokens; budget 4 fits 2, forces
        // continuation for the remaining 2.
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("aaaaa"), 1),
            make_event(Some("bbbbb"), 2),
            make_event(Some("ccccc"), 3),
            make_event(Some("ddddd"), 4),
        ]));
        let source = AircRagSource::new(persona(), reader);
        let first = source.deliver(&ctx(), 4, ResolutionPreference::Raw).await;
        assert!(!first.items.is_empty());
        let cursor = first.continuation.expect("expected continuation");
        let second = source
            .deliver_continuation(&ctx(), cursor, 1_000)
            .await
            .expect("continuation should yield");
        assert_eq!(
            first.items.len() + second.items.len(),
            4,
            "all events should surface across the two calls"
        );
    }

    #[tokio::test]
    async fn fetch_limit_caps_reader_call() {
        // 5 events available, source configured to fetch only 3.
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("a"), 1),
            make_event(Some("b"), 2),
            make_event(Some("c"), 3),
            make_event(Some("d"), 4),
            make_event(Some("e"), 5),
        ]));
        let source = AircRagSource::new(persona(), reader).with_fetch_limit(3);
        let delivery = source.deliver(&ctx(), 10_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 3, "fetch_limit caps the working set");
    }
}
