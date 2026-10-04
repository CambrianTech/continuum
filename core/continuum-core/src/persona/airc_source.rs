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

use airc_core::{TranscriptCursor, TranscriptEvent};
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
pub(crate) const SOURCE_ID: &str = "airc";

/// Default newest-events fetch cap when building a digest on demand (mirrors the
/// region's). The recipe-defined grounding window slices within this.
pub(crate) const FETCH_LIMIT: usize = 100;

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::{estimate_prompt_tokens as estimate_tokens, head_to_tokens};

/// Abstract reader over airc transcript events. Production impl rides on
/// `airc_lib::Airc`; tests use a stub that returns canned events without a daemon.
#[async_trait]
pub trait AircTranscriptReader: Send + Sync {
    /// Return up to `limit` most-recent room speech and typed activity events,
    /// newest-first per airc convention, excluding known controls before limit.
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
    /// lifecycle ride their own sources; ordinary typed work activity shares
    /// this page with conversation, while lease renewal does not.
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

    /// Raw oldest-first room page after an exact cursor. Production readers
    /// must forward to the durable owner; a newest-page fallback loses unread.
    async fn page_after_in(
        &self,
        room: airc_core::RoomId,
        cursor: &TranscriptCursor,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        let _ = (room, cursor, limit);
        Err(AircError::Transport(
            "reader does not support durable room replay".into(),
        ))
    }

    /// This reader's last-read `(lamport, event_id)` in `room` — the unread marker.
    ///
    /// THE CURSOR LIVES IN AIRC. It is durable runtime-consumer state
    /// (`runtime_cursor`, an ORM row keyed by consumer id), and airc's own API
    /// doc says why it exists: "intentionally store-backed so runtime delivery
    /// state does not sprawl into JSON sidecars." Continuum previously kept a
    /// PARALLEL `ChannelBookmarks` DashMap for this — process-memory only, so
    /// every cursor died on restart, and a second source of truth for a fact
    /// airc already owns. That is the defect this method removes.
    ///
    /// `None` means never read; equal-lamport siblings retain distinct positions.
    async fn read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
    ) -> Result<Option<TranscriptCursor>, AircError> {
        let _ = (persona, room);
        Ok(None)
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

    /// Acknowledge only a successful request's actual selected room inputs.
    /// Rescan the same durable room prefix; never step across an unseen fact.
    async fn acknowledge_presented(
        &self,
        persona: uuid::Uuid,
        inputs: &[crate::cognition::provenance::RoomInput],
    ) -> Result<(), AircError> {
        let mut rooms = std::collections::BTreeMap::<uuid::Uuid, Vec<_>>::new();
        for input in inputs {
            rooms.entry(input.room_id).or_default().push(input);
        }
        for (room, inputs) in rooms {
            let stored = self.read_cursor(persona, room).await?;
            let anchor = inputs
                .iter()
                .map(|input| input.read_after.as_ref())
                .min_by_key(|cursor| cursor.map(cursor_key))
                .flatten();
            let start = stored
                .as_ref()
                .into_iter()
                .chain(anchor)
                .max_by_key(|cursor| cursor_key(cursor))
                .cloned()
                .unwrap_or(TranscriptCursor {
                    lamport: 0,
                    event_id: airc_core::EventId::from_uuid(uuid::Uuid::nil()),
                });
            let presented: std::collections::HashSet<_> = inputs
                .iter()
                .map(|input| (input.cursor.lamport, input.cursor.event_id))
                .collect();
            let events = self
                .page_after_in(airc_core::RoomId::from_uuid(room), &start, FETCH_LIMIT)
                .await?;
            let mut through = None;
            for event in &events {
                if event.room_id.as_uuid() != room
                    || cursor_key(&event.cursor()) <= cursor_key(&start)
                {
                    return Err(AircError::Transport(
                        "room replay returned an invalid cursor or room".into(),
                    ));
                }
                let skipped = match crate::airc::realtime_wire::room_content_from_event(event) {
                    Err(reason) if crate::airc::realtime_wire::is_non_perceptual_reason(reason) => {
                        true
                    }
                    Err(reason) => {
                        return Err(AircError::Transport(format!(
                            "cannot acknowledge room event {}: {reason}",
                            event.event_id
                        )))
                    }
                    Ok(_) => false,
                };
                if !skipped && !presented.contains(&(event.lamport, event.event_id)) {
                    break;
                }
                through = Some(event);
            }
            if let Some(event) = through {
                self.advance_read_cursor(persona, room, event).await?;
            }
        }
        Ok(())
    }
}

