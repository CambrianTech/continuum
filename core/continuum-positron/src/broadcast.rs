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
//! ## Lazy per-kind init — BOTH sides
//!
//! `tokio::sync::watch::channel(initial)` requires an initial value, so
//! the watched type is `Option<Arc<StateEnvelope>>`, with `None` as the
//! cold-start placeholder. Lazy-init fires from EITHER side:
//!
//! - First `send()` for a kind creates the sender with the new
//!   envelope wrapped in `Some`.
//! - First `subscribe()` for a kind that has never been sent creates
//!   the sender with `None`. The first send AFTER that subscribe
//!   transitions `None` → `Some(env)` — which IS a `watch::changed()`
//!   notification, so the subscriber that arrived first wakes up.
//!
//! Without symmetric lazy-init (the original sketch only lazy-init'd
//! on send), a renderer that subscribed BEFORE the substrate ever
//! produced state for that kind would get `None` from this function,
//! with no path to ever wake up when state finally arrived — the
//! cold-start hole the Sentinel verdict on #1603 caught.
//!
//! `subscribe(kind)` therefore always returns a `Receiver`. Callers
//! pair this with the snapshot-then-live flow:
//! 1. `apply_subscribe(cache, msg)` retrieves the current snapshot from
//!    [`SubstrateStateCache`] (which itself returns `None` for
//!    no-state-yet kinds — that's correct; no snapshot to send).
//! 2. `Broadcast::subscribe(kind)` attaches a `watch::Receiver`. The
//!    initial value may be `None` (no state yet) or `Some(env)` (state
//!    exists). Session tasks filter `None` (cold-start placeholder is
//!    not a renderer-bound frame) and forward `Some(env)` as a live
//!    `ServerMessage::State` frame on every `changed()` tick.
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
//! let env = builder.session(chat_state);
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

/// Per-kind live broadcast —
/// `tokio::sync::watch::Sender<Option<Arc<StateEnvelope>>>` per kind,
/// lazy-initialized by EITHER first-send or first-subscribe.
///
/// The `Option` wrapper is the cold-start placeholder: a subscriber
/// that arrives before any state exists gets a `Receiver` whose
/// initial value is `None`. The first `send` after that flips the
/// watched value to `Some(env)`, which is a normal
/// `watch::Receiver::changed()` notification — the early subscriber
/// wakes up exactly when state finally arrives. Without this,
/// renderers that subscribe before the first state-store would
/// silently wait forever (the Sentinel-caught hole on #1603).
///
/// Cheap to share via `Arc`. Internal `Mutex<HashMap>` is held only
/// briefly during send/subscribe — not across `await`.
#[derive(Debug, Default)]
pub struct Broadcast {
    by_kind: Mutex<HashMap<String, watch::Sender<Option<Arc<StateEnvelope>>>>>,
}

impl Broadcast {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `envelope` as the latest state for its kind. The kind is
    /// read from `envelope.kind` — no need to pass it separately.
    ///
    /// If a sender already exists (created by a prior `send` OR by an
    /// early `subscribe` waiting on cold-start), this transitions the
    /// watched value to `Some(envelope)`. If `subscribe` had set the
    /// initial value to `None`, that transition fires `changed()` for
    /// the waiting subscribers. Otherwise it just replaces an older
    /// `Some(_)`.
    ///
    /// If no sender exists yet, lazy-init with `Some(envelope)` as the
    /// initial value — same shape as the original slice 2D-1.
    ///
    /// Uses `send_replace` (not `send`) so updates persist even when
    /// no subscribers are currently attached — the substrate's event
    /// bus does not block on consumer presence.
    pub fn send(&self, envelope: Arc<StateEnvelope>) {
        let kind = envelope.kind.clone();
        let mut by_kind = self.by_kind.lock().expect("Broadcast mutex poisoned");
        match by_kind.get(&kind) {
            Some(sender) => {
                let _ = sender.send_replace(Some(envelope));
            }
            None => {
                let (sender, _rx) = watch::channel(Some(envelope));
                by_kind.insert(kind, sender);
            }
        }
    }

    /// Attach a receiver for `kind`'s live updates. ALWAYS returns a
    /// `Receiver` — if no envelope has ever been sent for `kind`,
    /// lazy-init a sender with initial value `None` and hand back a
    /// receiver pinned to that None. The first `send` for this kind
    /// will transition the watched value to `Some(env)` and wake the
    /// subscriber via `changed()`.
    ///
    /// Session tasks consume the stream as:
    /// ```ignore
    /// let mut rx = broadcast.subscribe(kind);
    /// while rx.changed().await.is_ok() {
    ///     if let Some(env) = rx.borrow_and_update().clone() {
    ///         // forward Some(env) as ServerMessage::State
    ///     }
    ///     // None is the cold-start placeholder — not a frame.
    /// }
    /// ```
    ///
    /// The cache-and-broadcast pair stays decoupled: `apply_subscribe`
    /// reads the cache for snapshot frames (returning `None` if no
    /// cached state — correct silence per `[[no-fallbacks-ever]]`);
    /// this method handles the live edge.
    pub fn subscribe(&self, kind: &str) -> watch::Receiver<Option<Arc<StateEnvelope>>> {
        let mut by_kind = self.by_kind.lock().expect("Broadcast mutex poisoned");
        match by_kind.get(kind) {
            Some(sender) => sender.subscribe(),
            None => {
                let (sender, rx) = watch::channel(None);
                by_kind.insert(kind.to_string(), sender);
                rx
            }
        }
    }

