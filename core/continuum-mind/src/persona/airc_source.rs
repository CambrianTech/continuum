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
use async_trait::async_trait;

use crate::cognition::channel_digest::{ChannelDigest, ChannelDigestBuilder, DEFAULT_GROUNDING};
use crate::cognition::channel_digest_region::DigestBuffer;
use crate::cognition::channel_element::ChannelElement;
use crate::cognition::channel_substrate::{
    global_channel_digest_buffer, global_channel_digest_builder,
};
use crate::cognition::rag_budget::{RagContext, RagDelivery, RagItem, RagSource};
use crate::runtime::ready_buffer::ReadyBuffer;
use crate::cognition::conversation::transcript_reader::AircTranscriptReader;

use crate::cognition::faculty_ids::AIRC;

/// Default newest-events fetch cap when building a digest on demand (mirrors the
/// region's). The recipe-defined grounding window slices within this.
const FETCH_LIMIT: usize = 100;

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)

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
    /// The digest last delivered per room: what `mark_read` resolves an event id against
    /// (airc's cursor wants the event itself, not a bare id).
    last_digest: std::sync::Mutex<std::collections::HashMap<uuid::Uuid, Arc<ChannelDigest>>>,
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
            last_digest: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
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

    /// Every unit of a digest, whole, oldest first: the source delivers levels, never a size
    /// (FLOW-IMPLEMENTATION-PLAN §2d). Her one allocation in the prompt view decides which
    /// turns she sees, and when a start moves. This replaced a per-turn cap, head cuts and an
    /// anchored start that sized the conversation against a second budget (the 159k served
    /// window) before the view sized it again against hers. Receipt runs still fold to one
    /// line per author, and quarantined lines still never enter.
    fn digest_items(digest: &ChannelDigest, working: bool) -> Vec<RagItem> {
        let (units, quarantined) = Self::collapse_work_receipts(digest, working);
        if quarantined > 0 {
            continuum_base::probe!(
                class = "perception.quarantined",
                lines = quarantined as u64,
                working = working,
                "transcript lines the speak gate would refuse were kept out of her burst — the contagion carrier (card 169eb543)"
            );
        }
        units
            .iter()
            .filter(|unit| unit.collapsed.is_some() || digest.elements[unit.last_idx].text().is_some())
            .map(|unit| Self::format_item(&digest.elements[unit.last_idx], unit.last_idx >= digest.unread_start, unit.collapsed.clone()))
            .collect()
    }

    /// She read the room through `event`: advance her per-room cursor to it, exactly as the
    /// human's UI does on mark-read. Called by the deliberation faculty after a prompt that
    /// rendered `event` was sent (its read-receipt hook), never at delivery: delivery is not
    /// reading, and only the view knows which turns she saw. Monotonic in airc, so a repeat
    /// or a lower event never rewinds her.
    pub async fn mark_read(&self, room_id: uuid::Uuid, event: uuid::Uuid) {
        let element = self
            .last_digest
            .lock()
            .unwrap_or_else(|p| p.into_inner()) // a poisoned map still answers, same policy as every lock in this crate
            .get(&room_id)
            .and_then(|digest| digest.elements.iter().find(|el| el.event().event_id.as_uuid() == event).cloned());
        let Some(element) = element else {
            continuum_base::probe!(
                class = "perception.mark_read.unknown_event",
                persona = %self.persona_id,
                room = %room_id,
                "a read receipt named an event the last digest of this room does not hold: the cursor stays"
            );
            return;
        };
        if let Err(err) = self.reader.advance_read_cursor(self.persona_id, room_id, element.event()).await {
            tracing::warn!(
                error = %err,
                persona_id = %self.persona_id,
                room = %room_id,
                "airc rag: could not persist the read cursor — she will re-read this page next turn (bounded)"
            );
        }
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
                if line.starts_with(crate::cognition::presence_glyph::THOUGHT) {
                    last_thought = Some(line);
                } else if let Some(rest) = line.strip_prefix(crate::cognition::presence_glyph::ACT) {
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
        collapsed: Option<String>,
    ) -> RagItem {
        let ev = element.event();
        let text = collapsed.unwrap_or_else(|| element.text().unwrap_or_default().to_string());
        RagItem::whole(
            ev.event_id.as_uuid().to_string(),
            text,
            serde_json::json!({
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
        )
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
    crate::cognition::presence_glyph::is_presence_line(t)
}

/// The speak gate's verdict, read at the perception seam — one predicate, two seams.
fn is_not_a_contribution(text: &str) -> bool {
    crate::cognition::not_speech::is_not_a_contribution(text).is_some()
}

#[async_trait]
impl RagSource for AircRagSource {
    fn source_id(&self) -> &'static str {
        AIRC
    }

    fn expand_command(&self) -> Option<&'static str> {
        Some("collaboration/chat/export")
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        // Defense in depth: never serve another persona's context.
        if ctx.persona_id != self.persona_id {
            return RagDelivery::empty(AIRC);
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
                return RagDelivery::empty(AIRC);
            }
        };
        let Some(room_id) = ctx
            .airc_room
            .map(|r| r.as_uuid())
            .or_else(|| events.last().map(|e| e.room_id.as_uuid()))
        else {
            // No room scope AND no transcript — genuinely nothing to digest.
            return RagDelivery::empty(AIRC);
        };

        // #249: when the LIVE window is shallower than the grounding target (the
        // post-reboot shape — the persona's airc runtime log restarted while the
        // room's real history lives in the durable store), top up from the durable
        // transcript. Hydrated lines enter at lamport 0 (grounding-only, see
        // `hydrated_event`) and are deduped against live events by body text +
        // sender (the durable row id is the airc event id for persona lines, but
        // live events re-mint EventIds across runtimes — content identity is the
        // honest join). Fetch failure degrades to the shallow window, loudly.
        // How much history to stage: the recipe's grounding, at least the fetch window. What
        // she then SEES of it is her view's decision (FLOW-IMPLEMENTATION-PLAN §2d), never a
        // budget here (#259's starvation came from sizing this window by a budget).
        let grounding = self.grounding.max(self.fetch_limit);
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
                                crate::cognition::conversation::room_event::room_content_from_event(e)
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
        // What a rebuild would hold: the room's events after any durable top-up, up to the
        // staging depth. A staged digest is reused only when it holds at least that much, so a
        // shallow stage (the region stages the recipe floor) or a top-up forces the rebuild.
        let rebuild_holds = events.iter().filter(|e| e.room_id.as_uuid() == room_id).count().min(grounding);
        let digest = match self.buffer.peek(&(self.persona_id, room_id)) {
            Some(d) if d.elements.len() >= rebuild_holds => d,
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

        let items = Self::digest_items(&digest, crate::cognition::persona_workspace::acting_root_of(self.persona_id).is_some());
        let tokens_used = items.iter().map(|item| item.tokens).fold(0u32, |a, t| a.saturating_add(t));
        // Kept for `mark_read`: the faculty marks her read after a sent prompt rendered these
        // turns, never here (delivery is not reading).
        self.last_digest.lock().unwrap_or_else(|p| p.into_inner()).insert(room_id, Arc::clone(&digest)); // a poisoned map still takes the digest; losing it costs one unmarked turn, never a panic
        tracing::debug!(
            persona_id = %self.persona_id,
            room = %room_id,
            window = digest.elements.len(),
            items = items.len(),
            tokens_used,
            "airc_rag: deliver (digest, every unit whole)"
        );
        // The digest IS the window. More history = a command (scrollback/search),
        // not a budget continuation cursor.
        RagDelivery::of(AIRC, items, false)
    }

}

#[async_trait::async_trait]
impl crate::cognition::llm_deliberation_faculty::ReadReceipt for AircRagSource {
    async fn read_through(&self, room: uuid::Uuid, event: uuid::Uuid) {
        self.mark_read(room, event).await;
    }
}

#[cfg(test)]
mod tests;
