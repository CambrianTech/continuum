//! Her own recent thoughts and the answers she settled, one item each. Her actions are not
//! here: they are the action stream, rendered once ([`super::actions`]).

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::{render_dispatched, WmKind, WorkingMemory};

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
        let thoughts: Vec<(u64, String)> = self
            .memory
            .recent_entries()
            .into_iter()
            .filter(|entry| matches!(entry.kind, WmKind::Thought | WmKind::Settlement))
            .map(|entry| (entry.occurrence, entry.text))
            .collect();
        let dispatched = render_dispatched(&self.memory.dispatched_snapshot());
        if thoughts.is_empty() && dispatched.is_none() {
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
        items.extend(thoughts.into_iter().map(|(occurrence, text)| {
            // Keyed by its occurrence, never by what it says: a settlement that repeats an
            // earlier one ("passed" after "failed" after "passed") is its own item, so her latest
            // state reads right (it collapsed into the first and read as "failed"). A thought,
            // once recorded, does not change, and neither may its rendering: whole, its summary
            // from the codec (`levels`), and how much of her thoughts she reads is the one
            // allocation's call at an epoch start, never a per-act cut here (Benchy on the M5,
            // 2026-10-24: a share-of-window cut re-appended every thought on every act).
            let id = element(self.persona, Producer::Thought, &occurrence.to_be_bytes());
            let cause = previous.replace(id).map_or(Cause::Root(Origin::ActivityStart), Cause::Of);
            let summary = continuum_context::levels::summarize(&text, continuum_context::levels::Shape::Output)
                .map(|summary| ContextMessage::user(format!("- {summary}")));
            Item::whole(id, cause, format!("thought#{occurrence}"), ContextMessage::user(format!("- {text}"))).with_summary(summary)
        }));
        // Fresh each event, newest last; one item, so a status change appends one line set.
        items.extend(dispatched.map(|text| {
            super::standing(self.persona, "thoughts", "dispatched", ContextMessage::user(text))
        }));
        items
    }
}
