//! What arrived in her rooms while she was acting, one item per event. Before this the whole
//! list rode after her view and moved below each new act, so every act re-sent all of it: Aris
//! on the M5 (2026-10-27 11:3xZ) re-prefilled 59 room messages, ~30k chars and 12-13k tokens,
//! on every act of a 56-act drive while each act added ~1k. Keyed by event, each arrives once,
//! where it arrived, and the acts after it append behind it.

use std::sync::Arc;

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::conversation::IncomingMessage;

/// The burst's own list, shared by reference count: building the source copies nothing.
#[derive(Default)]
pub(crate) struct RoomUpdates {
    pub updates: Arc<Vec<Arc<IncomingMessage>>>,
}

impl Source<ContextMessage> for RoomUpdates {
    fn name(&self) -> &str {
        "room-updates"
    }

    /// Room input, weighed like her conversation and never required: a busy room must not
    /// claim the window as irreducible payload (BigMama, 2026-09-09: 53k tokens of required
    /// room input on a 23k window refused 84% of cognition cycles, two citizens mute for hours).
    fn priority(&self) -> u8 {
        priority::CONVERSATION
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.updates
            .iter()
            .map(|update| Item {
                key: update.event_id.to_string(),
                full: ContextMessage::user(update.render_room_update()),
                // Collapsed at a start, an older one leaves her view as a conversation turn
                // does: the room keeps it, `chat/history` rereads it, and the newest never goes.
                summary: None,
                pointer: None,
            })
            .collect()
    }
}
