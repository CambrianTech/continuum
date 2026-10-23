//! A per-room diagnostics ledger that cannot grow without bound.
//!
//! ## Why this exists (#3903 review, @Astra)
//!
//! Two diagnostics in the grounding path key their state BY ROOM: the doctrine
//! source's last observed outcome, and the `RagSourceFaculty`'s absence streak.
//! Keying them by room was itself a review fix, since a single cell let room A's
//! state answer for room B. But a plain `HashMap<Uuid, _>` traded one defect for
//! a smaller one: it retains every room the persona has EVER taken a turn in, and
//! per-task rooms plus arbitrary stamped contexts make that lifetime history
//! strictly larger than current subscriptions. Diagnostics must not outlive their
//! subject, and a map that only grows is a leak with a slow fuse.
//!
//! ## The eviction decision, stated rather than implied
//!
//! Per CLAUDE.md, a structure that accumulates gets an explicit eviction story or
//! it does not ship. This one: bounded at the `cap` given to
//! [`BoundedRoomLedger::new`], evicting the least-recently-WRITTEN room.
//! Recency-by-write is the right axis because every observation is a write. A
//! room being actively answered in is a room being written to, and the room not
//! written to longest is by construction the one whose diagnostics matter least.
//!
//! EVICTION IS NOT A STATE CHANGE AND MUST NEVER BE REPORTED AS ONE. An evicted
//! room's next observation looks like a first observation, which is honest: the
//! ledger genuinely does not know its prior state. What must not happen is an
//! eviction being read as RECOVERY, announcing that a room's absence ended when
//! in truth its record was dropped. Callers get `None` from
//! [`BoundedRoomLedger::take`] for an evicted room exactly as they do for a room
//! that was never absent, and that is the property their regressions pin.
//!
//! ## Why not an LRU crate
//!
//! At this cap the order queue is a handful of entries and the linear scan in
//! `touch` is cheaper than the allocation a fancier structure would cost. A
//! dependency for thirty elements is not a trade worth making, and the existing
//! convention in this tree for bounded per-room state is exactly this shape: a
//! `const` cap plus a `VecDeque` and `pop_front`
//! (`airc_persona_conversation.rs`). This is that convention applied to the ROOM
//! KEY rather than to the per-room ring, which is the level the leak was at.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// How many rooms of diagnostics either ledger keeps.
///
/// Sized for every room a persona plausibly works in at once, plus headroom, not
/// for its lifetime history. A citizen here is subscribed to six rooms, so the
/// cap is well above ordinary work and far below the unbounded growth per-task
/// rooms would otherwise produce.
pub(crate) const ROOMS_TRACKED: usize = 32;

/// A bounded map from room to some small diagnostic value.
#[derive(Debug)]
pub(crate) struct BoundedRoomLedger<K: Eq + Hash + Copy, V> {
    entries: HashMap<K, V>,
    /// Least-recently-written FIRST. Holds exactly the keys in `entries`.
    order: VecDeque<K>,
    cap: usize,
}

