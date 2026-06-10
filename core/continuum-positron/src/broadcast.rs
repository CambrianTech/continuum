//! Per-kind live broadcast — substrate-side fan-out of state changes
//! to attached session tasks.
//!
//! ## Why `tokio::sync::watch` (and not `broadcast`)
//!
//! Per Fable's design call on the airc grid (positron protocol +
//! session-protocol PR):
//!
//! > `watch` per kind — and not as a compromise; it IS the protocol.
//! > StateEnvelopes are complete snapshots, so latest-wins coalescing
//! > isn't a degradation, it's the spec — an intermediate envelope a
//! > receiver never saw is dead weight, not data loss. `broadcast`
//! > would buffer N intermediates nobody should ever render (needless
//! > copies) and bolt on a Lagged error path watch structurally avoids.
//!
//! So this module wires `tokio::sync::watch::Sender<Arc<StateEnvelope>>`
//! ONE-per-kind. When the substrate produces a new envelope for a
//! kind, every connection currently subscribed to that kind's stream
//! sees the new value at its own pace. Slow consumers see the latest;
//! fast consumers see every distinct revision.
//!
//! ## Lazy per-kind init
//!
//! `tokio::sync::watch::channel(initial)` requires an initial value, so
//! we don't pre-allocate senders for kinds that may never produce
//! state. Instead the first `send()` for a kind creates that kind's
//! sender (using the new envelope as the initial value). Subsequent
//! sends update through the existing sender.
//!
//! `subscribe(kind)` returns `None` if no envelope has ever been sent
//! for `kind` — honest answer per `[[no-fallbacks-ever]]`. Callers
//! pair `subscribe` with the snapshot-then-live flow:
//! 1. `apply_subscribe` retrieves the current snapshot from
//!    [`SubstrateStateCache`] (which itself returns `None` for
//!    no-state-yet kinds).
//! 2. `Broadcast::subscribe(kind)` attaches a `watch::Receiver` for
//!    subsequent updates. If both return `None`, the connection
//!    silently waits — the first state-store for that kind will both
//!    populate the cache and (re-creating the sender if needed) emit
//!    the first watch update.
//!
//! ## Coordination with [`crate::SubstrateStateCache`]
//!
//! Cache and Broadcast are SEPARATE concerns:
//! - Cache: snapshot-on-subscribe (cold path).
//! - Broadcast: subsequent updates (live path).
//!
//! Substrate code that produces new state pushes to BOTH:
//!
//! ```ignore
//! let env = builder.session(KnownKind::Chat, chat_state);
//! cache.store(env.clone());
//! broadcast.send(Arc::new(env));
//! ```
//!
//! A `Substrate` coordinator wrapping both lands in slice 2D-2
//! alongside the per-connection session task. Slice 2D-1 (this PR)
//! ships the primitive in isolation so its single concern (per-kind
//! watch fan-out) is testable on its own.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use positron_core::wire::StateEnvelope;
use tokio::sync::watch;

/// Per-kind live broadcast — `tokio::sync::watch::Sender<Arc<StateEnvelope>>`
/// per kind, lazy-initialized on first send for that kind.
///
/// Cheap to share via `Arc`. Internal `Mutex<HashMap>` is held only
/// briefly during send/subscribe — not across `await`.
#[derive(Debug, Default)]
pub struct Broadcast {
    by_kind: Mutex<HashMap<String, watch::Sender<Arc<StateEnvelope>>>>,
}

impl Broadcast {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `envelope` as the latest state for its kind. The kind is
    /// read from `envelope.kind` — no need to pass it separately.
    ///
    /// First send for a kind lazy-initializes that kind's
    /// `watch::Sender` (using `envelope` as the initial value). The
    /// initial receiver from `watch::channel` is immediately dropped
    /// — actual subscribers attach via [`Broadcast::subscribe`] and
    /// always get a fresh `Receiver` keyed off the persisted Sender.
    ///
    /// Subsequent sends for the same kind update the existing watch
    /// sender. Subscribers see the latest value; slow subscribers
    /// see fewer-than-N intermediates, which is correct per the
    /// protocol's snapshot semantics.
    pub fn send(&self, envelope: Arc<StateEnvelope>) {
        let kind = envelope.kind.clone();
        let mut by_kind = self.by_kind.lock().expect("Broadcast mutex poisoned");
        match by_kind.get(&kind) {
            Some(sender) => {
                // `send_replace` always updates the watched value
                // and notifies any current receivers, regardless of
                // whether receivers exist. `send` (the other method)
                // is a no-op when the channel is closed (no live
                // receivers) — that would silently lose state-no-
                // subscribers-yet updates, which is precisely the
                // moment between substrate-start and first-session-
                // attach we MUST get right per `[[no-fallbacks-ever]]`.
                // Returns the old value; we drop it.
                let _ = sender.send_replace(envelope);
            }
            None => {
                let (sender, _rx) = watch::channel(envelope);
                by_kind.insert(kind, sender);
            }
        }
    }

    /// Attach a receiver for `kind`'s live updates. Returns `None` if
    /// no envelope has ever been sent for this kind — honest answer
    /// per `[[no-fallbacks-ever]]`. Sessions pair this with
    /// `apply_subscribe` snapshot frames; the typical control flow:
    ///
    /// 1. Run `apply_subscribe(cache, msg)` — returns snapshot frames
    ///    if the cache has state for the requested kinds.
    /// 2. For each kind in the new subscription, call
    ///    `broadcast.subscribe(kind)`. If `Some`, spawn the live-
    ///    forwarding task. If `None`, no live stream yet — the next
    ///    substrate `store()` for that kind will create both the
    ///    cache entry AND the watch sender, and a future resubscribe
    ///    catches up via the snapshot path.
    pub fn subscribe(&self, kind: &str) -> Option<watch::Receiver<Arc<StateEnvelope>>> {
        let by_kind = self.by_kind.lock().expect("Broadcast mutex poisoned");
        by_kind.get(kind).map(|s| s.subscribe())
    }

