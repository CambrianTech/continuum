//! `SubstrateStateCache` — substrate-owned latest-StateEnvelope per
//! kind.
//!
//! Why a cache exists at all: positron's snapshot-then-live resync
//! contract requires the substrate to emit the **current** state on
//! every `Subscribe` (and `Observe`). With no cache, the substrate
//! would either replay history (forbidden per protocol — no transcript
//! replay) or re-derive state from upstream sources on every
//! subscribe (expensive and racy). A small in-memory cache of "latest
//! envelope per kind" gives the substrate a constant-time answer to
//! "what should the snapshot look like right now?"
//!
//! ## Ownership model
//!
//! - The event-bridging layer (slice 2B) writes to the cache after
//!   each new typed payload is built via [`crate::StateBuilder`] —
//!   `cache.store(envelope)`.
//! - The session handler (this slice's [`crate::session`]) reads from
//!   the cache to produce snapshot frames on `Subscribe`.
//!
//! Live broadcast to currently-attached subscribers is a SEPARATE
//! concern from the cache — the cache only answers "what's the
//! current state for resync?"; the broadcast layer (a `watch::Sender`
//! or analogous) fans live changes to attached subscribers.
//!
//! ## Why not just hold the typed payload?
//!
//! Two reasons:
//!
//! 1. The cache outputs `StateEnvelope` directly, so subscribe handling
//!    is allocation-free at the snapshot boundary (no re-serialization
//!    on resync).
//! 2. The envelope carries the revision the substrate stamped when it
//!    last produced state — the skip rule keys off THIS revision, and
//!    re-deriving it from the typed payload would mean re-allocating
//!    a revision (breaking monotonicity).
//!
//! Per `[[shared-decode-per-persona-perspective]]`: substrate decode
//! (event → typed payload → StateEnvelope) runs ONCE per arrival;
//! every subscribing renderer sees the same Arc-shared envelope. The
//! cache is the cell that holds the latest decoded value.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use positron_core::wire::StateEnvelope;

/// In-memory cache of the latest `StateEnvelope` per `kind`.
///
/// Concurrency: `Mutex<HashMap>` is fine here for the same reason it
/// is in [`crate::Revisions`] — writes happen on substrate-event
/// arrival (sub-millisecond cadence), reads happen on subscribe
/// (rare). `Arc<StateEnvelope>` lets every subscriber that snapshots
/// share the underlying allocation per
/// `[[shared-decode-per-persona-perspective]]`.
#[derive(Debug, Default)]
pub struct SubstrateStateCache {
    by_kind: Mutex<HashMap<String, Arc<StateEnvelope>>>,
}

/// The read seam the session serving needs: "give me the latest envelope for this
/// kind." Implemented by [`SubstrateStateCache`] (one store) AND by the composite
/// that unions a node substrate (per-ROOM kinds) with a citizen's per-user substrate
/// (per-USER kinds), routed by kind — so `apply_subscribe`/`apply_observe` read both
/// transparently without knowing there are two stores behind the kind. This is what
/// lets a session see the room's chat AND its own nav, each from the right store.
pub trait StateSource {
    /// The latest envelope for `kind`, or `None` if no state exists for it yet
    /// (honest silence — never a fabricated empty snapshot).
    fn get_state(&self, kind: &str) -> Option<Arc<StateEnvelope>>;
}

impl StateSource for SubstrateStateCache {
    fn get_state(&self, kind: &str) -> Option<Arc<StateEnvelope>> {
        self.get(kind)
    }
}

impl SubstrateStateCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store (replace) the latest envelope for its `kind`. The
    /// substrate calls this after every successful
    /// [`crate::StateBuilder`] emission — the envelope's `kind`
    /// string IS the cache key.
    ///
    /// Per `[[no-fallbacks-ever]]`: there's no monotonic-revision
    /// guard here. Monotonicity is enforced by [`crate::Revisions`]
    /// at the BUILDER seam, not by the cache. If a caller stores an
    /// envelope with an out-of-order revision, that's a substrate
    /// bug upstream of the cache — the cache is honest about what it
    /// was told to remember.
    pub fn store(&self, envelope: StateEnvelope) {
        self.store_arc(Arc::new(envelope));
    }

    /// Like [`Self::store`] but accepts an `Arc<StateEnvelope>`
    /// directly. Used by [`crate::Substrate::store`] so the cache
    /// and broadcast layers share the SAME allocation rather than
    /// each wrapping their own clone. Per
    /// `[[shared-decode-per-persona-perspective]]`: one decoded
    /// envelope, every consumer reads the same bytes.
    pub fn store_arc(&self, envelope: Arc<StateEnvelope>) {
        let kind = envelope.kind.clone();
        let mut by_kind = self.by_kind.lock().expect("cache mutex poisoned");
        by_kind.insert(kind, envelope);
    }

    /// Read the latest envelope for `kind`. `None` if the substrate
    /// has never produced state for this kind yet — a fresh
    /// substrate handing a `Subscribe { kinds: ["chat"] }` should
    /// see `None` here, meaning "no snapshot to send for chat yet",
    /// NOT "send an empty snapshot." Per
    /// `[[no-fallbacks-ever]]`: silence is honest; a synthetic
    /// "empty chat" snapshot would be a fallback that hides bugs
    /// (renderer would render an empty chat that isn't real).
    pub fn get(&self, kind: &str) -> Option<Arc<StateEnvelope>> {
        let by_kind = self.by_kind.lock().expect("cache mutex poisoned");
        by_kind.get(kind).map(Arc::clone)
    }

    /// Number of kinds the cache currently holds state for. Used by
    /// tests + diagnostics; not part of the snapshot-then-live hot
    /// path.
    pub fn len(&self) -> usize {
        self.by_kind.lock().expect("cache mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::wire::StateLayer;

    fn envelope(kind: &str, revision: u64) -> StateEnvelope {
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn store_then_get_returns_the_latest() {
        // what this catches: regression where the cache silently drops
        // a store, OR where `get` returns a stale value. Both would
        // make snapshot-then-live emit wrong revisions on subscribe.
        let cache = SubstrateStateCache::new();
        cache.store(envelope("chat", 1));
        assert_eq!(cache.get("chat").map(|e| e.revision), Some(Some(1)));
        cache.store(envelope("chat", 2));
        assert_eq!(cache.get("chat").map(|e| e.revision), Some(Some(2)));
    }

    #[test]
    fn unknown_kind_returns_none_not_empty_envelope() {
        // what this catches: regression where the cache lazy-inits a
        // missing kind to a default envelope. That would be a fallback
        // — renderer would render fake-empty state instead of the
        // honest "nothing yet" answer.
        let cache = SubstrateStateCache::new();
        cache.store(envelope("chat", 1));
        assert!(cache.get("unknown").is_none());
    }

    #[test]
    fn store_is_per_kind_not_global() {
        // what this catches: regression where `store` accidentally
        // overwrites unrelated kinds (e.g. if a refactor switched the
        // hashmap key to something derived). Would break multi-widget
        // surfaces.
        let cache = SubstrateStateCache::new();
        cache.store(envelope("chat", 5));
        cache.store(envelope("user-list", 1));
        assert_eq!(cache.get("chat").map(|e| e.revision), Some(Some(5)));
        assert_eq!(cache.get("user-list").map(|e| e.revision), Some(Some(1)));
        assert_eq!(cache.len(), 2);
    }
}
