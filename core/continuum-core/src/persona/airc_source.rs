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
use crate::cognition::channel_element::ChannelElement;
use crate::cognition::channel_substrate::{
    global_channel_digest_buffer, global_channel_digest_builder,
};
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};
use crate::runtime::ready_buffer::ReadyBuffer;

/// Source identifier — used by budget presets, telemetry, cursor scope checks.
const SOURCE_ID: &str = "airc";

/// Default newest-events fetch cap when building a digest on demand (mirrors the
/// region's). The recipe-defined grounding window slices within this.
const FETCH_LIMIT: usize = 100;

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::{estimate_prompt_tokens as estimate_tokens, head_to_tokens};

/// Abstract reader over airc transcript events. Production impl rides on
/// `airc_lib::Airc`; tests use a stub that returns canned events without a daemon.
#[async_trait]
pub trait AircTranscriptReader: Send + Sync {
    /// Return up to `limit` most-recent CONVERSATIONAL transcript events
    /// (Message + Attachment), newest-first per airc convention.
    ///
    /// Kinds are filtered BEFORE the page limit (#297): a raw newest-`limit`
    /// page counts ephemeral StreamChunk frames (~4/sec per talking persona),
    /// so active residents' own streaming evicted every durable message from
    /// their window within a minute — working personas were DEAF to direction
    /// while the room was busy (glass-boxed live 2026-08-01: a resident asked
    /// the same question 3× because four direction messages never entered her
    /// page while the attach cursor advanced normally). The diagnostic
    /// signature: cross-machine delivery perfect, local perception stale —
    /// the wire is fine, the WINDOW is flooded. Presence / receipts /
    /// lifecycle ride their own sources; this page is the room's
    /// conversation, never its firehose.
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError>;

    /// Room-scoped page (#367): return the same conversational page, but for
    /// the given room when `Some` — the TURN's room, not the reader's
    /// current-room pointer. `page_recent` follows the persona's landing-room
    /// pointer, so a turn happening anywhere else (a bench room, a project
    /// room she is subscribed to but not "in") paged the DEFAULT room's
    /// transcript — and everything downstream (digest, derived-room fallback,
    /// the recall query built from this window) followed the wrong room.
    ///
    /// Default impl ignores `room` and delegates — correct ONLY for test
    /// stubs whose canned events have no room dimension. Every production
    /// reader overrides this (the #262 lesson in `AircHandleAdapter`: a
    /// silently-inherited default is how regressions ship).
    async fn page_recent_in(
        &self,
        room: Option<airc_core::RoomId>,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        let _ = room;
        self.page_recent(limit).await
    }
}

/// The kinds a perception page means: the room's conversation. ONE place
/// (compression) — every reader impl funnels through the [`airc_lib::Airc`]
/// impl below, which applies this filter.
pub fn perception_page_filter() -> airc_lib::EventFilter {
    let mut filter = airc_lib::EventFilter::current_room();
    filter.kinds.insert(airc_core::TranscriptKind::Message);
    filter.kinds.insert(airc_core::TranscriptKind::Attachment);
    filter
}

/// The same conversational-kinds filter, pinned to a specific room (#367).
/// `None` leaves the channel unset, which `page_recent_filtered` scopes to
/// the current room — the pre-#367 behavior, still right for genuinely
/// room-less work.
pub fn perception_page_filter_in(room: Option<airc_core::RoomId>) -> airc_lib::EventFilter {
    let mut filter = perception_page_filter();
    filter.channel = room;
    filter
}

