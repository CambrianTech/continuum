//! Monotonic per-kind revision counter.
//!
//! The wire contract: `StateEnvelope.revision` is `Option<u64>`,
//! monotonic PER KIND. The session protocol's
//! `Subscribe { last_seen: [{kind, revision}] }` replay-on-resync
//! property is built on this invariant — a subscriber asks "give me
//! everything past my last_seen" and the substrate replays the
//! `StateEnvelope`s whose revisions are greater.
//!
//! Revisions are per-kind (not per `(kind, layer)`) because
//! `ViewState::revision()` is one counter per state instance. Layer
//! classifies an UPDATE's cadence, not state identity.
//!
//! The key is the kind STRING itself (`&'static str`, e.g. `"chat"`) —
//! each `ViewState` owns its `KIND` const (open self-registration), so
//! there is no central enum of kinds. A new view registers a counter
//! simply by publishing under its own kind; nothing here enumerates the
//! set. Insertion is always driven by a `ViewState`'s `'static` kind, so
//! keying on `&'static str` is zero-allocation and the map never needs
//! an owned `String`. Reads (`current`) accept any `&str` (a wire-
//! supplied cursor) via borrow.
//!
//! Per `[[no-fallbacks-ever]]`: there is no "did we drop a revision?"
//! recovery. The counter is monotonic and substrate-owned. If a
//! revision is missed on the wire, the resubscribe path's `last_seen`
//! semantics catches it — that's a wire-layer concern, not a
//! revision-source concern.

use std::collections::HashMap;
use std::sync::Mutex;

/// In-process monotonic revision generator. Cheap to share via `Arc`.
///
/// Concurrency: `Mutex<HashMap>` is the right choice here. Revisions
/// fire on substrate event arrivals (chat message ingested, persona
/// roster changes, etc.) — that's an O(1) hashmap probe + integer
/// bump per event. A lock-free counter per key would be premature for
/// the arrival rate (worst case substrate-event-arrival rate is
/// bounded by airc IPC throughput, which is sub-millisecond at the
/// substrate's per-tick cadence anyway). When measurements show this
/// lock is hot, swap to `DashMap` without touching the public API.
#[derive(Debug, Default)]
pub struct Revisions {
    next: Mutex<HashMap<&'static str, u64>>,
}

impl Revisions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next revision for `kind`. Monotonic within a
    /// single `Revisions` instance — the first call for a key returns
    /// `1`, then `2`, etc. (Revisions start at 1 so a `Some(0)` from
    /// a buggy subscriber can't pretend it has seen "the empty state"
    /// by accident.)
    pub fn next(&self, kind: &'static str) -> u64 {
        let mut next = self.next.lock().expect("Revisions mutex poisoned");
        let slot = next.entry(kind).or_insert(0);
        *slot += 1;
        *slot
    }

    /// Read the current revision for `kind` without advancing.
    /// Returns `None` if no revision has been allocated for this key
    /// yet — useful for the session-resume path so a subscriber's
    /// `last_seen` (a wire-supplied `&str`) can be checked against
    /// current state.
    pub fn current(&self, kind: &str) -> Option<u64> {
        let next = self.next.lock().expect("Revisions mutex poisoned");
        next.get(kind).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_is_strictly_monotonic_per_kind() {
        // what this catches: regression where the counter resets or
        // wraps within a session. Session-protocol replay would mis-
        // route — a subscriber's last_seen=42 plus a counter that
        // jumped back to 5 means "drop everything from 5 to 42 again
        // on reconnect" or "skip 5-42 entirely". Both break §6.
        let r = Revisions::new();
        assert_eq!(r.next("chat"), 1);
        assert_eq!(r.next("chat"), 2);
        assert_eq!(r.next("chat"), 3);
        assert_eq!(r.current("chat"), Some(3));
    }

    #[test]
    fn counters_are_independent_per_kind_string() {
        // what this catches: regression where two different kind
        // strings collide onto one counter (e.g. a stray `&str`
        // interning bug or a `HashMap` keyed on something coarser than
        // the full kind). Each open-registered kind must own its own
        // monotonic sequence — "wall" advancing must never bump "chat".
        let r = Revisions::new();
        assert_eq!(r.next("chat"), 1);
        assert_eq!(r.next("wall"), 1);
        assert_eq!(r.next("chat"), 2);
        assert_eq!(r.current("chat"), Some(2));
        assert_eq!(r.current("wall"), Some(1));
    }

    #[test]
    fn current_is_none_before_first_next() {
        // what this catches: regression where `current` lazy-inits
        // the entry to `Some(0)` and the resume path reads it as
        // "seen the empty state". Per the doctrine, no allocation
        // for a kind means no revision — the wire `Option<u64>` is
        // honest about that.
        let r = Revisions::new();
        assert_eq!(r.current("chat"), None);
    }
}
