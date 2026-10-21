//! Her conversation: one item per turn, keyed by when it occurred and who spoke, rendered
//! the way the prompt assembly renders a turn. A turn that leaves the room's window stays
//! in her view until the next epoch start.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};

pub(crate) struct Turn {
    pub key: String,
    pub role: &'static str,
    pub line: String,
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
        // A turn has no handle to reopen it by, so it has no pointer: it is kept whole.
        self.turns
            .iter()
            .map(|turn| {
                let message = ContextMessage { role: turn.role, text: turn.line.clone() };
                Item { key: turn.key.clone(), full: message.clone(), pointer: message }
            })
            .collect()
    }
}