/// `airc_lib::Airc` satisfies the reader contract via the kinds-filtered
/// page (daemon-side newest-N-of-kind since airc PR #1314). Orphan rule OK —
/// the trait is ours.
#[async_trait]
impl AircTranscriptReader for airc_lib::Airc {
    async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent_filtered(self, perception_page_filter(), limit).await
    }

    async fn page_recent_in(
        &self,
        room: Option<airc_core::RoomId>,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        airc_lib::Airc::page_recent_filtered(self, perception_page_filter_in(room), limit).await
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
    /// #249: durable-transcript top-up for a shallow live window (post-reboot the
    /// persona's airc runtime log holds only events since ITS boot). `Some` in
    /// production (default); tests without a store see a loud-skip, not a panic.
    history: Option<Arc<dyn crate::persona::durable_history::DurableRoomHistory>>,
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
            history: Some(Arc::new(crate::persona::durable_history::ChatStoreHistory)),
        }
    }

    /// Floor of tokens any packed turn can occupy (sender prefix + separators
    /// alone — a physical minimum, not a policy). Used ONLY to size the
    /// candidate window handed to the token packer: `budget / floor`
    /// candidates can never under-supply it, so the TOKEN budget — never a
    /// message count — is the binding constraint on what the persona sees.
    /// The 2026-07-30 glass box: a 5-message grounding pre-trim starved
    /// pack_digest into a 3–5-message world view while the L1 budget could
    /// hold dozens — the persona then confabulated generic-assistant filler
    /// because the actual conversation was invisible (#259).
    // context-budget-exempt: a FLOOR under a per-turn allocation — it only ever raises, so a large window is never clamped by it
    const MIN_TOKENS_PER_TURN: u32 = 8;

    /// Turns-that-fit grounding: derive the digest's before-bookmark window
    /// from the delivery budget. `recipe_floor` (the recipe-defined N, default
    /// [`DEFAULT_GROUNDING`]) is the floor; the ceiling bounds the durable
    /// top-up fetch, not what the packer may keep.
    fn grounding_for_budget(budget: u32, recipe_floor: usize) -> usize {
        ((budget / Self::MIN_TOKENS_PER_TURN) as usize).clamp(recipe_floor, 256)
    }

    /// Override (or disable) the durable-history top-up — tests inject a stub;
    /// `None` turns hydration off entirely.
    pub fn with_history(
        mut self,
        history: Option<Arc<dyn crate::persona::durable_history::DurableRoomHistory>>,
    ) -> Self {
        self.history = history;
        self
    }

    /// Synthesize a grounding-only `TranscriptEvent` from a durable transcript
    /// line. Lamport 0 puts it strictly BEFORE any live event after the split
    /// sort (which is stable, so hydrated lines keep their chronological input
    /// order among themselves) — hydrated history can therefore only ever land
    /// on the grounding side of the bookmark, never as unread. That is the #242
    /// contract: history is context, never fresh perception.
    fn hydrated_event(room_id: uuid::Uuid, sender: uuid::Uuid, text: &str) -> TranscriptEvent {
        use airc_core::{
            Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
        };
        let room = RoomId::from_uuid(room_id);
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: room,
            peer_id: PeerId::from_uuid(sender),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 0,
            lamport: 0,
            target: MentionTarget::Room(room),
            headers: Headers::default(),
            body: Some(Body::text(text)),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
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
    ///
    /// BREADTH OVER VERBATIM DEPTH (#128/#146, measured 2026-07-13): the original
    /// packer kept WHOLE messages newest-first, so a small budget in a verbose
    /// four-persona room held ~2-3 turns TOTAL — the persona's entire perceivable
    /// world. Any low-frequency speaker (the operator) was displaced within
    /// seconds; the room degenerated into parallel monologues that only rapid-fire
    /// exchange survived. A conversation is legible from turn HEADS; it is not
    /// legible from two verbatim essays. So each turn now costs at most a
    /// per-turn cap (a budget fraction, never a hardcoded model tier) and long
    /// turns are head-trimmed with an explicit marker — the same
    /// straddling-trim law the prompt fitter applies to messages, one level down.
    /// The NEWEST turn is exempt (kept verbatim up to the whole budget): it is
    /// what the persona is responding to.
    fn pack_digest(digest: &ChannelDigest, budget: u32) -> (Vec<RagItem>, u32) {
        // Per-turn cap: budget/8 → a useful window holds ~8+ turns; clamped so
        // tiny budgets still render a sentence and huge ones don't let one
        // essay crowd the window.
        let per_turn_cap = (budget / 8).clamp(48, 256);
        let mut keep: Vec<(usize, Option<String>)> = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut newest_kept = true;
        for (idx, el) in digest.elements.iter().enumerate().rev() {
            let Some(text) = el.text() else { continue };
            let full = estimate_tokens(text);
            let cap = if newest_kept { budget } else { per_turn_cap };
            let (cost, trimmed) = if full <= cap {
                (full, None)
            } else {
                let head = head_to_tokens(text, cap);
                let head_cost = estimate_tokens(&head).saturating_add(2); // marker
                (
                    head_cost,
                    Some(format!("{head} (…{full}-token message trimmed)")),
                )
            };
            if tokens_used.saturating_add(cost) > budget {
                break;
            }
            newest_kept = false;
            tokens_used += cost;
            keep.push((idx, trimmed));
        }
        keep.reverse();
        let items = keep
            .into_iter()
            .map(|(idx, trimmed)| {
                Self::format_item(&digest.elements[idx], idx >= digest.unread_start, trimmed)
            })
            .collect();
        (items, tokens_used)
    }

    fn format_item(
        element: &Arc<ChannelElement>,
        unread: bool,
        trimmed: Option<String>,
    ) -> RagItem {
        let ev = element.event();
        let text = trimmed.unwrap_or_else(|| element.text().unwrap_or_default().to_string());
        let tokens = estimate_tokens(&text);
        RagItem {
            content: text,
            tokens,
            metadata: serde_json::json!({
                "event_id": ev.event_id.as_uuid().to_string(),
                "room_id": ev.room_id.as_uuid().to_string(),
                // The LOGICAL author: a chat/send line is attributed to the human/web
                // identity that wrote it (envelope senderId), not the core's relay
                // peer — so the rendered room reads "Joel: …", never "core: …" (#177).
                "peer_id": element.sender_id().to_string(),
                "occurred_at_ms": ev.occurred_at_ms,
                "lamport": ev.lamport,
                "unread": unread,
                // The digest is a chronological window, not a ranked retrieval, so
                // "relevance" here IS the attention signal: an unread message the
                // persona must attend to (1.0) vs. an older grounding element kept
                // only for context (0.5). The glass box (rag_inspect) surfaces this
                // as the item score; without it the inspect layer silently saw 0.0.
                "score": if unread { 1.0 } else { 0.5 },
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

    fn expand_command(&self) -> Option<&'static str> {
        Some("collaboration/chat/export")
    }

    /// One conversation turn — a speaker and what they said. The recent-history
    /// appetite is much larger and is expressed as `min`, not as this floor;
    /// conflating the two is what made every source all-or-nothing.
    fn floor_tokens(&self) -> u32 {
        64
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
        // Page the TURN's room when the RagContext carries it (#367). Before this,
        // page_recent always followed the persona's current-room POINTER (her
        // landing room), so a turn happening in any other room — a bench room, a
        // project room she's subscribed to — delivered the DEFAULT room's
        // transcript, and everything downstream (digest, derived-room fallback,
        // the recall query built from this window) followed the wrong room.
        // When airc_room is None (room-less work: consolidation, dreams), the
        // pointer-scoped page remains correct and the events' own room is
        // DERIVED below — that fallback is legitimate, not a shim.
        let events = match self
            .reader
            .page_recent_in(ctx.airc_room, self.fetch_limit)
            .await
        {
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

        // #249: when the LIVE window is shallower than the grounding target (the
        // post-reboot shape — the persona's airc runtime log restarted while the
        // room's real history lives in the durable store), top up from the durable
        // transcript. Hydrated lines enter at lamport 0 (grounding-only, see
        // `hydrated_event`) and are deduped against live events by body text +
        // sender (the durable row id is the airc event id for persona lines, but
        // live events re-mint EventIds across runtimes — content identity is the
        // honest join). Fetch failure degrades to the shallow window, loudly.
        // Turns-that-fit: the budget sizes the candidate window; the packer's
        // token walk decides what survives. self.grounding stays only as the
        // recipe floor (#259 — kills the 5-message world-view starvation).
        let grounding = Self::grounding_for_budget(budget, self.grounding);
        let mut events = events;
        let live_in_room = events
            .iter()
            .filter(|e| e.room_id.as_uuid() == room_id)
            .count();
        if live_in_room < grounding {
            if let Some(history) = &self.history {
                match history.room_tail(room_id, grounding * 2).await {
                    Ok(lines) => {
                        // Text via the ONE room-turn decoder (both wire shapes),
                        // same as ChannelElement — content identity is the dedup
                        // key because event ids re-mint across runtime restarts.
                        let live_bodies: std::collections::HashSet<String> = events
                            .iter()
                            .filter(|e| e.room_id.as_uuid() == room_id)
                            .filter_map(|e| {
                                crate::airc::realtime_wire::room_turn_from_event(e)
                                    .ok()
                                    .map(|(_, text)| text)
                            })
                            .collect();
                        let mut hydrated = 0usize;
                        // Chronological input order; stable sort keeps it among
                        // the lamport-0 cohort. Prepend via extend + later sort.
                        for line in &lines {
                            if live_bodies.contains(&line.text) {
                                continue;
                            }
                            let Ok(sender) = uuid::Uuid::parse_str(&line.sender_id) else {
                                continue;
                            };
                            events.push(Self::hydrated_event(room_id, sender, &line.text));
                            hydrated += 1;
                        }
                        crate::probe!(
                            class = "airc_rag.hydrated",
                            persona = self.persona_id.to_string().as_str(),
                            live = live_in_room,
                            hydrated = hydrated
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            persona_id = %self.persona_id,
                            live = live_in_room,
                            "airc rag: durable top-up unavailable — serving the shallow live window (#249)"
                        );
                    }
                }
            }
        }

        // Pre-staged by the region (if it staged this room), else built once now from
        // the events we already paged — identical shape (lazy compute-once). NOTE:
        // the pre-staged path is skipped when we hydrated (the staged digest was
        // built from the same shallow log); the built-once path below carries the
        // topped-up window. Region-side hydration is the follow-up slice.
        let digest = match self.buffer.peek(&(self.persona_id, room_id)) {
            // The region pre-stages with the recipe floor; only reuse it when
            // it already covers the budget-derived window — else rebuild wide.
            Some(d) if live_in_room >= grounding && d.elements.len() >= grounding => d,
            _ => Arc::new(self.builder.build_from_events(
                self.persona_id,
                room_id,
                events,
                grounding,
            )),
        };

        let (items, tokens_used) = Self::pack_digest(&digest, budget);
        tracing::debug!(
            persona_id = %self.persona_id,
            room = %room_id,
            window = digest.elements.len(),
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

    /// Reader that records the room scope it was paged with (#367).
    struct RoomRecordingReader {
        events: Vec<TranscriptEvent>,
        paged_room: Mutex<Option<Option<RoomId>>>,
    }
    #[async_trait]
    impl AircTranscriptReader for RoomRecordingReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(self.events.iter().take(limit).cloned().collect())
        }
        async fn page_recent_in(
            &self,
            room: Option<RoomId>,
            limit: usize,
        ) -> Result<Vec<TranscriptEvent>, AircError> {
            *self.paged_room.lock().unwrap() = Some(room);
            self.page_recent(limit).await
        }
    }

    // what this catches: #367 — the perception page must be scoped to the TURN's
    // room, not the reader's current-room pointer. BigMama's find: a turn in any
    // room other than the persona's landing room paged the DEFAULT room's
    // transcript, so the digest AND the recall query built from it followed the
    // wrong conversation. deliver() must hand ctx.airc_room to the reader
    // (Some → that room; None → pointer-scoped page, the room-less fallback).
    #[tokio::test]
    async fn deliver_pages_the_turn_room_not_the_pointer() {
        let room = RoomId::new();
        let reader = Arc::new(RoomRecordingReader {
            events: vec![event_in(room, Some("hello"), 1)],
            paged_room: Mutex::new(None),
        });
        let (source, _, _) = isolated_source(reader.clone());
        source
            .deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(
            *reader.paged_room.lock().unwrap(),
            Some(Some(room)),
            "turn room must reach the reader's page scope"
        );

        // Room-less work (consolidation, dreams): the page is explicitly
        // pointer-scoped, not accidentally room-pinned.
        let ctx_no_room = RagContext::for_persona(persona(), 1_000_000);
        source
            .deliver(&ctx_no_room, 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(*reader.paged_room.lock().unwrap(), Some(None));
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
            // Isolated tests exercise the live window; hydration has its own test
            // (a stub DurableRoomHistory) and stays off here.
            history: None,
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
        let delivery = source
            .deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[0].content, "hello");
        assert_eq!(delivery.items[1].content, "world");
        assert_eq!(
            delivery.items[1]
                .metadata
                .get("unread")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    // what this catches: the 5-message world view (#259, glass-boxed
    // 2026-07-30: Asha's captures showed "[context] you can currently see the
    // last 3 messages" while her L1 budget could hold dozens — she then looped
    // generic-assistant filler because the real conversation was invisible).
    // Grounding must derive from the TOKEN budget (turns-that-fit), never a
    // fixed message count: with every message already read (bookmark at tip),
    // a generous budget must still deliver far more than DEFAULT_GROUNDING.
    #[tokio::test]
    async fn caught_up_persona_sees_budget_worth_of_history_not_five_messages() {
        let room = RoomId::new();
        let events: Vec<TranscriptEvent> = (1..=20)
            .map(|l| event_in(room, Some(&format!("turn number {l}")), l))
            .collect();
        let reader = Arc::new(StubReader::new(events));
        let (source, bookmarks, _) = isolated_source(reader);
        bookmarks.advance(persona(), room.as_uuid(), 20); // fully caught up

        let delivery = source
            .deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw)
            .await;
        assert!(
            delivery.items.len() > DEFAULT_GROUNDING,
            "a 4k-token budget must widen the window past the {DEFAULT_GROUNDING}-message \
             recipe floor, got {}",
            delivery.items.len()
        );
        assert_eq!(
            delivery.items.len(),
            20,
            "all 20 short turns fit the budget — the packer, not a count, decides"
        );
    }

    // what this catches: BREADTH OVER VERBATIM DEPTH (#128/#146, measured
    // 2026-07-13: live persona prompts held THREE messages total in a busy
    // room — the whole perceivable world — so any low-frequency speaker was
    // displaced in seconds and the operator went unheard for hours). Long
    // turns must render as head-trimmed summaries so a small budget holds
    // MANY turns: the oldest (operator) message survives trimmed, the newest
    // stays verbatim. Under the old whole-message packer this exact input
    // delivered 3 items and the operator message was gone.
    #[tokio::test]
    async fn small_budget_keeps_many_trimmed_turns_not_three_essays() {
        let room = RoomId::new();
        let long = |tag: &str| format!("{tag}: {}", "lorem ipsum dolor sit amet ".repeat(15));
        let mut events = vec![event_in(
            room,
            Some(&long("OPERATOR your card is 0b1a6230")),
            1,
        )];
        for (i, l) in (2..=5).enumerate() {
            events.push(event_in(room, Some(&long(&format!("peer essay {i}"))), l));
        }
        events.push(event_in(room, Some(&long("newest peer question")), 6));
        let reader = Arc::new(StubReader::new(events));
        let (source, _, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room), 400, ResolutionPreference::Raw)
            .await;

        assert!(
            delivery.items.len() >= 6,
            "breadth: all 6 turns fit a 400-token budget as trimmed heads, got {}",
            delivery.items.len()
        );
        assert!(
            delivery.items[0].content.starts_with("OPERATOR")
                && delivery.items[0].content.contains("trimmed"),
            "oldest low-frequency speaker survives, trimmed: {:?}",
            delivery.items[0].content
        );
        let newest = &delivery.items.last().unwrap().content;
        assert!(
            newest.starts_with("newest peer question") && !newest.contains("trimmed"),
            "the turn being responded to stays verbatim: {newest:?}"
        );
        assert!(
            delivery.tokens_used <= 400,
            "budget honored: {}",
            delivery.tokens_used
        );
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
        assert_eq!(
            delivery.items.len(),
            1,
            "derives the room from the transcript, not deaf"
        );
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
    // NOTE (#259): reuse now requires the staged digest to COVER the budget-derived
    // window — the tiny budget here keeps that window at 1 so the staged snapshot
    // qualifies; an under-grounded stage must be rebuilt wide instead (previous test).
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

        let delivery = source
            .deliver(&ctx_in(room), 8, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(
            delivery.items[0].content, "staged",
            "served the pre-staged digest, not a rebuild"
        );
    }

    // what this catches: cross-persona ctx is refused (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_refused() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("secret"), 1)]));
        let (source, _, _) = isolated_source(reader);
        let mut other = RagContext::for_persona(Uuid::new_v4(), 1_000_000);
        other.substrate.airc_room = Some(room);
        let delivery = source
            .deliver(&other, 1_000, ResolutionPreference::Raw)
            .await;
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
        let delivery = source
            .deliver(&ctx_in(room), 1_000, ResolutionPreference::Raw)
            .await;
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
        let delivery = source
            .deliver(&ctx_in(room), 4, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2, "two newest fit budget 4");
        assert!(
            delivery.continuation.is_none(),
            "digest model has no continuation cursor"
        );
    }

    struct StubHistory {
        lines: Vec<crate::persona::durable_history::HydratedLine>,
    }
    #[async_trait]
    impl crate::persona::durable_history::DurableRoomHistory for StubHistory {
        async fn room_tail(
            &self,
            _room: Uuid,
            _limit: usize,
        ) -> Result<Vec<crate::persona::durable_history::HydratedLine>, String> {
            Ok(self.lines.clone())
        }
    }

    // what this catches: the #249 greeting-chorus mechanism. Post-reboot the
    // persona's airc runtime log holds ~1 live event while the room's real
    // history lives in the durable store — every mind saw ONE message and
    // mirrored it (glass-boxed live 2026-07-30). A shallow live window must be
    // topped up from the durable tail as GROUNDING (lamport 0 — never unread,
    // the #242 no-replay contract), deduped by content against live events.
    #[tokio::test]
    async fn shallow_live_window_tops_up_from_durable_history_as_grounding() {
        let room = RoomId::new();
        let sender = Uuid::new_v4();
        // ONE live event — the post-reboot shape.
        let live = event_in(room, Some("Hello everyone! I'm Benchy."), 5);
        let reader = Arc::new(StubReader::new(vec![live]));
        let (source, _bookmarks, _buffer) = isolated_source(reader);
        let mut source = source;
        source.grounding = 4; // want 4 lines of context; live has 1
        source.history = Some(Arc::new(StubHistory {
            lines: vec![
                crate::persona::durable_history::HydratedLine {
                    message_id: "m1".into(),
                    sender_id: sender.to_string(),
                    text: "the wordstats tests are next".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m2".into(),
                    sender_id: sender.to_string(),
                    // Duplicate of the live event — must be deduped, not doubled.
                    text: "Hello everyone! I'm Benchy.".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m3".into(),
                    sender_id: sender.to_string(),
                    text: "Atlas claimed card 7cedd4cf".into(),
                },
            ],
        }));

        let delivery = source
            .deliver(&ctx_in(room), 400, ResolutionPreference::Raw)
            .await;
        let texts: Vec<&str> = delivery.items.iter().map(|i| i.content.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains("wordstats tests")),
            "durable history must appear in the window; got: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("card 7cedd4cf")),
            "all non-duplicate durable lines hydrate; got: {texts:?}"
        );
        assert_eq!(
            texts.iter().filter(|t| t.contains("I'm Benchy")).count(),
            1,
            "the live event and its durable copy dedup to ONE line"
        );
        // Chronology: hydrated grounding (lamport 0) precedes the live event.
        let benchy_pos = texts.iter().position(|t| t.contains("I'm Benchy")).unwrap();
        let hist_pos = texts
            .iter()
            .position(|t| t.contains("wordstats tests"))
            .unwrap();
        assert!(
            hist_pos < benchy_pos,
            "hydrated history is PRIOR context, before the live tail"
        );
    }
}
