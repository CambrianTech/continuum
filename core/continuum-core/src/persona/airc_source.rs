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
pub(crate) const SOURCE_ID: &str = "airc";

/// Default newest-events fetch cap when building a digest on demand (mirrors the
/// region's). The recipe-defined grounding window slices within this.
const FETCH_LIMIT: usize = 100;

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use continuum_base::token_estimate::estimate_prompt_tokens as estimate_tokens;
use crate::cognition::token_budget::head_to_tokens;

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
    fetch_limit: usize,
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

/// The oldest kept line of a room's window, as the event it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HistoryAnchor {
    pub event: uuid::Uuid,
}

/// The share of the window budget left free when the start jumps, so the next turns'
/// new events fit without moving it again. A quarter: the start moves once per ~quarter
/// window of new traffic, and every turn in between extends the cached prefix instead
/// of replacing it. A larger slack buys fewer jumps at the cost of history shown.
pub(crate) const HISTORY_SLACK: f32 = 0.25;

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
            anchors: std::sync::Mutex::new(std::collections::HashMap::new()),
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
    /// Returns the packed items, the tokens they cost, and the READ-THROUGH
    /// ELEMENT: the newest element that actually entered her prompt. That last
    /// value is what advances her cursor — she is marked read for what she was
    /// GIVEN, never for what was merely fetched and then dropped by the budget.
    ///
    /// The ELEMENT, not a lamport, because airc's `save_runtime_cursor_for_event`
    /// wants the source event (it carries the room + kind and emits
    /// `SubscriptionAdvanced`). Handing it a bare number would throw that away.
    fn pack_digest(
        digest: &ChannelDigest,
        budget: u32,
        working: bool,
        anchor: Option<HistoryAnchor>,
    ) -> (Vec<RagItem>, u32, Option<Arc<ChannelElement>>, Option<HistoryAnchor>) {
        // Per-turn cap: budget/8 → a useful window holds ~8+ turns; clamped so
        // tiny budgets still render a sentence and huge ones don't let one
        // essay crowd the window.
        let per_turn_cap = (budget / 8).clamp(48, 256);
        let (units, quarantined) = Self::collapse_work_receipts(digest, working);
        if quarantined > 0 {
            continuum_base::probe!(
                class = "perception.quarantined",
                lines = quarantined as u64,
                working = working,
                "transcript lines the speak gate would refuse were kept out of her burst — the contagion carrier (card 169eb543)"
            );
        }
        // Cost every unit once, newest-first (the newest is exempt from the per-turn
        // cap: it is what she is responding to). `costed` is oldest-first after the
        // reverse, parallel to `units`, with None for a unit that has no text.
        let mut costed: Vec<Option<(u32, Option<String>)>> = Vec::with_capacity(units.len());
        let mut newest_kept = true;
        for unit in units.iter().rev() {
            let idx = unit.last_idx;
            let text: &str = match unit.collapsed.as_deref() {
                Some(t) => t,
                None => match digest.elements[idx].text() {
                    Some(t) => t,
                    None => {
                        costed.push(None);
                        continue;
                    }
                },
            };
            let full = estimate_tokens(text);
            let cap = if newest_kept { budget } else { per_turn_cap };
            let (cost, trimmed) = if full <= cap {
                (full, unit.collapsed.clone())
            } else {
                let head = head_to_tokens(text, cap);
                let head_cost = estimate_tokens(&head).saturating_add(2); // marker
                // The marker SPELLING is shared with the tail-trim site so the two
                // cannot drift apart again (card 3833b472). The +2 reserve above is
                // left as it was on purpose: `trim_marker_tokens()` measures the real
                // cost at 8, so this site under-reserves, but raising it changes how
                // many turns fit in every prompt — a packing change that belongs in
                // its own card with its own before/after, not smuggled into a
                // disclosure fix.
                (
                    head_cost,
                    Some(crate::cognition::token_budget::mark_head_kept(
                        &head, full,
                    )),
                )
            };
            newest_kept = false;
            costed.push(Some((cost, trimmed)));
        }
        costed.reverse();
        // THE START HOLDS STILL. The window is the suffix of units from `start`; its
        // cost is the sum from there to the newest. Where the fill would naturally
        // begin (newest-first to the budget) is `natural`. The anchor from last turn
        // is kept while the suffix from it still fits; when it no longer fits, the
        // start jumps FORWARD past `natural` to leave HISTORY_SLACK of the budget
        // free, so the next turns' new events fit without moving it again. Every
        // turn in between then extends the prefix the engine already holds, instead
        // of replacing its first message. The first fill in a room (no anchor yet)
        // shows everything the budget holds; the room's second turn then pays one
        // jump and holds from there. An anchored event that scrolled out of the
        // digest is a jump too.
        let suffix_cost = |start: usize| -> u32 {
            costed[start..].iter().flatten().map(|(c, _)| *c).fold(0u32, |a, c| a.saturating_add(c))
        };
        let first_start_within = |limit: u32| -> usize {
            // the smallest start whose suffix fits `limit`; units.len() = nothing fits
            (0..=units.len()).find(|&start| suffix_cost(start) <= limit).unwrap_or(units.len()) // unwrap_or: the empty suffix always fits, so this is never reached
        };
        let natural = first_start_within(budget);
        // The anchor is the event at the unit's ORDER position (a run's first receipt),
        // never its newest: keyed on the newest, an open run at the window's start would
        // look scrolled out on every receipt and jump each turn (Cormac on #4806).
        let anchored = anchor.and_then(|a| {
            units.iter().position(|u| digest.elements[u.order_idx].event_id() == a.event)
        });
        let slack_limit = ((budget as f32) * (1.0 - HISTORY_SLACK)) as u32;
        let start = match (anchor, anchored) {
            // the first fill in this room: everything the budget holds
            (None, _) => natural,
            // last turn's start still fits: hold it, even if more would fit now
            (Some(_), Some(a)) if a >= natural => a,
            // it no longer fits, or it scrolled out: jump forward with headroom; never
            // past the newest unit, which is always shown
            _ => first_start_within(slack_limit).min(units.len().saturating_sub(1)),
        };
        let mut keep: Vec<(usize, Option<String>)> = Vec::new();
        let mut tokens_used: u32 = 0;
        for (unit, c) in units[start..].iter().zip(&costed[start..]) {
            if let Some((cost, trimmed)) = c {
                tokens_used = tokens_used.saturating_add(*cost);
                keep.push((unit.last_idx, trimmed.clone()));
            }
        }
        let new_anchor = units.get(start).map(|u| HistoryAnchor { event: digest.elements[u.order_idx].event_id() });
        if anchored.is_some_and(|a| a != start) || (anchored.is_none() && anchor.is_some()) {
            continuum_base::probe!(
                class = "perception.history.anchor_moved",
                room = %digest.room_id,
                from_natural = natural as u64,
                start = start as u64,
                units = units.len() as u64,
                tokens = tokens_used as u64,
                budget = budget as u64,
                "the start of her history in this room jumped: the prefix the engine held is replaced once, then holds again"
            );
        }
        // `keep` is oldest-first after the reverse, so its LAST entry is the newest
        // element that actually fit — how far she genuinely read this turn.
        // the newest element that entered her prompt, by the elements' own order (a run's
        // line sits at its first receipt but reads through its newest)
        let read_through = keep.iter().map(|(idx, _)| *idx).max().map(|idx| digest.elements[idx].clone());
        let items = keep
            .into_iter()
            .map(|(idx, trimmed)| {
                Self::format_item(&digest.elements[idx], idx >= digest.unread_start, trimmed)
            })
            .collect();
        (items, tokens_used, read_through, new_anchor)
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
                .map(|(idx, _)| PackUnit { last_idx: idx, order_idx: idx, collapsed: None })
                .collect();
            return (units, quarantined);
        }
        use std::collections::HashMap;
        // RUNS, DELIMITED BY SPEECH: an author's receipts fold into one line per run,
        // and a run ends at the next real line anyone says. "What everyone did since the
        // last thing said." Every run behind the last chat line is therefore FINAL: its
        // text and its place never change again, so the prefix the engine holds survives
        // every new receipt; only the open runs at the tail grow. A run's line sits at
        // its FIRST receipt and reads at its newest (the cursor).
        struct Run {
            first: usize,
            receipts: Vec<usize>,
        }
        let mut runs: Vec<Run> = Vec::new();
        let mut open: HashMap<uuid::Uuid, usize> = HashMap::new(); // author → index into runs
        for (idx, el) in digest.elements.iter().enumerate() {
            if !contributes[idx] {
                continue;
            }
            match el.text() {
                Some(text) if is_work_receipt(text) => {
                    let author = el.sender_id();
                    match open.get(&author) {
                        Some(&r) => runs[r].receipts.push(idx),
                        None => {
                            open.insert(author, runs.len());
                            runs.push(Run { first: idx, receipts: vec![idx] });
                        }
                    }
                }
                _ => open.clear(), // a line said closes every run
            }
        }
        let mut units: Vec<PackUnit> = Vec::new();
        for (idx, el) in digest.elements.iter().enumerate() {
            if !contributes[idx] {
                continue; // refused at the speak seam → never shown at the perception seam
            }
            if !el.text().is_some_and(is_work_receipt) {
                units.push(PackUnit { last_idx: idx, order_idx: idx, collapsed: None });
            }
        }
        for run in &runs {
            let newest = *run.receipts.last().unwrap_or(&run.first); // unwrap_or: a run is made with its first receipt, so this is never reached
            let collapsed = if run.receipts.len() == 1 {
                None
            } else {
                Some(Self::collapsed_receipt_text(
                    run.receipts.iter().filter_map(|i| digest.elements[*i].text()),
                    run.receipts.len(),
                ))
            };
            units.push(PackUnit { last_idx: newest, order_idx: run.first, collapsed });
        }
        units.sort_by_key(|u| u.order_idx);
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
            .map(|(k, n)| if *n > 1 { format!("{k}×{n}") } else { k.clone() })
            .collect();
        format!(
            "{} · ⚙ {batches} act batches: {}",
            last_thought.unwrap_or("💭 (working)"), // unwrap_or: a run of bare act lines has no thought to lead with
            if acts.is_empty() { "(no receipts)".to_string() } else { acts.join(" · ") }
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
    /// WHERE the unit sits in the window: the run's FIRST receipt, so a folded line
    /// never relocates when its author posts another receipt (Cormac on #4806: the fold
    /// sat at the newest receipt and moved from mid-window to the end on every receipt,
    /// so the prefix diverged mid-history whatever the anchor held). A plain line's
    /// order is its own index.
    order_idx: usize,
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
                                crate::persona::room_event::room_content_from_event(e)
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
                        continuum_base::probe!(
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
mod tests;
