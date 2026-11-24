//! Her thread of one room: the room's newest lines in room order, as one immutable revision.
//!
//! A room is an airc room, and the airc room stays the one record: a thread lives in her memory
//! only, is never persisted or hydrated, and is seeded from one airc page when she joins the room.
//! Each line holds the airc event it is by handle, never a copy of its fields. Each revision is a
//! new value shared by handle, never edited in place, so a turn holds the one revision it read for
//! its grounding, its request, its capture and its admission. Each line keeps the revision that
//! applied it, so the thread as of any event it still holds is the lines applied at or before that
//! event's.
//!
//! It lives beside [`RagContext`](crate::rag_budget::RagContext) because a turn hands its pinned
//! revision to every source through it: what was said is data the turn carries down, never a read
//! a source makes of another part.

use std::sync::Arc;

use airc_core::TranscriptEvent;
use uuid::Uuid;

/// One line of the room.
#[derive(Debug, Clone)]
pub struct Item {
    /// The airc event it is: the one record, shared.
    pub record: Arc<TranscriptEvent>,
    pub sender: Uuid,
    pub text: String,
    /// The revision of the thread that applied it.
    pub applied: u64,
}

impl Item {
    /// The airc event's id.
    pub fn event(&self) -> Uuid {
        self.record.event_id.as_uuid()
    }
}

impl PartialEq for Item {
    /// The same line: the same airc event (its id names the one record), applied by the same
    /// revision.
    fn eq(&self, other: &Self) -> bool {
        self.event() == other.event() && self.applied == other.applied
    }
}

impl Eq for Item {}

/// Whether the room's history before she heard it live has been read in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Seed {
    /// Not yet: the thread holds only what she heard live.
    #[default]
    Pending,
    /// One airc page of the room is merged in.
    Read,
    /// The page did not read; the thread holds only what she heard live, and says why.
    Unread(Unread),
}

/// Why a room's page did not read.
#[derive(Debug, Clone)]
pub enum Unread {
    /// She has no airc runtime on this core to read the room through.
    NoRuntime,
    /// airc refused the read.
    Airc(Arc<airc_lib::AircError>),
}

impl PartialEq for Unread {
    /// The same refusal, by identity: two reads that failed alike are still two refusals.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Unread::NoRuntime, Unread::NoRuntime) => true,
            (Unread::Airc(a), Unread::Airc(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for Unread {}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::NoRuntime => f.write_str("she has no airc runtime on this core to read the room through"),
            Unread::Airc(error) => write!(f, "airc refused the room's page: {error}"),
        }
    }
}

/// One revision of her thread of a room.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoomThread {
    pub room: Uuid,
    /// How many changes made this thread; 0 before any.
    pub revision: u64,
    pub seed: Seed,
    /// She missed room events since the seed (her feed window overran): the thread may lack lines
    /// until the room is read again.
    pub gapped: bool,
    items: Vec<Item>,
}

/// A room line as a thread takes it: the airc event, who said it and what.
#[derive(Debug, Clone)]
pub struct Line {
    pub record: Arc<TranscriptEvent>,
    pub sender: Uuid,
    pub text: String,
}

impl Line {
    fn event(&self) -> Uuid {
        self.record.event_id.as_uuid()
    }
}

impl RoomThread {
    /// A room's thread before anything was heard in it.
    pub fn new(room: Uuid) -> Self {
        Self { room, ..Self::default() }
    }

    /// The lines it holds, oldest first in room order.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Whether it still holds `event`: false once that many newer lines pushed it out, or before
    /// the event was heard.
    pub fn holds(&self, event: Uuid) -> bool {
        self.items.iter().any(|item| item.event() == event)
    }

    /// The line that is `event`, while the thread still holds it.
    pub fn item(&self, event: Uuid) -> Option<&Item> {
        self.items.iter().find(|item| item.event() == event)
    }

