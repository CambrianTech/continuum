//! What was said in one room, as one turn reads it: her thread of the room at the revision the turn
//! pinned, and which of its lines are hers. The repetition evidence her perception facts and the
//! burst's anchor escalation read (#148, #264, #301).
//!
//! A view over her [`RoomThread`], never a copy: the thread holds the lines, this names hers. It
//! reaches past the rendered window on purpose: live budgets squeezed the window to 2-6 turns
//! (#259), and with the older copy of every restatement scrolled out the facts were structurally
//! blind (0 fires across a live chorus, 2026-07-30). It is per ROOM because the thread is: what she
//! said in #academy told her she was circling on her first turn in a fresh room (2026-08-14). It
//! rides the turn's `Burst`, so the facts, the anchor and her `world_state` read one revision, and
//! an eval fork, which reads no room, has an empty one.

use std::sync::Arc;

use continuum_grounding::room_thread::{Item, RoomThread};

/// One room's lines and hers among them, at one revision of her thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    thread: Arc<RoomThread>,
    me: uuid::Uuid,
}

impl Said {
    /// What `me` reads of `thread`.
    pub fn of(thread: Arc<RoomThread>, me: uuid::Uuid) -> Self {
        Self { thread, me }
    }

    /// The revision this turn read: its capture and admission name it.
    pub fn thread(&self) -> &RoomThread {
        &self.thread
    }

    /// The room's lines with the events they are, oldest first.
    pub fn lines(&self) -> &[Item] {
        self.thread.items()
    }

    /// Her own recent utterances in this room, oldest first (empty where she has said nothing,
    /// which is the right reading for a room she just entered).
    pub fn own(&self) -> Vec<&str> {
        self.lines().iter().filter(|item| item.sender == self.me).map(|item| item.text.as_str()).collect()
    }

    /// The room's recent lines, hers included (the echo counters skip exactly one copy of
    /// hers), oldest first.
    pub fn room(&self) -> Vec<&str> {
        self.lines().iter().map(|item| item.text.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: her lines missing from the room's side (the echo counters skip exactly
    // one copy of hers), or a peer's line counted as hers.
    #[test]
    fn the_room_holds_every_line_and_hers_are_named_among_them() {
        let (me, peer, room) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2), uuid::Uuid::from_u128(9));
        let said_by = |n: u128, from: uuid::Uuid, text: &str| continuum_grounding::room_thread::said(room, 10 + n, n as u64, from, text);
        let thread = RoomThread::new(room).with(
            [said_by(1, peer, "are the wordstats tests green yet"), said_by(2, me, "the wordstats tests are green"), said_by(3, peer, "thanks, merging")],
            32,
        );
        let said = Said::of(Arc::new(thread.unwrap_or_default()), me);
        assert_eq!(said.room(), ["are the wordstats tests green yet", "the wordstats tests are green", "thanks, merging"]);
        assert_eq!(said.own(), ["the wordstats tests are green"]);
    }
}
