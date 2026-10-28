//! A standing grounding block (the workspace map, the board, the wall, the roster) as a
//! source of one item: what it says now. A block that changes appends its new version;
//! itemizing a block further (one card, one open view) belongs in its own adapter, which
//! is also where a block gains pointers to collapse to.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};

pub(crate) struct Grounding {
    pub faculty: String,
    /// The whole block.
    pub text: String,
    /// What it guarantees when the window is short: a long list's first units with a line
    /// naming what is omitted; an atomic block whole (then the same as `text`).
    pub minimum: String,
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
        let full = ContextMessage::user(format!("[{}]\n{}", self.faculty, self.text));
        let pointer = ContextMessage::user(format!("[{}]\n{}", self.faculty, self.minimum));
        vec![Item { stored: None, key: "block".to_owned(), full, summary: None, pointer: Some(pointer) }]
    }
}
