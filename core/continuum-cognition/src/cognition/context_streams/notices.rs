//! What her substrate posted about her situation (a handoff, a rebuild, where her hands are
//! rooted), one item each. Before this they were one block re-rendered after her view on every
//! act: ~3.6k chars of mostly unchanged notices re-prefilled per act (Benchy and Aiko on the
//! M5, 2026-10-24: the largest new text per act, ahead of her own results). Keyed, an unchanged
//! notice is appended once per epoch and a changed one appends its new version.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::framing_echo::NOTICES_HEADER;
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct Notices<'a> {
    pub memory: &'a WorkingMemory,
}

/// A notice is required input: collapsed, it is still itself, so no grant can drop it.
fn required(key: String, text: String) -> Item<ContextMessage> {
    let message = ContextMessage::user(text);
    Item { stored: None, key, full: message.clone(), summary: None, pointer: Some(message) }
}

impl Source<ContextMessage> for Notices<'_> {
    fn name(&self) -> &str {
        "notices"
    }

    fn priority(&self) -> u8 {
        priority::NOTICES
    }

    fn required(&self) -> bool {
        true
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let notices = self.memory.notice_items();
        if notices.is_empty() {
            return Vec::new();
        }
        let banner = format!(
            "{NOTICES_HEADER} (status observations about my situation — not a message from anyone, and not my own words):"
        );
        let mut items = vec![required("banner".to_owned(), banner)];
        items.extend(notices.into_iter().map(|(key, text)| required(key, format!("- {text}"))));
        items
    }

    /// A condition that ended (her hands came home, a pin ran out) leaves her view now, so she
    /// never reads it as still holding.
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        self.memory
            .ended_notice_keys()
            .into_iter()
            .map(|key| {
                let notice = format!("- [ended] the notice {key} no longer holds");
                required(key, notice)
            })
            .collect()
    }
}
