//! Replay and redelivery dedupe at the loop head: the event ids a drive has already
//! perceived, and the one staleness rule a drained line is judged by.

use std::collections::{HashSet, VecDeque};

use uuid::Uuid;

use super::IncomingMessage;

/// Event-id ring for replay/redelivery dedupe at the loop head. Small: the
/// pump already dedupes the store catch-up; this catches re-open replays.
pub struct SeenIds {
    ring: VecDeque<Uuid>,
    ids: HashSet<Uuid>,
    cap: usize,
}

impl SeenIds {
    pub fn new(cap: usize) -> Self {
        Self {
            ring: VecDeque::with_capacity(cap),
            ids: HashSet::with_capacity(cap),
            cap,
        }
    }

    /// Note an id; `true` when it is NEW (first sight), `false` on a repeat.
    pub fn note(&mut self, id: Uuid) -> bool {
        if self.ids.contains(&id) {
            return false;
        }
        if self.cap == 0 {
            return true;
        }
        if self.ring.len() == self.cap {
            if let Some(evicted) = self.ring.pop_front() {
                self.ids.remove(&evicted);
            }
        }
        self.ids.insert(id);
        self.ring.push_back(id);
        true
    }
}

/// Is this drained line stale? By event id when the source stamps one; by the
/// publisher clock only for id-less sources (scripted/test conversations).
pub fn is_stale(m: &IncomingMessage, seen: &mut SeenIds, high_water: u64) -> bool {
    if m.event_id.is_nil() {
        m.lamport <= high_water
    } else {
        !seen.note(m.event_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(peer: u8, lamport: u64, text: &str) -> IncomingMessage {
        IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport,
            peer_id: Uuid::from_u128(peer as u128),
            room_id: Uuid::from_u128(7),
            text: text.to_string(),
        }
    }

    // what this catches: the per-publisher lamport read a human line as stale
    // against citizens' larger clocks (the 9/4 deaf-citizens regression).
    #[test]
    fn a_human_line_with_a_small_publisher_clock_is_not_stale() {
        let mut seen = SeenIds::new(8);
        let human = line(1, 3, "Joel here — which card do you hold?");
        assert!(!is_stale(&human, &mut seen, 157_000));
    }

    // what this catches: a re-open replay redelivering the same event id.
    #[test]
    fn a_replayed_event_id_is_stale_the_second_time() {
        let mut seen = SeenIds::new(8);
        let m = line(1, 3, "once");
        assert!(!is_stale(&m, &mut seen, 0));
        assert!(is_stale(&m, &mut seen, 0));
    }

    // The index must evict with the ordered ring; retained snapshots do not
    // refresh recency or grow the bounded set on duplicate admission.
    #[test]
    fn seen_id_index_evicts_with_the_ring() {
        let mut seen = SeenIds::new(2);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        assert!(seen.note(a));
        assert!(seen.note(b));
        assert!(!seen.note(a));
        assert!(seen.note(c));
        assert!(seen.note(a));
        assert!(!seen.note(c));
        assert_eq!(seen.ids.len(), 2);
        assert_eq!(seen.ring.len(), 2);
        let mut empty = SeenIds::new(0);
        assert!(empty.note(a));
        assert!(empty.note(a));
        assert!(empty.ids.is_empty());
        assert!(empty.ring.is_empty());
    }
}