pub(crate) fn cursor_key(cursor: &TranscriptCursor) -> (u64, uuid::Uuid) {
    (cursor.lamport, cursor.event_id.as_uuid())
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

/// The kinds a perception page means: room speech and ordinary typed activity. ONE place
/// (compression) — every reader impl funnels through the [`airc_lib::Airc`]
/// impl below, which applies this filter.
pub fn perception_page_filter() -> airc_lib::EventFilter {
    let mut filter = airc_lib::EventFilter::current_room();
    filter.kinds.insert(airc_core::TranscriptKind::Message);
    filter.kinds.insert(airc_core::TranscriptKind::Attachment);
    filter.kinds.insert(airc_core::TranscriptKind::System);
    filter.headers_filter = crate::airc::realtime_wire::room_perception_header_filter();
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
    async fn read_cursor(
        &self,
        persona: uuid::Uuid,
        room: uuid::Uuid,
    ) -> Result<Option<TranscriptCursor>, AircError> {
        airc_lib::Airc::load_runtime_cursor(self, &read_cursor_consumer_id(persona, room)).await
    }

    async fn page_after_in(
        &self,
        room: airc_core::RoomId,
        cursor: &TranscriptCursor,
        limit: usize,
    ) -> Result<Vec<TranscriptEvent>, AircError> {
        let room = self
            .room_by_channel(room)
            .await?
            .ok_or_else(|| AircError::NotSubscribed(room.to_string()))?;
        self.resume_from_in(&room, cursor, limit).await
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
    fn hydrated_event(
        room_id: uuid::Uuid,
        sender: uuid::Uuid,
        line: &super::durable_history::HydratedLine,
    ) -> TranscriptEvent {
        super::durable_history::event_from_row(
            room_id,
            super::durable_history::RoomRow {
                id: uuid::Uuid::parse_str(&line.message_id)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                sender,
                occurred_at_ms: 0,
                text: line.text.clone(),
                media: line.media.clone(),
            },
        )
    }

    /// Override the newest-events fetch cap used when building a digest on demand.
    pub fn with_fetch_limit(mut self, fetch_limit: usize) -> Self {
        self.fetch_limit = fetch_limit;
        self
    }

    /// Format a digest into budget-packed `RagItem`s. Packs unread oldest-first,
    /// then already-read context, and emits chronological (oldest-first) so
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
    /// The newest turn may use the remaining budget after older unread input.
    /// Only complete items carry a RoomInput handle for later successful-request
    /// acknowledgement; trimmed heads and folded summaries do not consume facts.
    fn pack_digest(digest: &ChannelDigest, budget: u32, working: bool) -> (Vec<RagItem>, u32) {
        // Per-turn cap: budget/8 → a useful window holds ~8+ turns; clamped so
        // tiny budgets still render a sentence and huge ones don't let one
        // essay crowd the window.
        let per_turn_cap = (budget / 8).clamp(48, 256);
        let (units, quarantined) = Self::collapse_work_receipts(digest, working);
        if quarantined > 0 {
            crate::probe!(
                class = "perception.quarantined",
                lines = quarantined as u64,
                working = working,
                "transcript lines the speak gate would refuse were kept out of her burst — the contagion carrier (card 169eb543)"
            );
        }
        let mut keep: Vec<(usize, Option<String>)> = Vec::new();
        let mut tokens_used: u32 = 0;
        // Give unread input its original order before spending space on context.
        // Grounding is filled newest-first afterwards, then rendered chronologically.
        let ordered = units
            .iter()
            .filter(|unit| unit.last_idx >= digest.unread_start)
            .chain(
                units
                    .iter()
                    .rev()
                    .filter(|unit| unit.last_idx < digest.unread_start),
            );
        for unit in ordered {
            let idx = unit.last_idx;
            let text: &str = match unit.collapsed.as_deref() {
                Some(t) => t,
                None => match digest.elements[idx].text() {
                    Some(t) => t,
                    None => continue,
                },
            };
            let full = estimate_tokens(text);
            let remaining = budget.saturating_sub(tokens_used);
            let cap = if units.last().is_some_and(|newest| newest.last_idx == idx) {
                remaining
            } else {
                per_turn_cap.min(remaining)
            };
            let (cost, trimmed) = if full <= cap {
                (full, unit.collapsed.clone())
            } else {
                let reserve = crate::cognition::token_budget::trim_marker_tokens();
                if cap <= reserve {
                    break;
                }
                let head = head_to_tokens(text, cap - reserve);
                let trimmed = crate::cognition::token_budget::mark_head_kept(&head, full);
                (estimate_tokens(&trimmed), Some(trimmed))
            };
            if tokens_used.saturating_add(cost) > budget {
                break;
            }
            tokens_used += cost;
            keep.push((idx, trimmed));
        }
        keep.sort_by_key(|(idx, _)| *idx);
        let items = keep
            .into_iter()
            .map(|(idx, trimmed)| {
                // A head or folded summary cannot acknowledge its omitted body.
                let complete = trimmed.is_none();
                let mut item =
                    Self::format_item(&digest.elements[idx], idx >= digest.unread_start, trimmed);
                let event = digest.elements[idx].event();
                if complete && event.lamport > 0 {
                    item.metadata["room_input"] =
                        serde_json::to_value(crate::cognition::provenance::RoomInput {
                            room_id: event.room_id.as_uuid(),
                            cursor: event.cursor(),
                            read_after: digest.bookmark.clone(),
                        })
                        .expect("RoomInput contains only typed UUID/cursor values");
                }
                item
            })
            .collect();
        (items, tokens_used)
    }

    /// COLLAPSE, DON'T CLIP — work receipts. A working citizen radiates one
    /// `💭 thought` + `⚙ verb ✓/✗` receipt per act batch into the room (so
    /// roommates see live work). Grounded verbatim, a run room's window is 140
    /// receipts and no conversation: every citizen reads everyone's "I've been
    /// going in circles" and says it back (12 citizens, live 2026-09-03 — the
    /// loop was the WINDOW). Twelve workers interleave, so receipts are never
    /// consecutive per author (measured after the first cut: 168 items, still
    /// one per receipt). The unit is therefore PER AUTHOR ACROSS THE WINDOW:
    /// all of one author's receipts fold into a single line — her newest
    /// thought + a tally of what she ran — anchored at her newest receipt
    /// (read-through cursor, unread flag). One line per teammate; chat lines
    /// stay verbatim in place.
    ///
    /// QUARANTINE, BEFORE EITHER PATH: a line the speak gate would refuse (a tool
    /// envelope, a peer's voice, a framing echo — [`is_not_a_contribution`]) is not a
    /// message to her either, whoever posted it and whenever. It is the vector by
    /// which every malformed shape spread across the roster (card 169eb543: six
    /// citizens adopted one envelope over 20 hours, by reading it). Returns the
    /// units and how many lines were kept out, so the packer can say so.
    fn collapse_work_receipts(digest: &ChannelDigest, working: bool) -> (Vec<PackUnit>, usize) {
        // ONE pass decides both (Cormac's review of #4076): which elements contribute,
        // and how many were kept out — counted only among lines the branch would
        // otherwise have shown (in the working branch the presence plane is dropped
        // anyway, so a receipt-shaped envelope there is not "quarantined").
        let mut quarantined = 0usize;
        let contributes: Vec<bool> = digest
            .elements
            .iter()
            .map(|el| {
                let Some(text) = el.text() else { return true };
                if working && is_work_receipt(text) {
                    return true; // dropped below as presence, not as quarantine
                }
                let ok = !is_not_a_contribution(text);
                if !ok {
                    quarantined += 1;
                }
                ok
            })
            .collect();
        // WORKING (hands rooted at a card): the PRESENCE plane — every citizen's 💭
        // thought broadcast and ⚙ receipt, hers included (her newest thoughts lead
        // the turn from working memory) — is not packed at all: it is state, not a
        // message to her. The MESSAGE plane stays: human/agent lines and real speech.
        // Measured 2026-09-05: every work turn opened with "this room is noisy, let
        // me figure out what is real" (attention spent as deserialization, not sniffing).
        if working {
            let units = digest
                .elements
                .iter()
                .enumerate()
                .filter(|(idx, el)| contributes[*idx] && !el.text().is_some_and(is_work_receipt))
                .map(|(idx, _)| PackUnit {
                    last_idx: idx,
                    collapsed: None,
                })
                .collect();
            return (units, quarantined);
        }
        use std::collections::HashMap;
        // author → (newest receipt idx, every receipt idx in order)
        let mut by_author: HashMap<uuid::Uuid, (usize, Vec<usize>)> = HashMap::new();
        for (idx, el) in digest.elements.iter().enumerate() {
            if el.text().is_some_and(is_work_receipt) {
                let entry = by_author.entry(el.sender_id()).or_insert((idx, Vec::new()));
                entry.0 = idx;
                entry.1.push(idx);
            }
        }
        let mut units: Vec<PackUnit> = Vec::new();
        for (idx, el) in digest.elements.iter().enumerate() {
            if !contributes[idx] {
                continue; // refused at the speak seam → never shown at the perception seam
            }
            let is_receipt = el.text().is_some_and(is_work_receipt);
            if !is_receipt {
                units.push(PackUnit {
                    last_idx: idx,
                    collapsed: None,
                });
                continue;
            }
            let Some((newest, all)) = by_author.get(&el.sender_id()) else {
                continue;
            };
            if *newest != idx {
                continue; // an older receipt of hers — folded into her newest
            }
            let collapsed = if all.len() == 1 {
                None
            } else {
                Some(Self::collapsed_receipt_text(
                    all.iter().filter_map(|i| digest.elements[*i].text()),
                    all.len(),
                ))
            };
            units.push(PackUnit {
                last_idx: idx,
                collapsed,
            });
        }
        (units, quarantined)
    }

    /// The collapsed text of a receipt run: the newest `💭` line, then a tally
    /// of every `⚙ verb mark` across the run (`⚙ code/shell ✓×4 · code/github/issue-create ✗×2`).
    fn collapsed_receipt_text<'a>(texts: impl Iterator<Item = &'a str>, batches: usize) -> String {
        let mut last_thought: Option<&str> = None;
        let mut tally: Vec<(String, usize)> = Vec::new();
        for text in texts {
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with(crate::persona::presence_glyph::THOUGHT) {
                    last_thought = Some(line);
                } else if let Some(rest) = line.strip_prefix(crate::persona::presence_glyph::ACT) {
                    let mut parts = rest.split_whitespace();
                    let verb = parts.next().unwrap_or("?"); // unwrap_or: a bare marker still tallies as unknown
                    let mark = parts.last().unwrap_or("·"); // unwrap_or: a verb without a mark tallies as neutral
                    let key = format!("{verb} {mark}");
                    match tally.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, n)) => *n += 1,
                        None => tally.push((key, 1)),
                    }
                }
            }
        }
        let acts: Vec<String> = tally
            .iter()
            .map(|(k, n)| {
                if *n > 1 {
                    format!("{k}×{n}")
                } else {
                    k.clone()
                }
            })
            .collect();
        format!(
            "{} · ⚙ {batches} act batches: {}",
            last_thought.unwrap_or("💭 (working)"), // unwrap_or: a run of bare act lines has no thought to lead with
            if acts.is_empty() {
                "(no receipts)".to_string()
            } else {
                acts.join(" · ")
            }
        )
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
                "content_kind": element.content_kind(),
                "target": ev.target,
                "subject_card": element.content_kind().and_then(|kind| kind.subject_card()),
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