    /// The next revision with `lines` taken in: each one new by its event id, in room order, the
    /// oldest dropped past what her context `needs`. `None` when nothing in `lines` is new.
    pub fn with(&self, lines: impl IntoIterator<Item = Line>, needs: usize) -> Option<Self> {
        let applied = self.revision + 1;
        let mut items = self.items.clone();
        let before = items.len();
        for line in lines {
            if items.iter().any(|item| item.event() == line.event()) {
                continue;
            }
            items.push(Item { record: line.record, sender: line.sender, text: line.text, applied });
        }
        if items.len() == before {
            return None;
        }
        // Room order; an equal lamport (two peers' clocks) is ordered by event id, so every
        // resident orders the same room the same way.
        items.sort_by_key(|item| (item.record.lamport, item.event()));
        let surplus = items.len().saturating_sub(needs);
        items.drain(..surplus);
        Some(Self { room: self.room, revision: applied, seed: self.seed.clone(), gapped: self.gapped, items })
    }

    /// The next revision after a read of the room's page: a page that read closes any gap; one that
    /// did not leaves the gap as it was.
    pub fn seeded(&self, seed: Seed) -> Self {
        let gapped = self.gapped && seed != Seed::Read;
        Self { room: self.room, revision: self.revision + 1, seed, gapped, items: self.items.clone() }
    }

    /// The next revision after she missed room events: it may lack lines until the room is read again.
    pub fn gapped(&self) -> Self {
        Self { room: self.room, revision: self.revision + 1, seed: self.seed.clone(), gapped: true, items: self.items.clone() }
    }
}

/// A test line: event `n` at `lamport` from `sender`, saying `text` (shared by every crate's tests
/// that build a thread, so none hand-rolls an airc event).
#[cfg(any(test, feature = "test-fixtures"))]
pub fn said(room: Uuid, n: u128, lamport: u64, sender: Uuid, text: &str) -> Line {
    use airc_core::{Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind};
    let room = RoomId::from_uuid(room);
    let record = TranscriptEvent {
        event_id: EventId::from_uuid(Uuid::from_u128(n)),
        room_id: room,
        peer_id: PeerId::from_uuid(sender),
        client_id: ClientId::new(),
        kind: TranscriptKind::Message,
        occurred_at_ms: 1_000_000 + lamport,
        lamport,
        target: MentionTarget::Room(room),
        headers: Headers::default(),
        body: Some(Body::text(text)),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    };
    Line { record: Arc::new(record), sender, text: text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: u128, lamport: u64, sender: u128) -> Line {
        said(Uuid::from_u128(9), n, lamport, Uuid::from_u128(sender), &format!("line {n}"))
    }

    // what this catches: a line taken twice when the seed page and the live feed both carry it, a
    // thread read in arrival order instead of room order, and a revision that is not new when
    // nothing changed (a republish that would wake every reader for nothing).
    #[test]
    fn a_thread_takes_each_event_once_in_room_order() {
        let thread = RoomThread::new(Uuid::from_u128(9));
        let live = thread.with([line(2, 20, 1)], 32).unwrap();
        let seeded = live.with([line(1, 10, 2), line(2, 20, 1)], 32).unwrap();
        let events: Vec<Uuid> = seeded.items().iter().map(Item::event).collect();
        assert_eq!(events, [Uuid::from_u128(1), Uuid::from_u128(2)]);
        assert_eq!((seeded.items()[0].applied, seeded.items()[1].applied), (2, 1), "each line keeps the revision that applied it");
        assert!(seeded.with([line(1, 10, 2)], 32).is_none(), "nothing new is no revision");
    }

    // what this catches: Codex on the thread design: a backlog of newer arrivals before her turn is
    // built pushes the line that woke her out of the window even with no feed gap, so a turn must
    // ask whether its trigger is still held rather than assume it.
    #[test]
    fn thirty_three_newer_lines_push_the_trigger_out_of_a_thread_of_thirty_two() {
        let mut thread = RoomThread::new(Uuid::from_u128(9)).with([line(100, 1, 1)], 32).unwrap();
        assert!(thread.holds(Uuid::from_u128(100)));
        for n in 0..33 {
            thread = thread.with([line(n, 2 + n as u64, 2)], 32).unwrap();
        }
        assert!(!thread.holds(Uuid::from_u128(100)), "the trigger is outside the window");
        assert_eq!(thread.items().len(), 32);
        assert_eq!(thread.revision, 34);
    }
}
