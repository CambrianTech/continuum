//! `SeenRing`: THE bounded "have I seen this before?" set.
//!
//! Redelivery, replay and fan-out all hand the core the same id more than once, and
//! each consumer needs the same answer: first sight, or a repeat within a recent
//! window. That answer was hand-rolled again and again (a `VecDeque` plus `contains`
//! plus `pop_front` in the wake backlog, the work bridge and its card-state
//! dedupe, the conversation's per-room rings, credit settlement). One type now owns it: an ordered ring for
//! eviction and a hash set for O(1) membership, bounded by `cap`.

use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

pub struct SeenRing<T> {
    ring: VecDeque<T>,
    set: HashSet<T>,
    cap: usize,
}

impl<T: Eq + Hash + Clone> SeenRing<T> {
    /// A ring that remembers the last `cap` distinct items. `cap == 0` remembers
    /// nothing: every item reads as new.
    pub fn new(cap: usize) -> Self {
        Self { ring: VecDeque::with_capacity(cap), set: HashSet::with_capacity(cap), cap }
    }

    /// Note `item`; `true` when it is NEW (first sight), `false` on a repeat. A repeat
    /// does not refresh its place, so the window is by first sight.
    pub fn note(&mut self, item: T) -> bool {
        if self.set.contains(&item) {
            return false;
        }
        if self.cap == 0 {
            return true;
        }
        if self.ring.len() == self.cap {
            if let Some(evicted) = self.ring.pop_front() {
                self.set.remove(&evicted);
            }
        }
        self.set.insert(item.clone());
        self.ring.push_back(item);
        true
    }

    /// Whether `item` is within the window, without noting it.
    pub fn contains(&self, item: &T) -> bool {
        self.set.contains(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The index must evict with the ordered ring; retained items do not refresh
    // recency or grow the bounded set on duplicate admission.
    #[test]
    fn the_index_evicts_with_the_ring() {
        let mut seen = SeenRing::new(2);
        let (a, b, c) = (1u64, 2u64, 3u64);
        assert!(seen.note(a));
        assert!(seen.note(b));
        assert!(!seen.note(a));
        assert!(seen.note(c));
        assert!(seen.note(a));
        assert!(!seen.note(c));
        assert_eq!(seen.set.len(), 2);
        assert_eq!(seen.ring.len(), 2);
        let mut empty = SeenRing::new(0);
        assert!(empty.note(a));
        assert!(empty.note(a));
        assert!(empty.set.is_empty());
        assert!(empty.ring.is_empty());
    }
}
