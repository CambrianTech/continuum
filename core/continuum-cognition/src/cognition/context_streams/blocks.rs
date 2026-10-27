//! A standing grounding block (the workspace map, the board, the wall, the roster) as a
//! source of one item: what it says now about the room it describes. A block that changes
//! appends its new version; itemizing a block further (one card, one open view) belongs in
//! its own adapter, which is also where a block gains pointers to collapse to.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};

pub(crate) struct Grounding {
    pub faculty: String,
    /// The room the block describes (`Contribution::room`), `None` for one that is hers in
    /// every room. It names the item, so each room's block is one item of her view.
    pub room: Option<uuid::Uuid>,
    /// The whole block.
    pub text: String,
    /// What it guarantees when the window is short: a long list's first units with a line
    /// naming what is omitted; an atomic block whole (then the same as `text`).
    pub minimum: String,
}

impl Grounding {
    /// The block's header: its faculty, and the room it describes, so two rooms' blocks in
    /// one view never read as one (the newest copy is no longer the current room's).
    fn header(&self) -> String {
        match self.room {
            Some(room) => format!("[{}] (room {room})", self.faculty),
            None => format!("[{}]", self.faculty),
        }
    }
}

impl Source<ContextMessage> for Grounding {
    fn name(&self) -> &str {
        &self.faculty
    }

    fn priority(&self) -> u8 {
        priority::GROUNDING
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        // A block is what she works from now (her board, her checkout) with no handle to
        // reopen it by, so collapsed it is still its truthful minimum, never a one-liner.
        let header = self.header();
        let full = ContextMessage::user(format!("{header}\n{}", self.text));
        let pointer = ContextMessage::user(format!("{header}\n{}", self.minimum));
        // Keyed by the room it describes: the same subject is the same item. Keyed by the
        // faculty alone, a switch between two rooms superseded each room's block with the
        // other's, and every switch back re-appended a block that had not changed.
        let key = self.room.map_or_else(|| "block".to_owned(), |room| format!("room/{room}")); // map_or_else: a block that is hers in every room is one item
        vec![Item { key, full, summary: None, pointer: Some(pointer) }]
    }
}
