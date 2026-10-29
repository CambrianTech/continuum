//! The files and outputs she has open and which lines of each she has read, one item per view.
//! Before this they were one required block after her view, re-rendered and re-prefilled on
//! every act (~1.5k chars on the M5, 2026-10-24) though a view's line changes only when she
//! pages it. Keyed by subject, paging one file appends that file's line alone.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::open_views::OpenViews as Views;
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct OpenViews<'a> {
    pub memory: &'a WorkingMemory,
}

/// Required, as the block was: the history fit dropping it made her re-read from line 1 every
/// turn (Benchy, the M5, 2026-10-17), so collapsed it is still itself.
fn required(key: String, text: String) -> Item<ContextMessage> {
    let message = ContextMessage::user(text);
    Item { key, full: message.clone(), summary: None, pointer: Some(message), stored: None }
}

impl Source<ContextMessage> for OpenViews<'_> {
    fn name(&self) -> &str {
        "open-views"
    }

    fn priority(&self) -> u8 {
        priority::NOTICES
    }

    fn required(&self) -> bool {
        true
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let lines = self.memory.open_view_lines();
        if lines.is_empty() {
            return Vec::new();
        }
        let mut items = vec![required("banner".to_owned(), Views::BANNER.to_owned())];
        items.extend(lines.into_iter().map(|(key, line)| required(key, format!("- {line}"))));
        items
    }

    /// A view in a checkout her hands left reads as not visible now, never as open where she
    /// stands; it is offered again when they return.
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        self.memory
            .open_views_out_of_scope()
            .into_iter()
            .map(|key| {
                let notice = format!("- [not visible from here] {key}");
                required(key, notice)
            })
            .collect()
    }
}