/// One packable unit of the window: a message, or a collapsed run of work receipts.
struct PackUnit {
    /// The element that anchors the unit (the run's newest; the read-through cursor).
    last_idx: usize,
    /// The collapsed text when the unit is a receipt run of two or more; `None`
    /// packs the element's own text.
    collapsed: Option<String>,
}

/// A radiated work receipt (`act_observe::apply`): leads with `💭` or `⚙`.
fn is_work_receipt(text: &str) -> bool {
    let t = text.trim_start();
    crate::persona::presence_glyph::is_presence_line(t)
}

/// The speak gate's verdict, read at the perception seam — one predicate, two seams.
fn is_not_a_contribution(text: &str) -> bool {
    crate::cognition::not_speech::is_not_a_contribution(text).is_some()
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
                            if live_bodies.contains(&(
                                sender,
                                line.text.clone(),
                                line.media.clone(),
                            )) {
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

        // The shared builder owns bounded unread paging and control-only scan
        // progress. Rendering alone never advances the durable read cursor.
        let previous = self.buffer.peek(&(self.persona_id, room_id));
        let durable = match self.reader.read_cursor(self.persona_id, room_id).await {
            Ok(cursor) => cursor,
            Err(err) => {
                tracing::warn!(error = %err, persona_id = %self.persona_id, room = %room_id,
                    "airc rag: unread position unavailable; no delivery or acknowledgement");
                return Self::empty(ResolutionPreference::Placeholder);
            }
        };
        let reusable = previous.as_ref().filter(|digest| {
            digest.durable_bookmark == durable
                && digest.has_unread()
                && live_in_room >= grounding
                && digest.elements.len() >= grounding
        });
        let digest = if let Some(digest) = reusable {
            Arc::clone(digest)
        } else {
            match self
                .builder
                .build_from_reader_window(
                    self.persona_id,
                    room_id,
                    self.reader.as_ref(),
                    self.fetch_limit,
                    grounding,
                    events,
                    previous.as_deref(),
                )
                .await
            {
                Ok(digest) => Arc::new(digest),
                Err(err) => {
                    tracing::warn!(error = %err, persona_id = %self.persona_id, room = %room_id,
                    "airc rag: unread position unavailable; no delivery or acknowledgement");
                    return Self::empty(ResolutionPreference::Placeholder);
                }
            }
        };
        self.buffer
            .publish((self.persona_id, room_id), Arc::clone(&digest));
        let (items, tokens_used) = Self::pack_digest(
            &digest,
            budget,
            crate::cognition::persona_workspace::acting_root_of(self.persona_id).is_some(),
        );
        // `room_input` handles travel through final prompt selection. Only a
        // successful GenerationReceipt may acknowledge their contiguous prefix.
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
            fetch_limit: FETCH_LIMIT,
            // Isolated tests exercise the live window; hydration has its own test
            // (a stub DurableRoomHistory) and stays off here.
            history: None,
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
        // Exercise the same reader against its actual durable owner. The wire
        // root is isolated, so this never attaches to the installed daemon.
        let home = tempfile::tempdir().unwrap();
        let reader = Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .unwrap(),
        );
        let room = reader.join("digest-cursor-proof").await.unwrap().channel;
        let mut first = event_in(room, Some("hello"), 1);
        first.event_id = EventId::from_uuid(uuid::Uuid::from_u128(1));
        let mut second = event_in(room, Some("world"), 1);
        second.event_id = EventId::from_uuid(uuid::Uuid::from_u128(2));
        reader.append_event(first.clone()).await.unwrap();
        reader.append_event(second.clone()).await.unwrap();
        let (source, _) = isolated_source(reader.clone());
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
        let inputs: Vec<crate::cognition::provenance::RoomInput> = delivery
            .items
            .iter()
            .map(|item| serde_json::from_value(item.metadata["room_input"].clone()).unwrap())
            .collect();
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            None,
            "composition is not successful presentation"
        );
        reader.acknowledge_presented(persona(), &[]).await.unwrap();
        reader
            .acknowledge_presented(persona(), &inputs[1..])
            .await
            .unwrap();
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            None,
            "missing/failed receipt and a selected later sibling cannot cross an unpresented fact"
        );
        reader
            .acknowledge_presented(persona(), &inputs[..1])
            .await
            .unwrap();
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            Some(first.cursor())
        );
        drop(source);
        drop(reader);

        let reader = Arc::new(
            airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .unwrap(),
        );
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            Some(first.cursor()),
            "the full tuple survives reopening the real ORM store"
        );
        let mut expected = vec![second.event_id.as_uuid()];
        for index in 3..=125 {
            let event = event_in(room, Some(&format!("unread {index}")), index);
            expected.push(event.event_id.as_uuid());
            reader.append_event(event).await.unwrap();
        }
        let (source, _) = isolated_source(reader.clone());
        let mut observed = Vec::new();
        for _ in 0..4 {
            let delivery = source
                .deliver(&ctx_in(room), 10_000, ResolutionPreference::Raw)
                .await;
            let inputs: Vec<crate::cognition::provenance::RoomInput> = delivery
                .items
                .iter()
                .filter(|item| item.metadata["unread"] == true)
                .map(|item| serde_json::from_value(item.metadata["room_input"].clone()).unwrap())
                .collect();
            if inputs.is_empty() {
                break;
            }
            observed.extend(inputs.iter().map(|input| input.cursor.event_id.as_uuid()));
            reader
                .acknowledge_presented(persona(), &inputs)
                .await
                .unwrap();
        }
        assert_eq!(observed, expected, "oldest unread, including the equal-lamport sibling outside the newest100, is presented once in order");

        // More than one raw page of typed maintenance must make bounded scan
        // progress without generating input or prematurely saving a bookmark.
        let before_controls = reader
            .read_cursor(persona(), room.as_uuid())
            .await
            .unwrap()
            .unwrap();
        for offset in 1..=5 {
            let work = airc_work::WorkEvent::ClaimHeartbeat(airc_work::ClaimHeartbeat {
                card_id: airc_work::WorkCardId::new(),
                claim_id: airc_work::ClaimId::new(),
                owner: reader.peer_id(),
                ttl_ms: 1000,
                heartbeat_at_ms: offset,
            });
            let (headers, body) = airc_work::encode_work_event(&work).unwrap();
            let mut event = event_in(room, None, before_controls.lamport + offset);
            event.kind = TranscriptKind::System;
            event.headers = headers;
            event.body = Some(body);
            reader.append_event(event).await.unwrap();
        }
        let after_controls = event_in(room, Some("after maintenance"), before_controls.lamport + 6);
        reader.append_event(after_controls.clone()).await.unwrap();
        let mut scan = source
            .builder
            .build_with_progress(persona(), room.as_uuid(), reader.as_ref(), 2, 0, None)
            .await
            .unwrap();
        let mut control_pages = 0;
        while !scan.has_unread() && control_pages < 5 {
            control_pages += 1;
            scan = source
                .builder
                .build_with_progress(
                    persona(),
                    room.as_uuid(),
                    reader.as_ref(),
                    2,
                    0,
                    Some(&scan),
                )
                .await
                .unwrap();
        }
        assert!(control_pages >= 2);
        assert_eq!(scan.unread().len(), 1);
        assert_eq!(
            scan.unread()[0].event_id(),
            after_controls.event_id.as_uuid()
        );
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            Some(before_controls.clone())
        );
        reader
            .acknowledge_presented(
                persona(),
                &[crate::cognition::provenance::RoomInput {
                    room_id: room.as_uuid(),
                    cursor: after_controls.cursor(),
                    read_after: scan.bookmark,
                }],
            )
            .await
            .unwrap();

        let saved = reader
            .read_cursor(persona(), room.as_uuid())
            .await
            .unwrap()
            .unwrap();
        let tip = reader.latest_cursor().await.unwrap().unwrap();
        let mut malformed = event_in(room, None, tip.lamport + 1);
        malformed.kind = TranscriptKind::System;
        malformed.headers.insert(
            airc_protocol::HEADER_FORGE_BODY_HINT.into(),
            airc_work::BODY_HINT_FORGE_WORK_REVIEW.into(),
        );
        malformed.body = Some(Body::Json(
            serde_json::json!({"kind":"work_submission_reviewed"}),
        ));
        let malformed_cursor = malformed.cursor();
        reader.append_event(malformed).await.unwrap();
        let later = event_in(room, Some("after malformed"), tip.lamport + 2);
        reader.append_event(later.clone()).await.unwrap();
        let blocked = source
            .builder
            .build_with_progress(persona(), room.as_uuid(), reader.as_ref(), 8, 0, None)
            .await
            .unwrap();
        assert!(
            blocked
                .scanned_through
                .as_ref()
                .is_some_and(|cursor| cursor_key(cursor) < cursor_key(&malformed_cursor)),
            "malformed input is not a control-only scan advance"
        );
        let error = reader
            .acknowledge_presented(
                persona(),
                &[crate::cognition::provenance::RoomInput {
                    room_id: room.as_uuid(),
                    cursor: later.cursor(),
                    read_after: Some(saved.clone()),
                }],
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("work_event_decode_error"));
        assert_eq!(
            reader.read_cursor(persona(), room.as_uuid()).await.unwrap(),
            Some(saved)
        );

        // Reuse the same real store in another room: a submitted head excerpt
        // must not acknowledge the unseen suffix, even when generation succeeds.
        let oversized_room = reader.join("digest-oversized-proof").await.unwrap().channel;
        let oversized = event_in(oversized_room, Some(&"unread detail ".repeat(1000)), 1);
        reader.append_event(oversized.clone()).await.unwrap();
        // The first-join page excludes controls before applying its limit, so
        // lease traffic cannot hide the ordinary fact behind the newest100.
        for index in 0..=FETCH_LIMIT {
            let mut control = event_in(oversized_room, Some("control"), index as u64 + 2);
            control.kind = TranscriptKind::System;
            match index % 5 {
                0 | 1 => {
                    let work = if index % 5 == 0 {
                        airc_work::WorkEvent::ClaimHeartbeat(airc_work::ClaimHeartbeat {
                            card_id: airc_work::WorkCardId::new(),
                            claim_id: airc_work::ClaimId::new(),
                            owner: reader.peer_id(),
                            ttl_ms: 1000,
                            heartbeat_at_ms: index as u64,
                        })
                    } else {
                        airc_work::WorkEvent::WorkspaceHeartbeat(airc_work::WorkspaceHeartbeat {
                            workspace_id: airc_work::WorkspaceId::new(),
                            disk_bytes: None,
                            heartbeat_at_ms: index as u64,
                        })
                    };
                    let (headers, body) = airc_work::encode_work_event(&work).unwrap();
                    control.headers = headers;
                    control.body = Some(body);
                }
                kind => {
                    let key = match kind {
                        2 => airc_protocol::HEADER_AIRC_CORRELATION_ID,
                        3 => airc_lib::HEADER_STREAM_ID,
                        _ => airc_lib::HEADER_HEARTBEAT_KIND,
                    };
                    control.headers.insert(key.into(), "control".into());
                }
            }
            reader.append_event(control).await.unwrap();
        }
        let first_page = reader
            .page_recent_in(Some(oversized_room), 1)
            .await
            .unwrap();
        assert_eq!(first_page.len(), 1);
        assert_eq!(first_page[0].event_id, oversized.event_id);
        let excerpt = source
            .deliver(&ctx_in(oversized_room), 64, ResolutionPreference::Raw)
            .await;
        assert_eq!(excerpt.items.len(), 1);
        assert!(excerpt.items[0].content.contains("trimmed"));
        assert!(excerpt.items[0].metadata.get("room_input").is_none());
        reader.acknowledge_presented(persona(), &[]).await.unwrap();
        assert_eq!(
            reader
                .read_cursor(persona(), oversized_room.as_uuid())
                .await
                .unwrap(),
            None
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
            delivery.items[0].content.starts_with("OPERATOR")
                && delivery.items[0].content.contains("trimmed"),
            "oldest low-frequency speaker survives, trimmed: {:?}",
            delivery.items[0].content
        );
        assert!(
            delivery.items[0].metadata.get("room_input").is_none(),
            "a trimmed head cannot acknowledge the omitted suffix"
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

    fn receipt_from(
        room: RoomId,
        peer: PeerId,
        thought: &str,
        act: &str,
        lamport: u64,
    ) -> TranscriptEvent {
        let mut ev = event_in(room, Some(&format!("💭 {thought}\n⚙ {act}")), lamport);
        ev.peer_id = peer;
        ev
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
            events.push(receipt_from(
                room,
                atlas,
                &format!("thought {l}"),
                "code/shell ls ✓",
                l,
            ));
            // twelve workers interleave: another author's receipt between each of Atlas's
            events.push(receipt_from(
                room,
                lorcan,
                &format!("lorcan {l}"),
                "code/read x ✓",
                l + 100,
            ));
        }
        events.push(receipt_from(
            room,
            atlas,
            "thought six",
            "code/github/issue-create  ✗",
            200,
        ));
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
            delivery
                .items
                .iter()
                .map(|i| i.content.clone())
                .collect::<Vec<_>>()
        );
        let lorcan_unit = &delivery.items[1].content;
        assert!(delivery.items[1].metadata.get("room_input").is_none());
        assert!(delivery.items[2].metadata.get("room_input").is_none());
        assert!(
            lorcan_unit.starts_with("💭 lorcan 5"),
            "Lorcan's newest leads: {lorcan_unit:?}"
        );
        assert!(lorcan_unit.contains("code/read ✓×4"), "{lorcan_unit:?}");
        let run = &delivery.items[2].content;
        assert!(
            run.starts_with("💭 thought six"),
            "newest thought leads: {run:?}"
        );
        assert!(run.contains("5 act batches"), "batch count: {run:?}");
        assert!(run.contains("code/shell ✓×4"), "tally: {run:?}");
        assert!(run.contains("code/github/issue-create ✗"), "tally: {run:?}");
        assert!(
            !run.contains("thought 2"),
            "older thoughts folded away: {run:?}"
        );
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
        let digest = source
            .builder
            .build_from_events(persona(), room.as_uuid(), events, 0, None);
        for working in [false, true] {
            let (items, _) = AircRagSource::pack_digest(&digest, 4_000, working);
            let seen: Vec<&str> = items.iter().map(|i| i.content.as_str()).collect();
            assert_eq!(
                seen.len(),
                2,
                "working={working}: only the two real lines survive: {seen:?}"
            );
            assert!(seen[0].starts_with("Kira:"), "{seen:?}");
            assert!(
                seen[1].starts_with("Atlas:"),
                "a line DISCUSSING an envelope is speech: {seen:?}"
            );
        }
        // And the live path (the one every turn takes) delivers the same two.
        let delivery = source
            .deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(
            delivery.items.len(),
            2,
            "{:?}",
            delivery
                .items
                .iter()
                .map(|i| &i.content)
                .collect::<Vec<_>>()
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
        let (source, _) = isolated_source(reader);
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
                    sender_id: sender.to_string(),
                    media: Vec::new(),
                    text: "the wordstats tests are next".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m2".into(),
                    sender_id: sender.to_string(),
                    // Duplicate of the live event — must be deduped, not doubled.
                    media: Vec::new(),
                    text: "Hello everyone! I'm Benchy.".into(),
                },
                crate::persona::durable_history::HydratedLine {
                    message_id: "m3".into(),
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
    }
}
