//! Her own thoughts and the answers she settled, one item each: every thought in her log, her
//! newest whole, and how much of each older one she reads is the one allocation's at an epoch
//! start. Her actions are not here: they are the action stream, rendered once
//! ([`super::actions`]).

use continuum_context::cause::{Cause, Content, Origin};
use continuum_context::source::{Item, Source, StoredWhole};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::{ThoughtRecord, WmKind, WorkingMemory};

/// One entry of her interior chain.
enum Entry {
    Thought(ThoughtRecord),
    Settled(String),
}

pub(crate) struct Thoughts<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose occurrences these are (`super::element`).
    pub persona: uuid::Uuid,
}

impl Source<ContextMessage> for Thoughts<'_> {
    fn name(&self) -> &str {
        "thoughts"
    }

    fn priority(&self) -> u8 {
        priority::THOUGHTS
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        // Her thought log (`working_memory::thought_log`: every thought, across a turn of many acts
        // and a restart) and the answers she settled, in the one sequence of her memory.
        let mut entries: Vec<(u64, Entry)> = self.memory.thoughts().into_iter().map(|record| (record.occurrence, Entry::Thought(record))).collect();
        entries.extend(
            self.memory
                .recent_entries()
                .into_iter()
                .filter(|entry| entry.kind == WmKind::Settlement)
                .map(|entry| (entry.occurrence, Entry::Settled(entry.text))),
        );
        entries.sort_by_key(|(occurrence, _)| *occurrence);
        if entries.is_empty() {
            return Vec::new();
        }
        // First person, explicitly not a message (#264: a second-person header read as a
        // peer addressing her, and she thanked her own interoception).
        let banner = ContextMessage::user(
            "My own recent thoughts (working memory, my interior state, not a message from anyone):".to_owned(),
        );
        let mut items = vec![super::standing(self.persona, "thoughts", "banner", banner)];
        // Her thoughts are one interior chain: each follows the one before it.
        let mut previous = None;
        items.extend(entries.into_iter().map(|(occurrence, entry)| {
            // Keyed by its occurrence, never by what it says: a settlement that repeats an
            // earlier one ("passed" after "failed" after "passed") is its own item, so her latest
            // state reads right (it collapsed into the first and read as "failed"). A thought,
            // once recorded, does not change, and neither may its rendering: whole, its summary
            // (built once when recorded), or its pointer, and how much of her thoughts she reads is
            // the one allocation's call at an epoch start, never a per-act cut here (Benchy on the
            // M5, 2026-10-24: a share-of-window cut re-appended every thought on every act).
            let id = element(self.persona, Producer::Thought, &occurrence.to_be_bytes());
            let cause = previous.replace(id).map_or(Cause::Root(Origin::ActivityStart), Cause::Of);
            let key = format!("thought#{occurrence}");
            match entry {
                Entry::Settled(text) => {
                    let summary = continuum_context::levels::summarize(&text, continuum_context::levels::Shape::Output).map(|summary| ContextMessage::user(format!("- {summary}")));
                    Item::whole(id, cause, key, ContextMessage::user(format!("- {text}"))).with_summary(summary)
                }
                Entry::Thought(record) => {
                    let summary = record.summary.as_ref().map(|summary| ContextMessage::user(format!("- {summary}")));
                    let pointer = record.handle.as_ref().map(|handle| ContextMessage::user(format!("- (thought #{occurrence}, saved: tool/output {handle})")));
                    match (record.text, record.handle, record.stored_chars) {
                        // Resident: her whole thought.
                        (Some(text), _, _) => Item::whole(id, cause, key, ContextMessage::user(format!("- {text}"))).with_summary(summary).with_pointer(pointer),
                        // Shed to its handle: what is resident is its summary (or pointer); the whole
                        // is priced from its measured length and loaded when the allocation grants it.
                        (None, Some(handle), Some(chars)) => {
                            let resident = summary.clone().or_else(|| pointer.clone()).unwrap_or_else(|| ContextMessage::user(String::new())); // unwrap_or_else: a shed thought always has its pointer
                            Item::whole(id, cause, key, resident)
                                .with_summary(summary)
                                .with_pointer(pointer)
                                .with_stored(Some(StoredWhole { chars: chars + "- ".len(), handle, content: Content::Absent }))
                        }
                        // A shed record names its handle and length; anything else renders what it holds.
                        (None, _, _) => Item::whole(id, cause, key, summary.unwrap_or_else(|| ContextMessage::user(format!("- (thought #{occurrence})")))), // unwrap_or_else: a thought with nothing resident names itself
                    }
                }
            }
        }));
        items
    }

    /// A thought her working memory shed from RAM (storage, never visibility: FOCUS-CONTRACT rule
    /// 7), read back from its saved whole when her view grants it whole, rendered exactly as it
    /// read resident. `None` (its resident level renders) when the file is gone or unreadable.
    fn load(&self, item: &Item<ContextMessage>) -> Option<ContextMessage> {
        let stored = item.stored()?;
        let body = crate::cognition::tool_executor::spill::saved_whole(self.persona, &stored.handle).ok()?;
        Some(ContextMessage::user(format!("- {body}")))
    }
}