    /// Diagnostic: how many kinds have at least one envelope sent.
    /// Used by tests; not part of the hot path.
    pub fn kinds_active(&self) -> usize {
        self.by_kind.lock().expect("Broadcast mutex poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::wire::StateLayer;

    fn envelope(kind: &str, revision: u64) -> Arc<StateEnvelope> {
        Arc::new(StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({"rev": revision}),
        })
    }

    #[tokio::test]
    async fn subscribe_before_any_send_returns_none() {
        // what this catches: regression where the broadcast lazy-
        // initializes a sender on subscribe. That would let
        // subscribers attach against an empty stream and wait
        // forever for an initial value — looks like a bug at the
        // session-task layer but is actually a broadcast-layer
        // anti-fallback violation here.
        let b = Broadcast::new();
        assert!(b.subscribe("chat").is_none());
        assert_eq!(b.kinds_active(), 0);
    }

    #[tokio::test]
    async fn first_send_creates_kind_subsequent_sends_update() {
        // what this catches: regression where the kind's
        // watch::Sender gets re-created on every send (would drop
        // existing receivers) OR where subsequent sends are silently
        // ignored (would leave the receiver pinned to the initial
        // value).
        let b = Broadcast::new();
        b.send(envelope("chat", 1));
        assert_eq!(b.kinds_active(), 1);

        let mut rx = b.subscribe("chat").expect("attach after first send");
        // borrow_and_update marks current value as seen so the next
        // changed() actually awaits the NEXT change.
        let initial = rx.borrow_and_update().clone();
        assert_eq!(initial.revision, Some(1));

        b.send(envelope("chat", 2));
        rx.changed().await.expect("change signal");
        let next = rx.borrow_and_update().clone();
        assert_eq!(next.revision, Some(2));

        b.send(envelope("chat", 3));
        rx.changed().await.expect("change signal");
        let next = rx.borrow_and_update().clone();
        assert_eq!(next.revision, Some(3));

        // Still one kind active — sends didn't create new entries.
        assert_eq!(b.kinds_active(), 1);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_see_latest() {
        // what this catches: regression where multiple Receivers
        // for the same Sender don't all see new values. The protocol
        // claim is "every attached subscriber sees the latest" —
        // a refactor that broke fan-out would silently degrade.
        let b = Broadcast::new();
        b.send(envelope("chat", 1));

        let mut a = b.subscribe("chat").unwrap();
        let mut c = b.subscribe("chat").unwrap();

        // Reset their watermarks so the next .changed() awaits.
        a.borrow_and_update();
        c.borrow_and_update();

        b.send(envelope("chat", 2));
        a.changed().await.unwrap();
        c.changed().await.unwrap();
        assert_eq!(a.borrow().revision, Some(2));
        assert_eq!(c.borrow().revision, Some(2));
    }

    #[tokio::test]
    async fn send_with_no_subscribers_is_not_an_error() {
        // what this catches: regression where the substrate panics
        // or errors when no session is attached. That would block
        // the substrate event bus on consumer presence — exactly
        // the substrate-consumer coupling the watch primitive
        // exists to prevent.
        let b = Broadcast::new();
        b.send(envelope("chat", 1));
        b.send(envelope("chat", 2)); // no-subscribers state
        b.send(envelope("chat", 3));
        // After: a fresh subscriber sees the latest, not history.
        let rx = b.subscribe("chat").unwrap();
        assert_eq!(
            rx.borrow().revision,
            Some(3),
            "fresh subscriber sees latest, no history replay"
        );
    }

    #[tokio::test]
    async fn kinds_are_independent_streams() {
        // what this catches: regression where a send to one kind
        // wakes another kind's subscribers (cross-talk) OR where
        // the kinds share a single counter (would couple them in
        // unexpected ways). Each kind is its own watch::channel.
        let b = Broadcast::new();
        b.send(envelope("chat", 1));
        b.send(envelope("user-list", 1));
        assert_eq!(b.kinds_active(), 2);

        let mut chat_rx = b.subscribe("chat").unwrap();
        let mut user_rx = b.subscribe("user-list").unwrap();
        chat_rx.borrow_and_update();
        user_rx.borrow_and_update();

        // Send only on chat — user-list receiver MUST NOT wake.
        b.send(envelope("chat", 2));
        chat_rx.changed().await.unwrap();
        assert_eq!(chat_rx.borrow().revision, Some(2));
        // user_rx.changed() would hang — assert it's NOT ready.
        // tokio::select with a deadline is the right shape for a
        // negative test.
        let did_change = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            user_rx.changed(),
        )
        .await;
        assert!(
            did_change.is_err(),
            "user-list receiver must not see chat's send"
        );
    }

    #[tokio::test]
    async fn dropped_receivers_dont_break_subsequent_sends() {
        // what this catches: regression where the watch::Sender
        // gets dropped when all its receivers go away, and a
        // subsequent send for that kind silently fails. The Sender
        // is stored in the HashMap — it persists across receiver
        // lifecycles.
        let b = Broadcast::new();
        b.send(envelope("chat", 1));
        let rx = b.subscribe("chat").unwrap();
        drop(rx);

        // Send again; no receivers but the Sender persists.
        b.send(envelope("chat", 2));

        // A fresh subscribe attaches to the SAME persistent Sender,
        // sees the latest revision (2), not stale (1).
        let rx = b.subscribe("chat").unwrap();
        assert_eq!(rx.borrow().revision, Some(2));
    }
}
