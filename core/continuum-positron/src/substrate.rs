//! `Substrate` — the coordinator that owns the cache + broadcast pair.
//!
//! Two seams (cache + broadcast) for the snapshot-then-live protocol
//! are correct as separate primitives — the cold path (snapshot on
//! subscribe) and the live path (subsequent updates) have different
//! concerns. But every substrate event that produces new state MUST
//! hit BOTH seams or the system drifts:
//!
//! - Hit cache but not broadcast → live subscribers never see the
//!   update.
//! - Hit broadcast but not cache → next subscribe sees stale
//!   snapshot, doesn't reconcile to current.
//!
//! `Substrate::store(envelope)` does both in one call so substrate
//! code can't drift. Same allocation is shared between the cache
//! entry and the broadcast value (via `Arc`), so the
//! `[[shared-decode-per-persona-perspective]]` doctrine holds at
//! this seam too — one envelope per substrate event, every consumer
//! reads the same bytes.

use std::sync::Arc;

use positron_core::wire::StateEnvelope;

use crate::broadcast::Broadcast;
use crate::cache::SubstrateStateCache;

/// The substrate-side coordinator. Owns the cache (for snapshot
/// resync) and the broadcast (for live updates) as a pair; produces
/// state through the single [`Self::store`] entry point so the two
/// can't drift.
///
/// Cheap to share via `Arc`. Both internal primitives are
/// `Arc`-shared themselves; cloning a `Substrate` only clones the
/// `Arc`s.
#[derive(Debug, Clone, Default)]
pub struct Substrate {
    cache: Arc<SubstrateStateCache>,
    broadcast: Arc<Broadcast>,
}

impl Substrate {
    /// Construct with fresh, empty cache + broadcast.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from existing parts. Useful when the substrate's
    /// boot composes the cache + broadcast separately (e.g. injecting
    /// a pre-populated cache for tests).
    pub fn from_parts(cache: Arc<SubstrateStateCache>, broadcast: Arc<Broadcast>) -> Self {
        Self { cache, broadcast }
    }

    /// Substrate produces new state. Cache stores the snapshot;
    /// broadcast fans out to live subscribers. Both via the SAME
    /// `Arc<StateEnvelope>` allocation per `[[shared-decode-per-
    /// persona-perspective]]`.
    pub fn store(&self, envelope: StateEnvelope) {
        self.store_shared(Arc::new(envelope));
    }

    /// Store an ALREADY-SHARED envelope — the dual-sink seam (2026-08-23
    /// serialization audit): a producer publishing the same envelope to two
    /// substrates (websocket + mind) was deep-cloning the entire payload tree
    /// to satisfy `store(StateEnvelope)`'s by-value signature. Build the Arc
    /// once, hand it to every sink; the clone becomes a refcount bump. This is
    /// the module header's own promise ("every consumer reads the same bytes")
    /// finally honored on the producer side.
    pub fn store_shared(&self, arc: Arc<StateEnvelope>) {
        self.cache.store_arc(Arc::clone(&arc));
        self.broadcast.send(arc);
    }

    pub fn cache(&self) -> &SubstrateStateCache {
        &self.cache
    }

    pub fn broadcast(&self) -> &Broadcast {
        &self.broadcast
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
            payload: serde_json::json!({"rev": revision}),
        }
    }

    #[tokio::test]
    async fn store_hits_both_cache_and_broadcast() {
        // what this catches: regression where `store` only hits one
        // seam. Either silently loses live updates (broadcast missed)
        // or silently serves stale snapshots (cache missed).
        let substrate = Substrate::new();

        substrate.store(envelope("chat", 1));

        // Cache reflects it.
        let cached = substrate.cache().get("chat").expect("cache populated");
        assert_eq!(cached.revision, Some(1));

        // Broadcast reflects it — a subscriber attaches and sees
        // the latest value. Per the cold-start fix on #1603, subscribe
        // always returns a Receiver; the value is wrapped in Option to
        // distinguish "no state yet" (None) from "state exists" (Some).
        let rx = substrate.broadcast().subscribe("chat");
        let env = rx.borrow().clone().expect("broadcast populated by store");
        assert_eq!(env.revision, Some(1));
    }

    #[tokio::test]
    async fn cache_and_broadcast_see_identical_revisions_after_many_stores() {
        // what this catches: regression where the two seams drift
        // (cache at rev=3, broadcast at rev=2 or similar). Both must
        // mirror the substrate's stream of envelopes exactly.
        let substrate = Substrate::new();
        for r in 1..=5 {
            substrate.store(envelope("chat", r));
        }

        let cached = substrate.cache().get("chat").unwrap();
        let rx = substrate.broadcast().subscribe("chat");
        let live = rx.borrow().clone().expect("broadcast populated");
        assert_eq!(cached.revision, Some(5));
        assert_eq!(live.revision, Some(5));
    }

    #[tokio::test]
    async fn from_parts_composes_externally_built_pieces() {
        // what this catches: a future composition seam that needs to
        // share a single cache across two substrates (e.g. for a
        // test fixture that pre-populates state). `from_parts` keeps
        // the API extensible.
        let cache = Arc::new(SubstrateStateCache::new());
        let broadcast = Arc::new(Broadcast::new());

        // Pre-populate via the external cache reference.
        cache.store(envelope("chat", 99));

        let substrate = Substrate::from_parts(Arc::clone(&cache), Arc::clone(&broadcast));

        // Substrate sees the pre-populated state.
        assert_eq!(
            substrate.cache().get("chat").map(|e| e.revision),
            Some(Some(99))
        );

        // And new store flows through the shared parts.
        substrate.store(envelope("chat", 100));
        assert_eq!(cache.get("chat").unwrap().revision, Some(100));
        let live = broadcast
            .subscribe("chat")
            .borrow()
            .clone()
            .expect("broadcast populated");
        assert_eq!(live.revision, Some(100));
    }
}
