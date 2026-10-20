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

use crate::cognition::channel_digest::{ChannelDigestBuilder, DEFAULT_GROUNDING, FIRST_READ_PAGE};
use crate::cognition::channel_digest_region::DigestBuffer;
use crate::cognition::channel_element::ChannelElement;
use crate::cognition::channel_substrate::{
    global_channel_digest_buffer, global_channel_digest_builder,
};
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};
use crate::runtime::ready_buffer::ReadyBuffer;

pub(crate) use super::airc_window::HistoryAnchor;

/// Source identifier — used by budget presets, telemetry, cursor scope checks.
pub(crate) const SOURCE_ID: &str = "airc";

/// The verb that reads this room's transcript whole. (`collaboration/chat/export`,
/// named here before, is a stub that answers "not yet migrated" — a handle that fails
/// is worse than none.) Takes `roomId`; each message's id is on its trimmed marker.
pub(super) const EXPAND_COMMAND: &str = "chat/history";

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::{
    estimate_prompt_tokens as estimate_tokens,
};

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

    /// This reader's last-read lamport in `room` — the unread marker.
    ///
    /// THE CURSOR LIVES IN AIRC. It is durable runtime-consumer state
    /// (`runtime_cursor`, an ORM row keyed by consumer id), and airc's own API
    /// doc says why it exists: "intentionally store-backed so runtime delivery
    /// state does not sprawl into JSON sidecars." Continuum previously kept a
    /// PARALLEL `ChannelBookmarks` DashMap for this — process-memory only, so
    /// every cursor died on restart, and a second source of truth for a fact
    /// airc already owns. That is the defect this method removes.
    ///
    /// `0` = never read. Real airc lamports are >= 1.
    async fn read_cursor(&self, persona: uuid::Uuid, room: uuid::Uuid) -> Result<u64, AircError> {
        let _ = (persona, room);
        Ok(0)
    }

    /// Persist this reader's position in `room` at `event` — mark-read.
    ///
    /// Takes the EVENT, not a bare lamport, because airc's preferred path
    /// (`save_runtime_cursor_for_event`) carries the source event's room and
    /// kind and emits `SubscriptionAdvanced`, so a cursor move is visible to
    /// every other surface instead of being private to one process. A bare
    /// lamport would throw that away.
    async fn advance_read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
        event: &TranscriptEvent,
    ) -> Result<(), AircError> {
        let _ = (persona, room, event);
        Ok(())
    }
}

/// The durable consumer id for one reader's position in one room.
///
/// Namespaced string because that IS airc's convention for `runtime_cursor`
/// (its own rows look like `codex-hook:default`); the entity is keyed by a
/// consumer id, so this composes airc's key, it does not invent an identifier.
/// Both halves are rendered from real UUIDs — never a name, never a label.
pub fn read_cursor_consumer_id(persona: uuid::Uuid, room: uuid::Uuid) -> String {
    format!("persona:{persona}:room:{room}")
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

    /// airc's durable `runtime_cursor` row IS the unread marker — no second store.
    async fn read_cursor(&self, persona: uuid::Uuid, room: uuid::Uuid) -> Result<u64, AircError> {
        Ok(
            airc_lib::Airc::load_runtime_cursor(self, &read_cursor_consumer_id(persona, room))
                .await?
                .map(|c| c.lamport)
                .unwrap_or(0),
        )
    }

    async fn advance_read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
        event: &TranscriptEvent,
    ) -> Result<(), AircError> {
        airc_lib::Airc::save_runtime_cursor_for_event(
            self,
            &read_cursor_consumer_id(persona, room),
            event,
        )
        .await
    }
}

