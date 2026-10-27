//! What arrived in her rooms while she was acting, one item per event. Before this the whole
//! list rode after her view and moved below each new act, so every act re-sent all of it: Aris
//! on the M5 (2026-10-27 11:3xZ) re-prefilled 59 room messages, ~30k chars and 12-13k tokens,
//! on every act of a 56-act drive while each act added ~1k. Keyed by event, each arrives once,
//! where it arrived, and the acts after it append behind it.

use std::collections::HashSet;
use std::sync::Arc;

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::conversation::IncomingMessage;

/// The source's name, which keys its items in her view.
pub(crate) const NAME: &str = "room-updates";

/// The burst's own list, shared by reference count: building the source copies nothing.
#[derive(Default)]
pub(crate) struct RoomUpdates {
    pub updates: Arc<Vec<Arc<IncomingMessage>>>,
    /// Events her conversation already offers as turns: the turn carries the message, so the
    /// update is not offered beside it (one message, one place in her view).
    pub in_transcript: HashSet<uuid::Uuid>,
}

impl Source<ContextMessage> for RoomUpdates {
    fn name(&self) -> &str {
        NAME
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
            .filter(|update| !self.in_transcript.contains(&update.event_id))
            .map(|update| Item {
                key: update.event_id.to_string(),
                full: ContextMessage::user(update.render_room_update()),
                summary: None,
                // Collapsed (an older one at a start, or one larger than her whole window on
                // arrival), it still says who spoke, where, how much, and how to reread it: a
                // message she cannot read whole arrives as its handle, never as a refusal.
                pointer: Some(ContextMessage::user(format!(
                    "[Room message received during this turn; room {}; peer {}; event {}; {} chars, collapsed: chat/history rereads it]",
                    update.room_id,
                    update.peer_id,
                    update.event_id,
                    update.render_content().chars().count()
                ))),
            })
            .collect()
    }
}
