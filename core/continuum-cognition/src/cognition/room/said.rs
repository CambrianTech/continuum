//! What was said in one room: the room's newest lines and her own newest utterances there, oldest
//! first, read from the airc room itself. The repetition evidence her perception facts and the
//! burst's anchor escalation read (#148, #264, #301).
//!
//! A room is an airc room, so this is a projection over a page of its transcript, never a copy
//! continuum keeps: no ring, no hydration, no checkpoint field (all three existed and are gone).
//! It reaches past the rendered window on purpose: live budgets squeezed the window to 2-6 turns
//! (#259), and with the older copy of every restatement scrolled out the facts were structurally
//! blind (0 fires across a live chorus, 2026-07-30). It is per ROOM because the page is: what she
//! said in #academy told her she was circling on her first turn in a fresh room (2026-08-14). It
//! rides the turn's `Burst`, so the facts, the anchor and her `world_state` read one snapshot,
//! and an eval fork, which reads no room, has an empty one.

/// One line of the room: the airc event it is, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub event: uuid::Uuid,
    pub text: String,
}

/// A room's recent lines and hers among them, oldest first: exactly one page of the airc room,
/// each line kept once with the room event it is, so a capture can name which events a served
/// request was built from (Cormac on #6241).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    lines: Vec<Line>,
    /// Which of `lines` she sent.
    hers: Vec<usize>,
}

impl Said {
    /// How many of the room's newest conversation events a turn reads: in a room of four
    /// residents about her own last eight, the self-history the repetition cluster of three
    /// needs with slack, and twice the 16-line chorus window the old ring held. An estimate
    /// from those two needs, not a measurement; the codec should size it from the model's
    /// grant (Codex on #6241).
    pub const PAGE: usize = 32;

    /// The window over one page of the room's airc transcript (any order; read by lamport):
    /// every conversation line, and which of them `me` sent.
    pub fn of_room(page: &[airc_core::TranscriptEvent], me: uuid::Uuid) -> Self {
        let mut events: Vec<&airc_core::TranscriptEvent> = page.iter().collect();
        events.sort_by_key(|event| event.lamport);
        let mut said = Self::default();
        for event in events {
            let Ok(line) = crate::cognition::conversation::room_event::room_content_from_event(event) else { continue };
            if line.text.trim().is_empty() {
                continue;
            }
            if line.sender == me {
                said.hers.push(said.lines.len());
            }
            said.lines.push(Line { event: event.event_id.as_uuid(), text: line.text });
        }
        said
    }

    /// The room's lines with the events they are, oldest first.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Her own recent utterances in this room, oldest first (empty where she has said nothing,
    /// which is the right reading for a room she just entered).
    pub fn own(&self) -> Vec<&str> {
        self.hers.iter().filter_map(|at| self.lines.get(*at)).map(|line| line.text.as_str()).collect()
    }

    /// The room's recent lines, hers included (the echo counters skip exactly one copy of
    /// hers), oldest first.
    pub fn room(&self) -> Vec<&str> {
        self.lines.iter().map(|line| line.text.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(room: airc_core::RoomId, from: uuid::Uuid, text: &str, lamport: u64) -> airc_core::TranscriptEvent {
        airc_core::TranscriptEvent {
            event_id: airc_core::EventId::new(),
            room_id: room,
            peer_id: airc_core::PeerId::from_uuid(from),
            client_id: airc_core::ClientId::new(),
            kind: airc_core::TranscriptKind::Message,
            occurred_at_ms: 1_000 + lamport,
            lamport,
            target: airc_core::MentionTarget::All,
            headers: Default::default(),
            body: Some(airc_core::Body::text(text)),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: the window read in the page's order instead of the room's (a page can
    // arrive newest-first), a blank line counted as evidence, her lines missing from the room's
    // side (the echo counters skip exactly one copy of hers), or a peer's line counted as hers.
    #[test]
    fn the_window_is_the_page_in_room_order_and_hers_among_it() {
        let room = airc_core::RoomId::new();
        let (me, peer) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
        let mut page = vec![
            line(room, peer, "are the wordstats tests green yet", 1),
            line(room, peer, "   ", 2),
            line(room, me, "the wordstats tests are green", 3),
            line(room, peer, "thanks, merging", 4),
        ];
        page.reverse();
        let said = Said::of_room(&page, me);
        assert_eq!(said.room(), ["are the wordstats tests green yet", "the wordstats tests are green", "thanks, merging"]);
        assert_eq!(said.own(), ["the wordstats tests are green"]);
        // The page was reversed: in room order the lines are lamport 1, 3, 4 (page[3], [1], [0]).
        let events: Vec<uuid::Uuid> = said.lines().iter().map(|line| line.event).collect();
        assert_eq!(events, [3, 1, 0].map(|at| page[at].event_id.as_uuid()), "each line keeps the room event it is");
    }
}