/// Persona-bound source delivering the consolidated channel digest.
pub struct AircRagSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircTranscriptReader>,
    builder: Arc<ChannelDigestBuilder>,
    buffer: Arc<DigestBuffer>,
    grounding: usize,
    /// #249: durable-transcript top-up for a shallow live window (post-reboot the
    /// persona's airc runtime log holds only events since ITS boot). `Some` in
    /// production (default); tests without a store see a loud-skip, not a panic.
    history: Option<Arc<dyn crate::persona::durable_history::DurableRoomHistory>>,
    /// WHERE HER HISTORY STARTS, per room: the event the window's oldest kept line
    /// belongs to. Kept across turns so the window's start holds still and jumps in
    /// large steps ([`HISTORY_SLACK`]) instead of sliding one event per turn. Measured
    /// on the 5090, 2026-10-05 (Cormac's trace): the newest-first fill dropped the
    /// oldest line on every room event, so the FIRST history message changed every
    /// turn; her hybrid base can only roll back to a saved checkpoint before the first
    /// differing token, so each turn re-prefilled 39-59k tokens (20-75 s) and the
    /// checkpoint cap then erased even the head's. One anchor per room, in memory.
    anchors: std::sync::Mutex<std::collections::HashMap<uuid::Uuid, HistoryAnchor>>,
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
            history: Some(Arc::new(crate::persona::durable_history::ChatStoreHistory)),
            anchors: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Page the room newest-first until the window is FULL, measuring what it reads.
    ///
    /// The packer decides what shows, so the fetch must cost what is shown plus at most a
    /// page, never `budget / some floor` rows (a 30k budget fetched ~3,750 rows to show
    /// ~100). The first page is the digest's own page (`FIRST_READ_PAGE`, at least the
    /// recipe floor). Each page's measured token cost gives the density; the next request
    /// sizes the window to `budget / density` plus one page. It stops when the rows read
    /// cost the whole budget, or when the room returns fewer rows than asked (no more).
    /// Returns the rows and the depth the budget wants at the measured density (what a
    /// durable top-up should reach when the live log is shallower).
    async fn page_to_budget(
        &self,
        room: Option<airc_core::RoomId>,
        budget: u32,
    ) -> Result<(Vec<TranscriptEvent>, usize), AircError> {
        let page = FIRST_READ_PAGE;
        let mut limit = page.max(self.grounding);
        loop {
            let events = self.reader.page_recent_in(room, limit).await?;
            let tokens: u64 = events
                .iter()
                .filter_map(|e| crate::airc::realtime_wire::room_content_from_event(e).ok())
                .map(|turn| u64::from(estimate_tokens(&turn.text)))
                .sum();
            // measured density: tokens per row actually read (a row with no text costs 0,
            // and an all-empty page has no density to extrapolate)
            let wanted = if tokens == 0 || events.is_empty() {
                events.len()
            } else {
                let rows = u64::from(budget) * events.len() as u64 / tokens;
                (rows as usize).saturating_add(page)
            };
            let exhausted = events.len() < limit;
            let full = tokens >= u64::from(budget);
            if exhausted || full || wanted <= limit {
                return Ok((events, wanted.max(self.grounding)));
            }
            limit = wanted.max(limit.saturating_add(page));
        }
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
    fn hydrated_event(
        room_id: uuid::Uuid,
        sender: uuid::Uuid,
        line: &super::durable_history::HydratedLine,
    ) -> TranscriptEvent {
        super::durable_history::event_from_row(room_id, super::durable_history::RoomRow {
            id: uuid::Uuid::parse_str(&line.message_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            sender, occurred_at_ms: line.occurred_at_ms, text: line.text.clone(), media: line.media.clone(),
        })
    }

    pub(super) fn format_item(
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
        Some(EXPAND_COMMAND)
    }

    fn parts_order(&self) -> crate::cognition::workspace::PartsOrder {
        crate::cognition::workspace::PartsOrder::Chronological
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
        // Depth follows the budget, measured: the room is paged until the rows read cost the
        // budget (`page_to_budget`), so the fetch is what is shown plus at most a page.
        let (events, grounding) = match self.page_to_budget(ctx.airc_room, budget).await {
            Ok(page) => page,
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
        let mut events = events;
        let live_in_room = events
            .iter()
            .filter(|e| e.room_id.as_uuid() == room_id)
            .count();
        if live_in_room < grounding {
            if let Some(history) = &self.history {
                match history.room_tail(room_id, grounding * 2).await {
                    Ok(lines) => {
                        // Sender, text and media via the ONE room-turn decoder (both wire shapes),
                        // same as ChannelElement — content identity is the dedup
                        // key because event ids re-mint across runtime restarts.
                        let live_bodies: std::collections::HashSet<_> = events
                            .iter()
                            .filter(|e| e.room_id.as_uuid() == room_id)
                            .filter_map(|e| {
                                crate::airc::realtime_wire::room_content_from_event(e)
                                    .ok()
                                    .map(|turn| (turn.sender, turn.text, turn.media))
                            })
                            .collect();
                        let mut hydrated = 0usize;
                        // Chronological input order; stable sort keeps it among
                        // the lamport-0 cohort. Prepend via extend + later sort.
                        for line in &lines {
                            let Ok(sender) = uuid::Uuid::parse_str(&line.sender_id) else {
                                continue;
                            };
                            if live_bodies.contains(&(sender, line.text.clone(), line.media.clone())) {
                                continue;
                            }
                            events.push(Self::hydrated_event(room_id, sender, line));
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
            Some(d) if !(live_in_room < grounding && self.history.is_some()) && d.elements.len() >= live_in_room => d,
            _ => {
                // The cursor is AIRC's, read per build. A read failure must not mute
                // her — fall back to 0 (everything unread), which the FIRST_READ_PAGE
                // bound then caps at one page rather than the whole transcript.
                let bookmark = match self.reader.read_cursor(self.persona_id, room_id).await {
                    Ok(b) => b,
                    Err(err) => {
                        tracing::warn!(
                            error = %err,
                            persona_id = %self.persona_id,
                            room = %room_id,
                            "airc rag: read cursor unavailable — treating the room as unread \
                             (bounded to one page), perception stays up"
                        );
                        0
                    }
                };
                Arc::new(self.builder.build_from_events(
                    self.persona_id,
                    room_id,
                    events,
                    grounding,
                    bookmark,
                ))
            }
        };

        let anchor = self.anchors.lock().unwrap_or_else(|p| p.into_inner()).get(&room_id).copied(); // a poisoned map still answers, same policy as every lock in this crate
        let (items, tokens_used, read_through, new_anchor) =
            Self::pack_digest(&digest, budget, crate::cognition::persona_workspace::acting_root_of(self.persona_id).is_some(), anchor);
        if let Some(a) = new_anchor {
            self.anchors.lock().unwrap_or_else(|p| p.into_inner()).insert(room_id, a); // a poisoned map still takes the anchor; losing it costs one re-prefill, never a panic
        }
        // SHE HAS NOW READ THE ROOM — advance her per-room cursor, exactly as the
        // human's UI does on nav/mark-read and on navigating away from a room.
        //
        // THE BUG THIS FIXES: `ChannelBookmarks` is per-(persona, room) and has
        // been correct since it was written — but the ONLY production callers of
        // `advance` were in `modules/nav.rs`, both keyed on `ctx.user_id`, the
        // authenticated HUMAN. A persona never navigates, so her `last_read` stayed
        // at 0 forever, and `last_read`'s own doc spells out what 0 means:
        // "never read (everything is unread)". Every turn therefore re-delivered
        // the ENTIRE paged history as UNREAD, each item at attention score 1.0,
        // with an EMPTY grounding split (unread_start == 0) — the digest's
        // read/unread structure was built and then never used for a persona.
        //
        // That is the storm: with nothing ever marked read, no message is ever
        // consumed, so two citizens in one room re-excite each other on a window
        // that only grows. With the cursor advancing, a wake shows what is NEW
        // plus N-before grounding, and a room with nothing new is quiet — the same
        // contract every message client has had for thirty years.
        //
        // Mark-read on DELIVERY, not on reply, because delivery is when she saw
        // it — `tip_lamport`'s own doc says "ignore/skip/respond all mark-read".
        // Advancing to `read_through` (what was PACKED) rather than the digest tip
        // keeps it honest under a tight budget: she is never marked read for a
        // message the packer dropped. `advance` is monotonic, so a repeat delivery
        // or a late lower lamport can never rewind her.
        if let Some(element) = read_through.as_ref() {
            if let Err(err) = self
                .reader
                .advance_read_cursor(self.persona_id, room_id, element.event())
                .await
            {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    room = %room_id,
                    "airc rag: could not persist the read cursor — she will re-read this \
                     page next turn (bounded), never the whole transcript"
                );
            }
        }
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
    use crate::cognition::channel_digest::ChannelDigest;
    use crate::persona::airc_window::HISTORY_SLACK;
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
        /// store calls and rows returned, so a test can price the fetch
        calls: Mutex<usize>,
        rows_read: Mutex<usize>,
    }
    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events,
                fail: Mutex::new(false),
                calls: Mutex::new(0),
                rows_read: Mutex::new(0),
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
            let page: Vec<TranscriptEvent> = self.events.iter().take(limit).cloned().collect();
            *self.calls.lock().unwrap() += 1;
            *self.rows_read.lock().unwrap() += page.len();
            Ok(page)
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
        let (source, _) = isolated_source(reader.clone());
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

    /// Source over an ISOLATED digest substrate (own cache/buffer) so tests don't
    /// touch process globals. The cursor is the READER's — airc owns it in
    /// production, and a stub reader carries it here.
    fn isolated_source(
        reader: Arc<dyn AircTranscriptReader>,
    ) -> (AircRagSource, Arc<DigestBuffer>) {
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let builder = Arc::new(ChannelDigestBuilder::new(cache));
        let buffer = Arc::new(DashMapReadyBuffer::new());
        let source = AircRagSource {
            persona_id: persona(),
            reader,
            builder,
            buffer: buffer.clone(),
            grounding: 0,
            // Isolated tests exercise the live window; hydration has its own test
            // (a stub DurableRoomHistory) and stays off here.
            history: None,
            anchors: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        (source, buffer)
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
        let (source, _) = isolated_source(reader);
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
        // Fully caught up — the STUB READER carries the cursor (airc's job live).
        let (source, _) = isolated_source(reader);

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
        let (source, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room), 400, ResolutionPreference::Raw)
            .await;

        assert!(
            delivery.items.len() >= 6,
            "breadth: all 6 turns fit a 400-token budget as trimmed heads, got {}",
            delivery.items.len()
        );
        assert!(
            delivery.items[0].content.starts_with("(…")
                && delivery.items[0].content.contains("trimmed")
                && delivery.items[0].content.trim_end().ends_with("sit amet"),
            "oldest low-frequency speaker survives, its end kept behind a marker: {:?}",
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

    fn receipt_from(room: RoomId, peer: PeerId, thought: &str, act: &str, lamport: u64) -> TranscriptEvent {
        let mut ev = event_in(room, Some(&format!("💭 {thought}\n⚙ {act}")), lamport);
        ev.peer_id = peer;
        ev
    }

    // what this catches (Cormac on #4806): a folded receipt line never relocates and a
    // run behind a chat line never changes again. Receipts fold per author per RUN,
    // a run ends at the next line anyone says, and the line sits at the run's start;
    // so another receipt from the same author after a chat line starts a NEW line at
    // the end and leaves every earlier byte of the window as it was, while the open
    // run at the tail (nothing said since) is the only thing that grows.
    #[test]
    fn a_receipt_run_ends_at_speech_and_an_earlier_run_never_changes() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let mut events = vec![event_in(room, Some("Kira: morning"), 1)];
        events.push(receipt_from(room, atlas, "thought one", "code/read a ✓", 2));
        events.push(receipt_from(room, atlas, "thought two", "code/read b ✓", 3));
        events.push(event_in(room, Some("Kira: how is it going"), 4));
        events.push(receipt_from(room, atlas, "thought three", "code/shell ls ✓", 5));
        let (source, _) = isolated_source(Arc::new(StubReader::new(Vec::new())));
        let render = |events: &Vec<TranscriptEvent>| {
            let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
            let (items, _, read_through, _) = AircRagSource::pack_digest(&digest, 4_000, false, None);
            (items.into_iter().map(|i| i.content).collect::<Vec<_>>(), read_through.map(|e| e.event().lamport))
        };
        let (before, read_before) = render(&events);
        assert_eq!(before.len(), 4, "{before:?}");
        assert!(before[1].starts_with("💭 thought two") && before[1].contains("2 act batches"), "the first run, folded, at its start: {:?}", before[1]);
        assert!(before[3].contains("thought three"), "the run after Kira's line is its own line: {:?}", before[3]);
        assert_eq!(read_before, Some(5), "read through the newest receipt");
        // Another receipt from Atlas: the open run at the tail grows; nothing before
        // it changes by a byte.
        events.push(receipt_from(room, atlas, "thought four", "code/shell cat ✓", 6));
        let (after, read_after) = render(&events);
        assert_eq!(after.len(), 4);
        assert_eq!(&after[..3], &before[..3], "every line before the open run is byte-identical");
        assert!(after[3].starts_with("💭 thought four") && after[3].contains("2 act batches"), "{:?}", after[3]);
        assert_eq!(read_after, Some(6));
    }

    // what this catches (Cormac on #4806, point 2): in a receipts-only room the window
    // starts on an OPEN run; keyed on the run's newest receipt the anchor looked
    // scrolled out on every receipt and jumped each turn. Keyed on the run's first
    // receipt it holds while the run grows.
    #[test]
    fn an_open_run_at_the_window_start_holds_the_anchor_while_it_grows() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let mut events: Vec<TranscriptEvent> = (1..=3).map(|l| receipt_from(room, atlas, &format!("thought {l}"), "code/read a ✓", l)).collect();
        let (source, _) = isolated_source(Arc::new(StubReader::new(Vec::new())));
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        let (_, _, _, a0) = AircRagSource::pack_digest(&digest, 4_000, false, None);
        let a0 = a0.expect("an anchor");
        events.push(receipt_from(room, atlas, "thought 4", "code/read b ✓", 4));
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        let (items, _, _, a1) = AircRagSource::pack_digest(&digest, 4_000, false, Some(a0));
        assert_eq!(a1, Some(a0), "the open run grew; its first receipt is still the start");
        assert_eq!(items.len(), 1);
        assert!(items[0].content.starts_with("💭 thought 4") && items[0].content.contains("4 act batches"), "{:?}", items[0].content);
    }

    // what this catches (Cormac's 5090 trace, 2026-10-05: 20-75 s of re-prefill per
    // turn): the window's START holds still across turns while the suffix from it
    // fits, so the first history message is byte-identical turn to turn and the
    // engine's prefix survives; when it no longer fits, the start jumps FORWARD with
    // HISTORY_SLACK of headroom, once, then holds again. Newest-first filling moved
    // it by one line on every room event.
    #[test]
    fn the_history_start_holds_still_and_jumps_in_blocks() {
        let room = RoomId::new();
        let (source, _) = isolated_source(Arc::new(StubReader::new(Vec::new())));
        let line = |l: u64| event_in(room, Some(&format!("peer{l}: line number {l} of the room, said with enough words that each line costs a real share of the window")), l);
        let mut events: Vec<TranscriptEvent> = (1..=120).map(line).collect();
        let budget = 300; // ~ sixty of these lines: half the room
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        // The first fill in a room shows everything the budget holds.
        let (_, tokens0, _, a0) = AircRagSource::pack_digest(&digest, budget, false, None);
        let a0 = a0.expect("an anchor");
        assert!(tokens0 <= budget && tokens0 > ((budget as f32) * (1.0 - HISTORY_SLACK)) as u32, "full: {tokens0} of {budget}");
        // One more event: the suffix from that start no longer fits, so the start
        // jumps ONCE, forward, with headroom.
        events.push(line(121));
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        let (items1, tokens1, _, a1) = AircRagSource::pack_digest(&digest, budget, false, Some(a0));
        let a1 = a1.expect("an anchor");
        assert_ne!(a1, a0, "the start jumped");
        assert!(tokens1 <= ((budget as f32) * (1.0 - HISTORY_SLACK)) as u32, "the jump left headroom: {tokens1}");
        let first1 = items1.first().unwrap().content.clone();
        // Several more: the start HOLDS and the first message stays byte-identical;
        // the new lines extend the window instead of replacing its head.
        events.push(line(122));
        events.push(line(123));
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        let (items2, tokens2, _, a2) = AircRagSource::pack_digest(&digest, budget, false, Some(a1));
        assert_eq!(a2, Some(a1), "the start held");
        assert_eq!(items2.first().unwrap().content, first1, "the first history message did not change");
        assert!(tokens2 <= budget && items2.len() == items1.len() + 2, "{} → {}", items1.len(), items2.len());
        // Enough traffic that it cannot hold: one more jump, headroom again, newest shown.
        for l in 124..=200 {
            events.push(line(l));
        }
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events.clone(), 0, 0);
        let (items3, tokens3, _, a3) = AircRagSource::pack_digest(&digest, budget, false, Some(a1));
        let a3 = a3.expect("an anchor");
        assert_ne!(a3, a1);
        assert!(tokens3 <= ((budget as f32) * (1.0 - HISTORY_SLACK)) as u32, "{tokens3}");
        assert!(items3.last().unwrap().content.contains("line number 200"), "the newest line is always shown");
    }

    // what this catches: THE LOOP WAS THE WINDOW (2026-09-03) — a run of one
    // author's work receipts collapses to ONE unit (her newest thought + an act
    // tally) instead of N verbatim "I've been going in circles" lines; a chat
    // line breaks the run and stays verbatim; the newest receipt anchors the unit.
    #[tokio::test]
    async fn a_run_of_work_receipts_collapses_to_the_latest_thought_plus_a_tally() {
        let room = RoomId::new();
        let atlas = PeerId::new();
        let lorcan = PeerId::new();
        let mut events = vec![event_in(room, Some("OPERATOR: card 678b8f5c is yours"), 1)];
        for l in 2..=5 {
            events.push(receipt_from(room, atlas, &format!("thought {l}"), "code/shell ls ✓", l));
            // twelve workers interleave: another author's receipt between each of Atlas's
            events.push(receipt_from(room, lorcan, &format!("lorcan {l}"), "code/read x ✓", l + 100));
        }
        events.push(receipt_from(room, atlas, "thought six", "code/github/issue-create  ✗", 200));
        events.push(event_in(room, Some("Kira: Atlas, stop filing issues"), 201));
        let reader = Arc::new(StubReader::new(events));
        let (source, _) = isolated_source(reader);
        let delivery = source
            .deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(
            delivery.items.len(),
            4,
            "operator line + ONE unit per working author (Lorcan, Atlas) + Kira: {:?}",
            delivery.items.iter().map(|i| i.content.clone()).collect::<Vec<_>>()
        );
        // Runs sit where they STARTED (Atlas's at lamport 2, Lorcan's at 102), so a
        // later receipt never relocates a line; each still reads its newest thought.
        let lorcan_unit = &delivery.items[2].content;
        assert!(lorcan_unit.starts_with("💭 lorcan 5"), "Lorcan's newest leads: {lorcan_unit:?}");
        assert!(lorcan_unit.contains("code/read ✓×4"), "{lorcan_unit:?}");
        let run = &delivery.items[1].content;
        assert!(run.starts_with("💭 thought six"), "newest thought leads: {run:?}");
        assert!(run.contains("5 act batches"), "batch count: {run:?}");
        assert!(run.contains("code/shell ✓×4"), "tally: {run:?}");
        assert!(run.contains("code/github/issue-create ✗"), "tally: {run:?}");
        assert!(!run.contains("thought 2"), "older thoughts folded away: {run:?}");
        assert!(delivery.items[3].content.starts_with("Kira:"));
    }

    // what this catches: THE CARRIER (card 169eb543, 2026-09-15) — a roommate's line
    // that the speak gate would refuse (a bare `[code/run]` envelope, a `[wake]` framing
    // echo, a peer-voice line) never enters her burst, in EITHER packing mode. Six
    // citizens adopted one envelope over 20 hours by reading it in the room; the
    // store still holds thousands of them and other nodes keep posting them, so the
    // speak gate alone cannot end the contagion. Real speech and receipts are untouched.
    #[tokio::test]
    async fn a_line_the_speak_gate_would_refuse_never_enters_her_burst() {
        let room = RoomId::new();
        let events = vec![
            event_in(room, Some("Kira: the fix is in models.py:44"), 1),
            event_in(room, Some("[code/run]\n[invalid] command_run: unsupported lang"), 2),
            event_in(room, Some("[wake] You are Paige, awake on the continuum grid."), 3),
            event_in(
                room,
                Some("b6dcfc8e-98ab-4488-b469-d1441720621b: I understand the confusion."),
                4,
            ),
            event_in(
                room,
                Some("```python\n# Mark this room's activity concluded (o); code/shell({\"cmd\":\"# Mark o\"})\n```"),
                5,
            ),
            event_in(
                room,
                Some("Atlas: to run it you'd write code/shell({\"cmd\":\"ls\"}) — note the braces."),
                6,
            ),
        ];
        let (source, _) = isolated_source(Arc::new(StubReader::new(events.clone())));
        // Both packing modes share the seam: the working path (presence plane dropped)
        // and the conversational path (receipts collapsed per author).
        let digest = source.builder.build_from_events(persona(), room.as_uuid(), events, 0, 0);
        for working in [false, true] {
            let (items, _, _, _) = AircRagSource::pack_digest(&digest, 4_000, working, None);
            let seen: Vec<&str> = items.iter().map(|i| i.content.as_str()).collect();
            assert_eq!(seen.len(), 2, "working={working}: only the two real lines survive: {seen:?}");
            assert!(seen[0].starts_with("Kira:"), "{seen:?}");
            assert!(seen[1].starts_with("Atlas:"), "a line DISCUSSING an envelope is speech: {seen:?}");
        }
        // And the live path (the one every turn takes) delivers the same two.
        let delivery = source.deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 2, "{:?}", delivery.items.iter().map(|i| &i.content).collect::<Vec<_>>());
    }

    // what this catches: THE DEAF-PERSONA FIX — when the turn's ctx has no airc_room
    // (compose_for_turn sets None), the room is DERIVED from the transcript (page_recent
    // is room-scoped) so the persona still hears the conversation, instead of going
    // deaf. Regression guard for the slice-2 over-strict airc_room requirement.
    #[tokio::test]
    async fn no_room_scope_derives_room_from_transcript() {
        let room = RoomId::new();
        let reader = Arc::new(StubReader::new(vec![event_in(room, Some("hi"), 1)]));
        let (source, _) = isolated_source(reader);
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
        let (source, _) = isolated_source(reader);
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
        let (source, buffer) = isolated_source(reader);
        // Build a staged digest via a separate builder over the SAME-shape elements.
        let cache = Arc::new(ChannelElementCache::new(Arc::new(NoopEmbedder)));
        let staged_builder = ChannelDigestBuilder::new(cache);
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
        let (source, _) = isolated_source(reader);
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
        let (source, _) = isolated_source(reader);
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
        let (mut source, _) = isolated_source(reader);
        // the production recipe floor, which the first page never goes under
        source.grounding = DEFAULT_GROUNDING;
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
        // ONE live event — the post-reboot shape.
        let live = event_in(room, Some("Hello everyone! I'm Benchy."), 5);
        // A durable copy has the SAME sender. Equal text from different peers
        // is a distinct message and must not be silently collapsed.
        let sender = live.peer_id.to_string();
        let reader = Arc::new(StubReader::new(vec![live]));
        let (source, _buffer) = isolated_source(reader);
        let mut source = source;
        source.grounding = 4; // want 4 lines of context; live has 1
        source.history = Some(Arc::new(StubHistory {
            lines: vec![
                crate::persona::durable_history::HydratedLine {
                    message_id: "m1".into(),
                    occurred_at_ms: 1791078791935,
                    sender_id: sender.to_string(),
                    media: Vec::new(),
                    text: "the wordstats tests are next".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m2".into(),
                    occurred_at_ms: 1791078792935,
                    sender_id: sender.to_string(),
                    // Duplicate of the live event — must be deduped, not doubled.
                    media: Vec::new(),
                    text: "Hello everyone! I'm Benchy.".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m3".into(),
                    occurred_at_ms: 1791078793935,
                    sender_id: sender.to_string(),
                    media: Vec::new(),
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
        // regression (Kimi, 2026-10-04): hydration dropped each line's time and rebuilt it
        // as 0, so history rendered "[occurrence time unknown]". It keeps the store's time;
        // lamport 0 alone keeps it on the grounding side (asserted above).
        let line = crate::persona::durable_history::HydratedLine {
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: sender.to_string(),
            occurred_at_ms: 1791078791935,
            media: Vec::new(),
            text: "timed".into(),
        };
        let event = AircRagSource::hydrated_event(room.as_uuid(), uuid::Uuid::parse_str(&sender).expect("peer uuid"), &line);
        assert_eq!(event.occurred_at_ms, 1791078791935, "a hydrated line keeps its own time");
    }

    // what this catches: THE PER-MESSAGE CLIP. A message was cut to budget/8 clamped to
    // 48..256 tokens whatever the window held, so a 300-token message was trimmed even
    // when the window could hold ten of them. With a large window it must arrive WHOLE,
    // and the depth ceiling (256) must be gone: the fetch and the window follow the budget.
    #[tokio::test]
    async fn a_large_window_shows_a_long_message_whole_and_depth_follows_the_budget() {
        let room = RoomId::new();
        let long = format!("essay: {}", "lorem ipsum dolor sit amet ".repeat(45)); // ~300 tokens
        let reader = Arc::new(StubReader::new(vec![
            event_in(room, Some(&long), 1),
            event_in(room, Some("short reply"), 2),
            event_in(room, Some("newest"), 3),
        ]));
        let (source, _) = isolated_source(reader);
        let delivery = source.deliver(&ctx_in(room), 8_000, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items[0].content, long, "the whole message, no marker");
    }

    // what this catches: the fetch sized by an unmeasured floor (`budget / 8` rows: a 30k
    // budget read ~3,750 rows to show ~100) and the old 100/256 ceilings. The packer drives
    // the fetch: a big budget over short messages reads about what it shows plus a page, and
    // depth still follows the budget (more shown than the old 256 ceiling allowed).
    #[tokio::test]
    async fn the_fetch_costs_what_is_shown_plus_at_most_a_page() {
        let budget = 4_000;
        // (message body, whether the window should exceed the old 256-turn ceiling)
        for (body, past_old_ceiling) in [("lorem ipsum dolor sit amet ".repeat(9), false), ("ok".to_string(), true)] {
            let room = RoomId::new();
            let events: Vec<TranscriptEvent> = (1..=3_000u64)
                .map(|l| event_in(room, Some(&format!("{body} {l:05}")), l))
                .collect();
            let reader = Arc::new(StubReader::new(events));
            let (source, _) = isolated_source(reader.clone());
            let delivery = source.deliver(&ctx_in(room), budget, ResolutionPreference::Raw).await;
            let shown = delivery.items.len();
            let rows = *reader.rows_read.lock().unwrap();
            let calls = *reader.calls.lock().unwrap();
            if past_old_ceiling {
                assert!(shown > 256, "depth follows the budget past the old ceiling, shown {shown}");
            }
            assert!(
                rows <= shown + 3 * FIRST_READ_PAGE,
                "{} chars/msg: read {rows} rows in {calls} calls to show {shown}; budget/8 would read {}",
                body.len(),
                budget / 8
            );
            assert!(delivery.tokens_used <= budget);
        }
    }

    // what this catches: a trim with no handle. A message bigger than its fair share of a
    // small window collapses to its end, led by a marker that names the verb and the id that return
    // it whole; the small messages around it keep every token, and the newest is verbatim.
    #[tokio::test]
    async fn an_over_share_message_keeps_its_end_behind_a_marker_that_names_how_to_read_it() {
        let room = RoomId::new();
        let long = format!("OPERATOR: {}THE ANSWER IS 42", "lorem ipsum dolor sit amet ".repeat(80));
        let long_event = event_in(room, Some(&long), 1);
        let long_id = long_event.event_id.as_uuid().to_string();
        let reader = Arc::new(StubReader::new(vec![
            long_event,
            event_in(room, Some("peer: short one"), 2),
            event_in(room, Some("peer: short two"), 3),
            event_in(room, Some("newest question"), 4),
        ]));
        let (source, _) = isolated_source(reader);
        let delivery = source.deliver(&ctx_in(room), 200, ResolutionPreference::Raw).await;
        let first = &delivery.items[0].content;
        assert!(first.starts_with("(…") && first.ends_with("THE ANSWER IS 42"), "the end is kept whole: {first}");
        assert!(
            first.contains(EXPAND_COMMAND) && first.contains(&long_id) && first.contains("opening trimmed"),
            "the head names the verb and the id: {first}"
        );
        assert_eq!(delivery.items[1].content, "peer: short one", "a message within its share is not touched");
        assert_eq!(delivery.items.last().map(|i| i.content.as_str()), Some("newest question"));
        assert!(delivery.tokens_used <= 200, "{}", delivery.tokens_used);
    }
}
