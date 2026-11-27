//! AircRagSource: what was said in the turn's room, rendered from her thread of it at the revision
//! the turn pinned.
//!
//! ### One record, read once
//!
//! The airc room is the one record, and her thread holds its newest lines by handle. The turn reads
//! the thread's revision once, before it composes, and hands it down in the [`RagContext`]: this
//! source renders that value and reads nothing itself. So the conversation in her prompt is the same
//! revision her facts, her anchor, her capture and her read receipt name. Before this the source
//! paged airc on every turn, topped the page up from the chat store (#249), peeked or rebuilt a
//! node-wide digest and awaited her read cursor: three more copies of the room, read at a different
//! moment than the revision the turn held.
//!
//! ### What it shapes
//!
//! Every line arrives whole, oldest first: her one view decides which she sees (FLOW plan §2d).
//! Work receipts fold to one line per author per run, and a line the speak gate would refuse never
//! enters (card 169eb543). Her read cursor moves only on a sent prompt's read receipt, through the
//! event that prompt rendered, resolved against the revision that prompt carried.

use std::sync::Arc;

use airc_core::TranscriptEvent;
use async_trait::async_trait;
use continuum_grounding::room_thread::{Item, RoomThread};

use crate::cognition::conversation::transcript_reader::AircTranscriptReader;
use crate::cognition::faculty_ids::AIRC;
use crate::cognition::rag_budget::{RagContext, RagDelivery, RagItem, RagSource};

/// Persona-bound source rendering her pinned thread of the turn's room.
pub struct AircRagSource {
    persona_id: uuid::Uuid,
    /// Where her read cursor is kept (airc's): written by a sent prompt's read receipt, never read
    /// on the turn path.
    reader: Arc<dyn AircTranscriptReader>,
}

impl AircRagSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircTranscriptReader>) -> Self {
        Self { persona_id, reader }
    }

    /// Every unit of the thread, whole, oldest first: the source delivers levels, never a size
    /// (FLOW-IMPLEMENTATION-PLAN §2d). Receipt runs fold to one line per author, and quarantined
    /// lines never enter.
    fn thread_items(thread: &RoomThread, working: bool) -> Vec<RagItem> {
        let items = thread.items();
        let (units, quarantined) = Self::collapse_work_receipts(items, working);
        if quarantined > 0 {
            continuum_base::probe!(
                class = "perception.quarantined",
                lines = quarantined as u64,
                working = working,
                "transcript lines the speak gate would refuse were kept out of her burst — the contagion carrier (card 169eb543)"
            );
        }
        units.into_iter().map(|unit| Self::format_item(&items[unit.last_idx], unit.collapsed)).collect()
    }

    fn collapse_work_receipts(items: &[Item], working: bool) -> (Vec<PackUnit>, usize) {
        let mut quarantined = 0usize;
        let contributes: Vec<bool> = items
            .iter()
            .map(|item| {
                if working && is_work_receipt(&item.text) {
                    return true; // dropped below as presence, not as quarantine
                }
                let ok = !is_not_a_contribution(&item.text);
                if !ok {
                    quarantined += 1;
                }
                ok
            })
            .collect();
        if working {
            let units = items
                .iter()
                .enumerate()
                .filter(|(idx, item)| contributes[*idx] && !is_work_receipt(&item.text))
                .map(|(idx, _)| PackUnit { last_idx: idx, order_idx: idx, collapsed: None })
                .collect();
            return (units, quarantined);
        }
        use std::collections::HashMap;
        struct Run {
            first: usize,
            receipts: Vec<usize>,
        }
        let mut runs: Vec<Run> = Vec::new();
        let mut open: HashMap<uuid::Uuid, usize> = HashMap::new(); // author → index into runs
        for (idx, item) in items.iter().enumerate() {
            if !contributes[idx] {
                continue;
            }
            if is_work_receipt(&item.text) {
                match open.get(&item.sender) {
                    Some(&r) => runs[r].receipts.push(idx),
                    None => {
                        open.insert(item.sender, runs.len());
                        runs.push(Run { first: idx, receipts: vec![idx] });
                    }
                }
            } else {
                open.clear(); // a line said closes every run
            }
        }
        let mut units: Vec<PackUnit> = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            if !contributes[idx] {
                continue; // refused at the speak seam → never shown at the perception seam
            }
            if !is_work_receipt(&item.text) {
                units.push(PackUnit { last_idx: idx, order_idx: idx, collapsed: None });
            }
        }
        for run in &runs {
            let newest = *run.receipts.last().unwrap_or(&run.first); // unwrap_or: a run is made with its first receipt, so this is never reached
            let collapsed = if run.receipts.len() == 1 {
                None
            } else {
                Some(Self::collapsed_receipt_text(run.receipts.iter().map(|i| items[*i].text.as_str()), run.receipts.len()))
            };
            units.push(PackUnit { last_idx: newest, order_idx: run.first, collapsed });
        }
        units.sort_by_key(|u| u.order_idx);
        (units, quarantined)
    }

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
        let acts: Vec<String> = tally.iter().map(|(k, n)| if *n > 1 { format!("{k}×{n}") } else { k.clone() }).collect();
        format!(
            "{} · ⚙ {batches} act batches: {}",
            last_thought.unwrap_or("💭 (working)"), // unwrap_or: a run of bare act lines has no thought to lead with
            if acts.is_empty() { "(no receipts)".to_string() } else { acts.join(" · ") }
        )
    }

    fn format_item(item: &Item, collapsed: Option<String>) -> RagItem {
        let record = &item.record;
        let event = item.event().to_string();
        let metadata = serde_json::json!({
            "event_id": &event,
            "room_id": record.room_id.as_uuid().to_string(),
            "peer_id": item.sender.to_string(),
            "occurred_at_ms": record.occurred_at_ms,
            "lamport": record.lamport,
        });
        let text = match collapsed {
            Some(folded) => folded,
            None => item.text.clone(),
        };
        RagItem::whole(event, text, metadata)
    }
}

