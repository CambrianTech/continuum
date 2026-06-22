//! AircRagSource — delivers a persona's current-channel context to the L1 budget
//! allocator as a consolidated [`ChannelDigest`] (CONCURRENT-MIND §3.3), NOT a raw
//! per-message page.
//!
//! ### Single path, no fallback
//!
//! The `ChannelDigest` is the ONLY representation of channel context
//! ([[consolidate-before-concern-shared-elements-via-cache]]). `deliver` obtains it
//! one of two ways that produce the IDENTICAL shape (so this is lazy-compute-once,
//! not a fallback per [[no-fallbacks-ever]]):
//!
//! - **pre-staged** — [`ChannelDigestRegion`] published it into the shared buffer;
//!   `deliver` peeks the freshest snapshot (the hot path does no work), or
//! - **built once** — not yet staged, so `deliver` builds it via the SAME
//!   `ChannelDigestBuilder` (page_recent → shared elements → bookmark split).
//!
//! `page_recent` survives only as the read primitive *inside* the builder, never as
//! an alternate context path. The old raw `pack_within_budget` + continuation-cursor
//! packing is gone — the digest's window IS the budget shape.
//!
//! ### Why it matters
//!
//! One consumer, one allocator (task #8): the persona's room context is exactly the
//! consolidated digest every other persona shares element-for-element. airc stays
//! the system of record; the digest window only bounds what's pulled into thought
//! by default ([[persona-is-a-client]]).

use std::sync::Arc;

use airc_core::TranscriptEvent;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::cognition::channel_digest::{ChannelDigest, ChannelDigestBuilder, DEFAULT_GROUNDING};
use crate::cognition::channel_digest_region::DigestBuffer;
use crate::cognition::channel_substrate::{
    global_channel_digest_buffer, global_channel_digest_builder,
};
use crate::cognition::channel_element::ChannelElement;
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};
use crate::runtime::ready_buffer::ReadyBuffer;

/// Source identifier — used by budget presets, telemetry, cursor scope checks.
const SOURCE_ID: &str = "airc";

/// Default newest-events fetch cap when building a digest on demand (mirrors the
/// region's). The recipe-defined grounding window slices within this.
const FETCH_LIMIT: usize = 100;

/// Rough chars/token estimate — same heuristic the rest of the budget layer uses.
fn estimate_tokens(content: &str) -> u32 {
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Abstract reader over airc transcript events. Production impl rides on
/// `airc_lib::Airc`; tests use a stub that returns canned events without a daemon.
#[async_trait]
pub trait AircTranscriptReader: Send + Sync {
    /// Return up to `limit` most-recent transcript events, newest-first per airc
    /// convention.
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly via its existing
/// `page_recent` method. Orphan rule OK — the trait is ours.
#[async_trait]
impl AircTranscriptReader for airc_lib::Airc {
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent(self, limit).await
    }
}

/// Persona-bound source delivering the consolidated channel digest.
pub struct AircRagSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircTranscriptReader>,
    builder: Arc<ChannelDigestBuilder>,
    buffer: Arc<DigestBuffer>,
    grounding: usize,
    fetch_limit: usize,
}

