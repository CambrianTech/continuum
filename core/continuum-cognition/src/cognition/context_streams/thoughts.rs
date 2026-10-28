//! Her own recent thoughts and the answers she settled, one item each. Her actions are not
//! here: they are the action stream, rendered once ([`super::actions`]).

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::working_memory::{collapse_thought, render_dispatched, WmKind, WorkingMemory};

/// What a collapsed thought keeps: its end, where the decision is.
const POINTER_KEEP: usize = 200;

pub(crate) struct Thoughts<'a> {
    pub memory: &'a WorkingMemory,
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
        let mut items = vec![Item { stored: None, key: "banner".to_owned(), full: banner.clone(), summary: None, pointer: Some(banner) }];
        items.extend(thoughts.into_iter().map(|text| {
            // Keyed by what it says: a thought, once recorded, does not change, and neither may
            // its rendering. Full is the whole thought; how much of her thoughts she reads is the
            // one allocation's call at an epoch start (collapsed to `pointer`), never a per-act
            // head here. Cut to a share of the served window, the same thought rendered 1,284
            // chars on one act and 1,445 on the next, so every act re-appended every thought
            // (Benchy on the M5, 2026-10-24 22:3xZ: ~7k chars of unchanged thoughts per act).
            Item {
                stored: None,
                key: text.clone(),
                full: ContextMessage::user(format!("- {text}")),
                summary: None,
                pointer: Some(ContextMessage::user(format!("- {}", collapse_thought(&text, POINTER_KEEP)))),
            }
        }));
        // Fresh each event, newest last; one item, so a status change appends one line set.
        items.extend(dispatched.map(|text| {
            let message = ContextMessage::user(text);
            Item { stored: None, key: "dispatched".to_owned(), full: message.clone(), summary: None, pointer: Some(message) }
        }));
        items
    }
}
