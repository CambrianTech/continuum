//! The files and outputs she has open and which lines of each she has read, one item per view.
//! Before this they were one required block after her view, re-rendered and re-prefilled on
//! every act (~1.5k chars on the M5, 2026-10-24) though a view's line changes only when she
//! pages it. Keyed by subject, paging one file appends that file's line alone.

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::open_views::OpenViews as Views;
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct OpenViews<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose occurrences these are (`super::element`).
    pub persona: uuid::Uuid,
}

/// Standing notices contain no allocatable detail.
impl OpenViews<'_> {
    fn standing(&self, key: &str, text: String) -> Item<ContextMessage> {
        super::standing(self.persona, "open-views", key, ContextMessage::user(text))
    }
}

impl Source<ContextMessage> for OpenViews<'_> {
    fn name(&self) -> &str {
        "open-views"
    }

    fn priority(&self) -> u8 {
        priority::ACTIONS
    }

    fn required(&self) -> bool {
        true
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let lines = self.memory.open_view_lines();
        if lines.is_empty() {
            return Vec::new();
        }
        let mut items = vec![self.standing("banner", Views::BANNER.to_owned())];
        // A view follows from the act that last read it.
        items.extend(lines.into_iter().map(|line| {
            let id = element(self.persona, Producer::OpenView, format!("{}#{}", line.key, line.touched_seq).as_bytes());
            let cause = Cause::Of(element(self.persona, Producer::Act, &line.touched_seq.to_be_bytes()));
            Item::whole(id, cause, line.key, ContextMessage::user(format!("- {}", line.full)))
                .with_pointer(ContextMessage::user(format!("- {}", line.pointer)))
        }));
        items
    }

    /// A view in a checkout her hands left reads as not visible now, never as open where she
    /// stands; it is offered again when they return.
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        self.memory
            .open_views_out_of_scope()
            .into_iter()
            .map(|key| {
                let id = element(self.persona, Producer::Notice, key.as_bytes());
                let message = ContextMessage::user(format!("- [not visible from here] {key}"));
                Item::whole(id, Cause::Root(Origin::External), key, message.clone()).with_pointer(message)
            })
            .collect()
    }
}