    /// Diagnostic: how many kinds the broadcast knows about (either
    /// because state has been sent for them OR because a subscriber
    /// has lazy-init'd a cold-start placeholder). Used by tests; not
    /// part of the hot path.
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
    async fn subscribe_before_any_send_yields_none_then_wakes_on_first_send() {
        // what this catches: the cold-start hole the Sentinel verdict
        // on #1603 surfaced. A renderer that opens its Subscribe BEFORE
        // the substrate has ever produced state for that kind needs a
        // path to wake when state finally arrives. Previously subscribe
        // returned None and the renderer waited forever — looked like
        // a bug at the session-task layer but was actually a broadcast-
        // primitive hole. With Option<Arc<StateEnvelope>> as the
        // watched type and lazy-init from subscribe(), the early
        // subscriber sees None initially, then changed() fires when
        // the first send transitions None → Some(env).
        let b = Broadcast::new();
        let mut rx = b.subscribe("chat");
        assert!(
            rx.borrow_and_update().is_none(),
            "cold-start placeholder is None"
        );
        // Lazy-init from subscribe counts the kind as known.
        assert_eq!(b.kinds_active(), 1);

        // The first send transitions None → Some(env). The waiting
        // receiver wakes through normal watch semantics.
        b.send(envelope("chat", 1));
        rx.changed().await.expect("first send wakes early subscriber");
        let first = rx
            .borrow_and_update()
            .clone()
            .expect("first send populates Some(env)");
        assert_eq!(first.revision, Some(1));
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

        let mut rx = b.subscribe("chat");
        // borrow_and_update marks current value as seen so the next
        // changed() actually awaits the NEXT change.
        let initial = rx
            .borrow_and_update()
            .clone()
            .expect("first send populated Some(env)");
        assert_eq!(initial.revision, Some(1));

        b.send(envelope("chat", 2));
        rx.changed().await.expect("change signal");
        let next = rx
            .borrow_and_update()
            .clone()
            .expect("Some(env) after send");
        assert_eq!(next.revision, Some(2));

        b.send(envelope("chat", 3));
        rx.changed().await.expect("change signal");
        let next = rx
            .borrow_and_update()
            .clone()
            .expect("Some(env) after send");
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

        let mut a = b.subscribe("chat");
        let mut c = b.subscribe("chat");

        // Reset their watermarks so the next .changed() awaits.
        a.borrow_and_update();
        c.borrow_and_update();

        b.send(envelope("chat", 2));
        a.changed().await.unwrap();
        c.changed().await.unwrap();
        assert_eq!(a.borrow().as_ref().unwrap().revision, Some(2));
        assert_eq!(c.borrow().as_ref().unwrap().revision, Some(2));
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
        let rx = b.subscribe("chat");
        assert_eq!(
            rx.borrow().as_ref().unwrap().revision,
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

        let mut chat_rx = b.subscribe("chat");
        let mut user_rx = b.subscribe("user-list");
        chat_rx.borrow_and_update();
        user_rx.borrow_and_update();

        // Send only on chat — user-list receiver MUST NOT wake.
        b.send(envelope("chat", 2));
        chat_rx.changed().await.unwrap();
        assert_eq!(chat_rx.borrow().as_ref().unwrap().revision, Some(2));
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
        let rx = b.subscribe("chat");
        drop(rx);

        // Send again; no receivers but the Sender persists.
        b.send(envelope("chat", 2));

        // A fresh subscribe attaches to the SAME persistent Sender,
        // sees the latest revision (2), not stale (1).
        let rx = b.subscribe("chat");
        assert_eq!(rx.borrow().as_ref().unwrap().revision, Some(2));
    }

    #[tokio::test]
    async fn cold_start_subscribers_for_different_kinds_wake_independently() {
        // what this catches: regression where lazy-init from subscribe
        // accidentally coalesces multiple kinds onto one sender, or
        // where the first send for ANY kind wakes all cold-start
        // subscribers. Each kind's None → Some transition is its own
        // event.
        let b = Broadcast::new();
        let mut chat_rx = b.subscribe("chat");
        let mut user_rx = b.subscribe("user-list");
        assert_eq!(b.kinds_active(), 2);
        assert!(chat_rx.borrow_and_update().is_none());
        assert!(user_rx.borrow_and_update().is_none());

        b.send(envelope("chat", 1));
        chat_rx.changed().await.expect("chat wakes");
        assert_eq!(
            chat_rx.borrow().as_ref().unwrap().revision,
            Some(1),
            "chat saw its own first send"
        );

        // user-list cold-start subscriber MUST still be at None — no
        // user-list send has happened yet.
        let did_change = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            user_rx.changed(),
        )
        .await;
        assert!(
            did_change.is_err(),
            "user-list cold-start subscriber must not wake on chat's send"
        );
        assert!(user_rx.borrow().is_none());
    }
}
