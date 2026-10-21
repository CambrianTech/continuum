//! A standing grounding block (the workspace map, the board, the wall, the roster) as a
//! source of one item: what it says now. A block that changes appends its new version;
//! itemizing a block further (one card, one open view) belongs in its own adapter, which
//! is also where a block gains pointers to collapse to.

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};

pub(crate) struct Grounding {
    pub faculty: String,
    pub text: String,
}

impl Source<ContextMessage> for Grounding {
    fn name(&self) -> &str {
        &self.faculty
    }

    fn priority(&self) -> u8 {
        priority::GROUNDING
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        // Whole, always: a block is what she works from now (her board, her checkout), and
        // it has no handle to reopen it by, so it never collapses.
        let full = ContextMessage::user(format!("[{}]\n{}", self.faculty, self.text));
        vec![Item { key: "block".to_owned(), pointer: full.clone(), full }]
    }
}
