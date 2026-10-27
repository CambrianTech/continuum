//! Her conversation: one item per turn, keyed by when it occurred and who spoke, rendered
//! the way the prompt assembly renders a turn. A turn that leaves the room's window stays
//! in her view until the next epoch start. Collapsed at a start, an older turn leaves the view;
//! the room's history keeps it (`chat/history` rereads it).

use std::collections::HashSet;

use continuum_context::source::{Item, Source};
use continuum_context::view::View;
use uuid::Uuid;

use super::{priority, ContextMessage, Role};

pub(crate) struct Turn {
    pub key: String,
    pub role: Role,
    pub line: String,
    /// The room event this turn is, when it came from a room.
    pub event_id: Option<Uuid>,
}

#[derive(Default)]
pub(crate) struct Conversation {
    pub turns: Vec<Turn>,
}

impl Source<ContextMessage> for Conversation {
    fn name(&self) -> &str {
        "conversation"
    }

    fn priority(&self) -> u8 {
        priority::CONVERSATION
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.turns.iter().map(Turn::item).collect()
    }
}

impl Turn {
    fn item(&self) -> Item<ContextMessage> {
        Item {
            key: self.key.clone(),
            full: ContextMessage { role: self.role, text: self.line.clone() },
            // Collapsed, a turn leaves her view: the room's history holds it, and the
            // `[context]` fact says how many turns the room offered. A one-line pointer would
            // weigh about what a short turn does, saving nothing.
            summary: None,
            pointer: None,
        }
    }
}

/// Her conversation less the turns her view already holds as a room update. A message that
/// arrived during her drive is in the view under `room-updates`; on her next act the same
/// message is a turn of the room's transcript, and the view keys each source apart, so
/// without this it would append twice (Cormac on #5526).
pub(crate) struct Unheld<'a> {
    conversation: &'a Conversation,
    held: HashSet<Uuid>,
}

impl<'a> Unheld<'a> {
    pub(crate) fn of(conversation: &'a Conversation, view: Option<&View<ContextMessage>>) -> Self {
        let held = view.map_or_else(HashSet::new, |view| { // map_or_else: no view yet holds nothing
            conversation
                .turns
                .iter()
                .filter_map(|turn| turn.event_id)
                .filter(|event_id| view.holds(super::room_updates::NAME, &event_id.to_string()))
                .collect()
        });
        Self { conversation, held }
    }
}

impl Source<ContextMessage> for Unheld<'_> {
    fn name(&self) -> &str {
        self.conversation.name()
    }

    fn priority(&self) -> u8 {
        self.conversation.priority()
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.conversation
            .turns
            .iter()
            .filter(|turn| turn.event_id.map_or(true, |event_id| !self.held.contains(&event_id))) // map_or: a turn with no event cannot be a held update
            .map(Turn::item)
            .collect()
    }
}
