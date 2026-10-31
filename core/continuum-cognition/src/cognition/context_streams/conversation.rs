//! Her conversation: one item per turn, keyed by when it occurred and who spoke, rendered
//! the way the prompt assembly renders a turn. A turn that leaves the room's window stays
//! in her view until the next epoch start. Collapsed at a start, an older turn leaves the view;
//! the room's history keeps it (`chat/history` rereads it).

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer, Role};

pub(crate) struct Turn {
    pub key: String,
    pub role: Role,
    pub line: String,
    /// What remains when collapsing must not drop the turn (the index of messages that
    /// reached her room during this turn). `None` for ordinary turns.
    pub pointer: Option<String>,
}

#[derive(Default)]
pub(crate) struct Conversation {
    /// Whose conversation this is (its turns' ids, `super::element`).
    pub persona: uuid::Uuid,
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
        // A room's conversation is one chain in its order: each turn follows the one before it
        // (who answered whom is not recorded yet); the first came from outside her.
        let mut previous = None;
        self.turns
            .iter()
            .map(|turn| {
                let id = element(self.persona, Producer::Turn, turn.key.as_bytes());
                let cause = previous.replace(id).map_or(Cause::Root(Origin::External), Cause::Of);
                let full = ContextMessage { role: turn.role, text: turn.line.clone() };
                let pointer = turn.pointer.clone().map(|text| ContextMessage { role: turn.role, text });
                // Collapsed, a turn leaves her view: the room's history holds it, and the
                // `[context]` fact says how many turns the room offered. A one-line pointer would
                // weigh about what a short turn does, saving nothing. The room-update index keeps
                // itself, so an update too large to show is never unknown.
                Item::whole(id, cause, turn.key.clone(), full).with_pointer(pointer)
            })
            .collect()
    }
}