impl AircRagSource {
    /// Production constructor — shares the process-global digest substrate so every
    /// persona reuses one element cache + bookmark store + pre-staged buffer.
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircTranscriptReader>) -> Self {
        Self {
            persona_id,
            reader,
            builder: global_channel_digest_builder(),
            buffer: global_channel_digest_buffer(),
            grounding: DEFAULT_GROUNDING,
            fetch_limit: FETCH_LIMIT,
        }
    }

    /// Override the newest-events fetch cap used when building a digest on demand.
    pub fn with_fetch_limit(mut self, fetch_limit: usize) -> Self {
        self.fetch_limit = fetch_limit;
        self
    }

    /// Format a digest into budget-packed `RagItem`s. Walks the window newest-first
    /// accumulating tokens until budget, then emits chronological (oldest-first) so
    /// the chat template reads turns in order. Each item is tagged `unread` vs
    /// grounding so the prompt builder / glass box can tell them apart.
    fn pack_digest(digest: &ChannelDigest, budget: u32) -> (Vec<RagItem>, u32) {
        let mut keep: Vec<usize> = Vec::new();
        let mut tokens_used: u32 = 0;
        for (idx, el) in digest.elements.iter().enumerate().rev() {
            let Some(text) = el.text() else { continue };
            let tokens = estimate_tokens(text);
            if tokens_used.saturating_add(tokens) > budget {
                break;
            }
            tokens_used += tokens;
            keep.push(idx);
        }
        keep.reverse();
        let items = keep
            .into_iter()
            .map(|idx| Self::format_item(&digest.elements[idx], idx >= digest.unread_start))
            .collect();
        (items, tokens_used)
    }

    fn format_item(element: &Arc<ChannelElement>, unread: bool) -> RagItem {
        let ev = element.event();
        let text = element.text().unwrap_or_default().to_string();
        let tokens = estimate_tokens(&text);
        RagItem {
            content: text,
            tokens,
            metadata: serde_json::json!({
                "event_id": ev.event_id.as_uuid().to_string(),
                "room_id": ev.room_id.as_uuid().to_string(),
                "peer_id": ev.peer_id.as_uuid().to_string(),
                "occurred_at_ms": ev.occurred_at_ms,
                "lamport": ev.lamport,
                "unread": unread,
            }),
        }
    }

    fn empty(resolution: ResolutionPreference) -> RagDelivery {
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: resolution,
        }
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
        // Defense in depth: never serve another persona's context.
        if ctx.persona_id != self.persona_id {
            return Self::empty(ResolutionPreference::Placeholder);
        }
        // Page the transcript FIRST, so we can DERIVE the room when the turn's
        // RagContext didn't carry it (compose_for_turn builds it with airc_room=None).
        // page_recent is scoped to the persona's current room, so the events' own
        // room IS the channel — deriving it is correct, not a fallback, and is what
        // keeps the persona from going deaf to the live conversation.
        let events = match self.reader.page_recent(self.fetch_limit).await {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "airc rag: page_recent failed — empty delivery, cognition stays up"
                );
                return Self::empty(ResolutionPreference::Placeholder);
            }
        };
        let Some(room_id) = ctx
            .airc_room
            .map(|r| r.as_uuid())
            .or_else(|| events.last().map(|e| e.room_id.as_uuid()))
        else {
            // No room scope AND no transcript — genuinely nothing to digest.
            return Self::empty(ResolutionPreference::Placeholder);
        };

        // Pre-staged by the region (if it staged this room), else built once now from
        // the events we already paged — identical shape (lazy compute-once).
        let digest = match self.buffer.peek(&(self.persona_id, room_id)) {
            Some(d) => d,
            None => Arc::new(self.builder.build_from_events(
                self.persona_id,
                room_id,
                events,
                self.grounding,
            )),
        };

        let (items, tokens_used) = Self::pack_digest(&digest, budget);
        tracing::debug!(
            persona_id = %self.persona_id,
            room = %room_id,
            window = digest.elements.len(),
            unread = digest.unread().len(),
            budget,
            items_packed = items.len(),
            tokens_used,
            "airc_rag: deliver (digest)"
        );
        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            // The digest IS the window. More history = a command (scrollback/search),
            // not a budget continuation cursor.
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
        // No continuation in the digest model — the consolidated window is the unit.
        // Reaching further back is an explicit scrollback/search command, not a
        // budget-allocator cursor.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::channel_digest::ChannelBookmarks;
    use crate::cognition::channel_element::ChannelElementCache;
    use crate::cognition::embedding::EmbeddingProvider;
    use crate::runtime::ready_buffer::DashMapReadyBuffer;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    struct NoopEmbedder;
    #[async_trait]
    impl EmbeddingProvider for NoopEmbedder {
        fn id(&self) -> &str {
            "noop"
        }
        fn dim(&self) -> usize {
            1
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.0]
        }
    }

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
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    /// Source over an ISOLATED digest substrate (own cache/bookmarks/buffer) so
    /// tests don't touch process globals.
    fn isolated_source(
        reader: Arc<dyn AircTranscriptReader>,
    ) -> (AircRagSource, Arc<ChannelBookmarks>, Arc<DigestBuffer>) {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let bookmarks = Arc::new(ChannelBookmarks::new());
        let builder = Arc::new(ChannelDigestBuilder::new(cache, bookmarks.clone()));
        let buffer = Arc::new(DashMapReadyBuffer::new());
        let source = AircRagSource {
            persona_id: persona(),
            reader,
            builder,
            buffer: buffer.clone(),
            grounding: 0,
            fetch_limit: FETCH_LIMIT,
        };
        (source, bookmarks, buffer)
    }

    fn ctx_in(room: RoomId) -> RagContext {
        let mut c = RagContext::for_persona(persona(), 1_000_000);
        c.substrate.airc_room = Some(room);
        c
    }

    fn event_in(room: RoomId, text: Option<&str>, lamport: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: room,
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_000_000 + lamport,
            lamport,
            target: MentionTarget::Room(room),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: a fresh channel delivers its messages as the consolidated
    // digest window (the single context path), in chronological order.
    #[tokio::test]
    async fn delivers_channel_digest() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![
            event_in(room, Some("hello"), 1),
            event_in(room, Some("world"), 2),
        ]));
        let (source, _, _) = isolated_source(reader);
        let delivery = source.deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[0].content, "hello");
        assert_eq!(delivery.items[1].content, "world");
        assert_eq!(delivery.items[1].metadata.get("unread").and_then(|v| v.as_bool()), Some(true));
    }

    // what this catches: THE DEAF-PERSONA FIX — when the turn's ctx has no airc_room
    // (compose_for_turn sets None), the room is DERIVED from the transcript (page_recent
    // is room-scoped) so the persona still hears the conversation, instead of going
    // deaf. Regression guard for the slice-2 over-strict airc_room requirement.
    #[tokio::test]
    async fn no_room_scope_derives_room_from_transcript() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("hi"), 1)]));
        let (source, _, _) = isolated_source(reader);
        let ctx = RagContext::for_persona(persona(), 1_000_000); // airc_room = None
        let delivery = source.deliver(&ctx, 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1, "derives the room from the transcript, not deaf");
        assert_eq!(delivery.items[0].content, "hi");
    }

    // what this catches: genuinely nothing — no room scope AND no transcript → empty.
    #[tokio::test]
    async fn no_room_no_transcript_delivers_empty() {
        let reader = Arc::new(StubReader::new(vec![]));
        let (source, _, _) = isolated_source(reader);
        let ctx = RagContext::for_persona(persona(), 1_000_000);
        let delivery = source.deliver(&ctx, 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: a pre-staged digest in the buffer is served WITHOUT
    // rebuilding (the hot path peeks the region's snapshot). We seed the buffer with
    // a digest the reader could not have produced, and confirm it's what's served.
    #[tokio::test]
    async fn serves_prestaged_digest_without_rebuild() {
        let room = RoomId::new();
        // Reader would return "live"; buffer holds a pre-staged "staged".
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("live"), 9)]));
        let (source, bookmarks, buffer) = isolated_source(reader);
        // Build a staged digest via a separate builder over the SAME-shape elements.
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let staged_builder = ChannelDigestBuilder::new(cache, bookmarks);
        let staged_reader = StubReader::new(vec![event_in(room, Some("staged"), 1)]);
        let staged = staged_builder
            .build(persona(), room.as_uuid(), &staged_reader, 100, 0)
            .await
            .unwrap();
        buffer.publish((persona(), room.as_uuid()), Arc::new(staged));

        let delivery = source.deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(delivery.items[0].content, "staged", "served the pre-staged digest, not a rebuild");
    }

    // what this catches: cross-persona ctx is refused (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_refused() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("secret"), 1)]));
        let (source, _, _) = isolated_source(reader);
        let mut other = RagContext::for_persona(Uuid::new_v4(), 1_000_000);
        other.substrate.airc_room = Some(room);
        let delivery = source.deliver(&other, 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: a reader error degrades to empty (cognition stays up), no
    // panic, no fallback to a raw path.
    #[tokio::test]
    async fn reader_error_delivers_empty() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("x"), 1)]));
        reader.set_fail(true);
        let (source, _, _) = isolated_source(reader);
        let delivery = source.deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: budget caps the window — only the newest messages that fit
    // are packed, and there is NO continuation cursor (the digest is the unit).
    #[tokio::test]
    async fn budget_caps_window_no_continuation() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![
            event_in(room, Some("aaaaa"), 1),
            event_in(room, Some("bbbbb"), 2),
            event_in(room, Some("ccccc"), 3),
        ]));
        let (source, _, _) = isolated_source(reader);
        let delivery = source.deliver(&ctx_in(room), 4, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 2, "two newest fit budget 4");
        assert!(delivery.continuation.is_none(), "digest model has no continuation cursor");
    }
}