impl<K: Eq + Hash + Copy, V> BoundedRoomLedger<K, V> {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            entries: HashMap::new(),
            // A cap of 0 would make every insert evict what it just wrote: a
            // silently useless ledger rather than a loud error at the call site.
            cap: cap.max(1),
            order: VecDeque::new(),
        }
    }

    fn touch(&mut self, key: K) {
        if let Some(at) = self.order.iter().position(|k| *k == key) {
            self.order.remove(at);
        }
        self.order.push_back(key);
    }

    /// Write `value` for `key`, returning the PREVIOUS value if this ledger still
    /// held one. `None` means either never-seen or evicted, deliberately the same
    /// answer, because the ledger cannot distinguish them and pretending
    /// otherwise is the exact failure this PR is about.
    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        let previous = self.entries.insert(key, value);
        self.touch(key);
        while self.order.len() > self.cap {
            if let Some(evicted) = self.order.pop_front() {
                self.entries.remove(&evicted);
            }
        }
        previous
    }

    /// Read without disturbing recency. Gated like [`Self::len`] because every
    /// production caller goes through `insert`/`take`/`entry_or` — a read that
    /// does NOT count as a write would quietly make an actively-inspected room
    /// evictable, so leaving it available to production invites that bug rather
    /// than serving a need. Ungate it the day a real caller wants it, with a
    /// decision about recency attached.
    #[cfg(test)]
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    /// Remove and return this key's value. `None` for an evicted room, same as
    /// for a room that was never written. See the module doc on why eviction must
    /// not read as recovery.
    pub(crate) fn take(&mut self, key: &K) -> Option<V> {
        let taken = self.entries.remove(key)?;
        if let Some(at) = self.order.iter().position(|k| k == key) {
            self.order.remove(at);
        }
        Some(taken)
    }

    /// Mutable access, inserting `default` first if absent. Counts as a WRITE for
    /// recency, which is what makes an actively-observed room un-evictable.
    pub(crate) fn entry_or(&mut self, key: K, default: V) -> &mut V {
        if self.entries.contains_key(&key) {
            self.touch(key);
        } else {
            self.insert(key, default);
        }
        self.entries
            .get_mut(&key)
            // Present by construction: either it already was, or the line above
            // just inserted it. entries and order hold the same key set.
            .expect("entry_or guarantees the key is present")
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        debug_assert_eq!(
            self.entries.len(),
            self.order.len(),
            "ledger key sets drift"
        );
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the leak this type exists to close. A plain HashMap
    // keyed by room retains every room the persona ever took a turn in, and
    // per-task rooms make that unbounded. Exceeding the cap must evict, and the
    // ledger must never hold more than the cap.
    #[test]
    fn exceeding_the_cap_evicts_the_least_recently_written_room() {
        let mut led = BoundedRoomLedger::new(3);
        for room in 0..3u32 {
            led.insert(room, room);
        }
        assert_eq!(led.len(), 3);

        // Touch room 0 so room 1 becomes the least-recently-written.
        led.insert(0, 100);
        led.insert(9, 9);

        assert_eq!(led.len(), 3, "the cap is a ceiling, not a suggestion");
        assert_eq!(led.get(&1), None, "the least-recently-WRITTEN room goes");
        assert_eq!(
            led.get(&0),
            Some(&100),
            "a room being actively written to must survive a newer arrival: \
             recency-by-write is the whole reason this is not plain FIFO"
        );
        assert_eq!(led.get(&9), Some(&9));
    }

    // what this catches: EVICTION MUST NOT READ AS RECOVERY (@Astra, #3903). A
    // caller asks take() whether this room had a running absence. For an evicted
    // room the honest answer is unknown, and the only safe encoding of that in
    // this API is the same None a never-absent room gives, so a caller cannot
    // accidentally announce that a room's absence of N ticks ended about a record
    // that was simply dropped.
    #[test]
    fn an_evicted_room_reports_nothing_rather_than_a_recovery() {
        let mut led = BoundedRoomLedger::new(2);
        led.insert(1, 7u32);
        led.insert(2, 7);
        led.insert(3, 7); // evicts room 1

        assert_eq!(led.take(&1), None, "an evicted room yields no prior value");
        assert_eq!(
            led.take(&99),
            None,
            "non-degeneracy: a room that was NEVER written gives the same answer, \
             which is what makes the two indistinguishable to a caller by design"
        );
        assert_eq!(led.take(&3), Some(7), "a live room still yields its value");
    }

    // what this catches: revisiting an evicted room must behave as a FIRST
    // observation. No stale value resurrected, and the key set stays consistent.
    #[test]
    fn revisiting_an_evicted_room_starts_over() {
        let mut led = BoundedRoomLedger::new(2);
        led.insert(1, 5u32);
        led.insert(2, 5);
        led.insert(3, 5); // evicts 1
        assert_eq!(led.get(&1), None);

        assert_eq!(
            led.insert(1, 42),
            None,
            "the returned previous value must be None: there is no prior state to \
             compare against, and claiming one would invent history"
        );
        assert_eq!(led.get(&1), Some(&42));
        assert_eq!(led.len(), 2, "and the cap still holds after the revisit");
    }

    // what this catches: entry_or counts as a write, so a room being incremented
    // every tick can never become the eviction victim.
    #[test]
    fn entry_or_refreshes_recency() {
        let mut led = BoundedRoomLedger::new(2);
        led.insert(1, 0u32);
        led.insert(2, 0);
        *led.entry_or(1, 0) += 1;
        led.insert(3, 0);
        assert_eq!(led.get(&1), Some(&1), "the touched room survived");
        assert_eq!(led.get(&2), None, "the untouched one was evicted");
    }
}
