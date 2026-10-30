//! Her own recent thoughts and the answers she settled, one item each. Her actions are not
//! here: they are the action stream, rendered once ([`super::actions`]).

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::{collapse_thought, render_dispatched, WmKind, WorkingMemory};

/// What a collapsed thought keeps: its end, where the decision is.
const POINTER_KEEP: usize = 200;

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
        let thoughts: Vec<String> = self
            .memory
            .recent_entries()
            .into_iter()
            .filter(|entry| matches!(entry.kind, WmKind::Thought | WmKind::Settlement))
            .map(|entry| entry.text)
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
        items.extend(thoughts.into_iter().map(|text| {
            let id = element(self.persona, Producer::Thought, text.as_bytes());
            let cause = previous.replace(id).map_or(Cause::Root(Origin::ActivityStart), Cause::Of);
            // Keyed by what it says: a thought, once recorded, does not change, and neither may
            // its rendering. Full is the whole thought; how much of her thoughts she reads is the
            // one allocation's call at an epoch start (collapsed to `pointer`), never a per-act
            // head here. Cut to a share of the served window, the same thought rendered 1,284
            // chars on one act and 1,445 on the next, so every act re-appended every thought
            // (Benchy on the M5, 2026-10-24 22:3xZ: ~7k chars of unchanged thoughts per act).
            Item::whole(id, cause, text.clone(), ContextMessage::user(format!("- {text}")))
                .with_pointer(ContextMessage::user(format!("- {}", collapse_thought(&text, POINTER_KEEP))))
        }));
        // Fresh each event, newest last; one item, so a status change appends one line set.
        items.extend(dispatched.map(|text| {
            super::standing(self.persona, "thoughts", "dispatched", ContextMessage::user(text))
        }));
        items
    }
}
