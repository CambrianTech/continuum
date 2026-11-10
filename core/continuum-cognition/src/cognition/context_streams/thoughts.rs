//! Her own thoughts and the answers she settled, one item each: every thought in her log, her
//! newest whole, and how much of each older one she reads is the one allocation's at an epoch
//! start. Her actions are not here: they are the action stream, rendered once
//! ([`super::actions`]).

use continuum_context::cause::{Cause, Content, Origin};
use continuum_context::source::{Item, Source, StoredWhole};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::{ThoughtRecord, WmKind, WorkingMemory};
use crate::cognition::tool_executor::tool_output::Reopen;

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
                    let pointer = record.handle.as_ref().map(|handle| ContextMessage::user(format!("- (thought #{occurrence}, saved: {})", Reopen::Output(handle))));
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

    /// Her oldest thoughts as ONE line: the first and last `#n`, how many, and how to reopen the
    /// newest of them (each was saved whole when shed), so her floor is one line however many
    /// thoughts she has had. Without it every shed thought floored at its own pointer, forever:
    /// Benchy's 2,536 pointers were 78,755 tokens against a 27,333 window, so every act was
    /// refused before it reached the model; Aris's and Aiko's bound the same way (the M5,
    /// 2026-11-09, `delib.payload.over_window` binding `view/thoughts`). Reads only the run's
    /// ends. `None` for a run with nothing saved in it: resident thoughts leave when collapsed.
    fn fold(&self, folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        let occurrence = |item: &Item<ContextMessage>| item.key().strip_prefix("thought#").and_then(|n| n.parse::<u64>().ok());
        let first = folded.iter().find_map(occurrence)?;
        let last = folded.iter().rev().find_map(occurrence)?;
        let saved = folded.iter().rev().find_map(|item| item.stored())?;
        Some(ContextMessage::user(format!(
            "- [thoughts #{first}-#{last}: {} earlier thoughts folded; the newest of them reopens with {}]",
            folded.len(),
            Reopen::Output(&saved.handle)
        )))
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

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_context::cost::{Chars, Estimate};
    use continuum_context::epoch::Block;

    /// A shed thought as `items` builds it: resident is its pointer, its whole stored by handle.
    fn shed(persona: uuid::Uuid, occurrence: u64) -> Item<ContextMessage> {
        let id = element(persona, Producer::Thought, &occurrence.to_be_bytes());
        let handle = format!("thought-{occurrence:08}");
        let pointer = ContextMessage::user(format!("- (thought #{occurrence}, saved: {})", Reopen::Output(&handle)));
        Item::whole(id, Cause::Root(Origin::ActivityStart), format!("thought#{occurrence}"), pointer.clone())
            .with_pointer(Some(pointer))
            .with_stored(Some(StoredWhole { chars: 2_000, handle, content: Content::Absent }))
    }

    // what this catches: a thoughts source with no fold floors every shed thought at its own
    // pointer, so her required floor grows with every thought she ever had and outgrows her window
    // (Benchy, the M5, 2026-11-09: 2,536 thought pointers, 78,755 tokens against 27,333, every act
    // refused before it reached the model). Ten times the thoughts must not cost more at the floor.
    #[test]
    fn her_thoughts_floor_at_one_fold_line_however_many_she_has() {
        let memory = WorkingMemory::new(8);
        let persona = uuid::Uuid::from_u128(7);
        let source = Thoughts { memory: &memory, persona };
        let run = |n: u64| (1..=n).map(|occurrence| shed(persona, occurrence)).collect::<Vec<_>>();
        let few = continuum_context::view::floor(&Chars, &source, &run(100));
        let many = continuum_context::view::floor(&Chars, &source, &run(1_000));
        let one_pointer = Chars.block(&shed(persona, 1).pointer().cloned().unwrap_or_else(|| ContextMessage::user(String::new()))); // unwrap_or_else: shed() always sets a pointer
        assert!(few < 2 * one_pointer, "100 thoughts floor at {few} chars, more than one line");
        assert!(many <= few + 8, "1,000 thoughts floor at {many} chars against {few} for 100: the floor grows with her log");
        let folded = source.fold(&run(1_000)).map(|m| m.text().to_owned()).unwrap_or_default(); // unwrap_or_default: an empty line fails below
        assert!(folded.contains("#1-#1000") && folded.contains("1000 earlier thoughts") && folded.contains("thought-00001000"), "{folded}");
    }
}