struct PackUnit {
    last_idx: usize,
    order_idx: usize,
    collapsed: Option<String>,
}

fn is_work_receipt(text: &str) -> bool {
    crate::cognition::presence_glyph::is_presence_line(text.trim_start())
}

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
        if ctx.persona_id != self.persona_id {
            return RagDelivery::empty(AIRC);
        }
        let Some(thread) = ctx.thread.as_ref() else {
            continuum_base::probe!(class = "airc_rag.unpinned", persona = %self.persona_id, "the turn pinned no thread of a room: nothing was said that she can see");
            return RagDelivery::empty(AIRC);
        };
        if ctx.airc_room.is_some_and(|room| room.as_uuid() != thread.room) {
            continuum_base::probe!(class = "airc_rag.thread_elsewhere", persona = %self.persona_id, thread = %thread.room, "the pinned thread is of another room than the turn's: refused");
            return RagDelivery::empty(AIRC);
        }
        let items = Self::thread_items(thread, crate::cognition::persona_workspace::acting_root_of(self.persona_id).is_some());
        tracing::debug!(persona_id = %self.persona_id, room = %thread.room, revision = thread.revision, items = items.len(), "airc_rag: deliver (pinned thread, every unit whole)");
        RagDelivery::of(AIRC, items, false)
    }
}

#[async_trait::async_trait]
impl crate::cognition::llm_deliberation_faculty::ReadReceipt for AircRagSource {
    async fn read_through(&self, room: uuid::Uuid, record: &TranscriptEvent) {
        if let Err(err) = self.reader.advance_read_cursor(self.persona_id, room, record).await {
            tracing::warn!(
                error = %err,
                persona_id = %self.persona_id,
                room = %room,
                "airc rag: could not persist the read cursor — she will re-read this page next turn (bounded)"
            );
        }
    }
}

#[cfg(test)]
mod tests;
