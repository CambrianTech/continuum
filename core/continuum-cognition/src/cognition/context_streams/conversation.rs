//! Her conversation: one item per turn, keyed by when it occurred and who spoke, rendered
//! the way the prompt assembly renders a turn. A turn that leaves the room's window stays
//! in her view until the next epoch start. Collapsed at a start, an older turn leaves the view;
//! the room's history keeps it (`chat/history` rereads it).

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage, Role};

pub(crate) struct Turn {
    pub key: String,
    pub role: Role,
    pub line: String,
    /// A handle when collapsing must not drop the turn: a message that reached her room
    /// during this turn and that she has not read yet. `None` for the burst's turns.
    pub pointer: Option<String>,
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
        self.turns
            .iter()
            .map(|turn| Item {
                stored: None,
                key: turn.key.clone(),
                full: ContextMessage { role: turn.role, text: turn.line.clone() },
                // Collapsed, a turn leaves her view: the room's history holds it, and the
                // `[context]` fact says how many turns the room offered. A one-line pointer would
                // weigh about what a short turn does, saving nothing. An unread room update keeps
                // its handle, so one larger than her window is still hers to open.
                summary: None,
                pointer: turn.pointer.clone().map(|text| ContextMessage { role: turn.role, text }),
            })
            .collect()
    }
}
