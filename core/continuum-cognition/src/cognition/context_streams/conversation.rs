//! Her conversation: one item per turn, keyed by when it occurred and who spoke, rendered
//! the way the prompt assembly renders a turn. A turn that leaves the room's window stays
//! in her view until the next epoch start. Collapsed at a start, an older turn leaves the view;
//! the room's history keeps it (`chat/history` rereads it).

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer, Role};

/// The source's name: its claim is `view/conversation`.
pub(crate) const NAME: &str = "conversation";

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
    /// Every turn but the act's ask (the newest external speech), which her act's tail carries
    /// (`tail::Tail`) under the same element, so once it is held there it never arrives here again.
    pub turns: Vec<Turn>,
    /// The key of the turn that arrived during this act (the newest room message that reached her
    /// while she acted): her newest interaction, which the act's allocation holds whole when the
    /// window can ([`continuum_context::allocation::ViewClaim::arrived`]).
    pub arrived: Option<String>,
}

impl Conversation {
    /// The turn that arrived during this act, as her view renders it whole.
    pub(crate) fn arrived(&self) -> Option<ContextMessage> {
        let key = self.arrived.as_deref()?;
        self.turns.iter().find(|turn| turn.key == key).map(|turn| ContextMessage::new(turn.role, turn.line.clone()))
    }
}

impl Source<ContextMessage> for Conversation {
    fn name(&self) -> &str {
        NAME
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
                let full = ContextMessage::new(turn.role, turn.line.clone());
                let pointer = turn.pointer.clone().map(|text| ContextMessage::new(turn.role, text));
                // Collapsed, a turn leaves her view: the room's history holds it, and the
                // `[context]` fact says how many turns the room offered. A one-line pointer would
                // weigh about what a short turn does, saving nothing. The room-update index keeps
                // itself, so an update too large to show is never unknown.
                Item::whole(id, cause, turn.key.clone(), full).with_pointer(pointer)
            })
            .collect()
    }

    /// No fold: an ordinary turn has no pointer and leaves her view when collapsed (the room's
    /// history keeps it, `chat/history`); only a room-update index keeps itself, so the run's
    /// pointers are only those indexes.
    fn fold(&self, _folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        None
    }
}
