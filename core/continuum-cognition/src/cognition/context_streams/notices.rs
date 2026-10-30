//! What her substrate posted about her situation (a handoff, a rebuild, where her hands are
//! rooted), one item each. Before this they were one block re-rendered after her view on every
//! act: ~3.6k chars of mostly unchanged notices re-prefilled per act (Benchy and Aiko on the
//! M5, 2026-10-24: the largest new text per act, ahead of her own results). Keyed, an unchanged
//! notice is appended once per epoch and a changed one appends its new version.

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::framing_echo::NOTICES_HEADER;
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct Notices<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose occurrences these are (`super::element`).
    pub persona: uuid::Uuid,
}

/// A notice is required input: collapsed, it is still itself, so no grant can drop it.
impl Notices<'_> {
    /// A notice comes from her situation, outside her: an external origin.
    fn notice(&self, key: String, text: String) -> Item<ContextMessage> {
        let message = ContextMessage::user(text);
        Item::whole(element(self.persona, Producer::Notice, key.as_bytes()), Cause::Root(Origin::External), key, message.clone()).with_pointer(message)
    }
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
        let mut items = vec![super::standing(self.persona, "notices", "banner", ContextMessage::user(banner))];
        items.extend(notices.into_iter().map(|(key, text)| self.notice(key, format!("- {text}"))));
        items
    }

    /// A condition that ended (her hands came home, a pin ran out) leaves her view now, so she
    /// never reads it as still holding.
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        self.memory
            .ended_notice_keys()
            .into_iter()
            .map(|key| {
                // The end of a notice follows from that notice.
                let message = ContextMessage::user(format!("- [ended] the notice {key} no longer holds"));
                let id = element(self.persona, Producer::Notice, format!("ended/{key}").as_bytes());
                let cause = Cause::Of(element(self.persona, Producer::Notice, key.as_bytes()));
                Item::whole(id, cause, key, message.clone()).with_pointer(message)
            })
            .collect()
    }
}
