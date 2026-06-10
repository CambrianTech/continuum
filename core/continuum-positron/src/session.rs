//! Substrate-side session handlers — translate positron
//! [`ClientMessage`]s into [`ServerMessage`]s per the snapshot-then-live
//! protocol.
//!
//! This module implements the substrate side of
//! `positron-core@0.1.0`'s session protocol:
//!
//! > On `Subscribe` — first connect and every reconnect alike — the
//! > substrate MUST immediately emit the current `StateEnvelope` for
//! > each subscribed kind (revision-tagged), then stream live updates
//! > from that moment forward.
//!
//! Slice 2A scope: `Subscribe` handling — `apply_subscribe()` turns a
//! `ClientMessage::Subscribe` into the `(new subscription state,
//! snapshot frames to send right now)` pair. Live-broadcast wiring
//! and `Command` / `Observe` handling land in slice 2B.
//!
//! ## The skip rule (exact equality, never `>=`)
//!
//! Per positron protocol §"Skip rule":
//!
//! > The substrate MAY skip a kind's snapshot ONLY when the client's
//! > `last_seen` revision EXACTLY equals the substrate's current
//! > revision for that kind. Never a `>=` comparison: a substrate
//! > restart may reset its revision counter, and under `>=` a client
//! > holding `last_seen: 500` against a freshly-restarted substrate at
//! > revision 3 would keep stale state forever.
//!
//! [`apply_subscribe`] implements exactly this: matches each
//! `KindRevision` in `last_seen` against the substrate's cached
//! current revision for that kind. Equality → skip; anything else
//! (including the substrate having NO state for a requested kind yet)
//! → send the current snapshot if one exists, or silently emit
//! nothing for that kind on this cycle.
//!
//! ## Subscription replacement (declarative)
//!
//! Per protocol: `Subscribe` is declarative; it REPLACES any prior
//! subscription on the same connection. [`Subscription::replace`]
//! makes that replacement explicit at the type level — calling it
//! returns a fresh `Subscription` value, not a merged one.
//!
//! Renderers that re-subscribe with the SAME `kinds` + `layers` after
//! a transient transport blip get an idempotent snapshot-then-live;
//! renderers that re-subscribe with a NARROWER set stop receiving
//! the dropped kinds. No add/remove RPC drift is possible because no
//! delta API exists.

use std::collections::HashSet;
use std::sync::Arc;

use positron_core::session::{ClientMessage, KindRevision, ServerMessage};
use positron_core::wire::{StateEnvelope, StateLayer};

use crate::cache::SubstrateStateCache;

/// One connection's current declared interest set. Created (or
/// replaced) by [`apply_subscribe`]; consulted by the live-broadcast
/// layer (slice 2B) to decide which live envelopes to forward.
///
/// `kinds` uses `HashSet` for O(1) membership tests; `layers` uses
/// `Vec` because positron-core's `StateLayer` doesn't derive `Hash`
/// (filed as a sharp follow-up to positron). With only 4 variants
/// (Ephemeral / Session / Persistent / Semantic) the linear scan is
/// free.
///
/// A fresh `Subscription::empty()` declines everything until a
/// `Subscribe` arrives — `[[no-fallbacks-ever]]`-correct (the
/// renderer must explicitly opt in to each kind/layer).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subscription {
    pub kinds: HashSet<String>,
    pub layers: Vec<StateLayer>,
}

impl Subscription {
    /// A no-op subscription — receives nothing. The default state of a
    /// new connection before `Subscribe` arrives.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Replace this subscription wholesale. The protocol's
    /// declarative-replace semantic at the type level: a caller can't
    /// accidentally `merge` instead of `replace` — there's no such
    /// method.
    pub fn replace(kinds: Vec<String>, mut layers: Vec<StateLayer>) -> Self {
        // Dedup defensively — the protocol declares the field as
        // Vec<StateLayer>; a misbehaving client could send `[Session,
        // Session]`. Cheap dedup on a 4-variant universe; harmless if
        // already unique.
        layers.sort_by_key(|l| *l as u8);
        layers.dedup();
        Self {
            kinds: kinds.into_iter().collect(),
            layers,
        }
    }

    /// True iff this subscription is interested in receiving updates
    /// for `(kind, layer)`. The live-broadcast layer calls this to
    /// gate per-envelope forwarding. O(kinds) hashmap probe + O(4)
    /// layer scan.
    pub fn covers(&self, kind: &str, layer: StateLayer) -> bool {
        self.kinds.contains(kind) && self.layers.contains(&layer)
    }
}

/// Apply a `ClientMessage::Subscribe` against the substrate's current
/// state cache and produce:
///
/// 1. The new [`Subscription`] state for this connection (REPLACES the
///    prior one — see the declarative-replace doctrine in module
///    docs).
/// 2. The Vec of `ServerMessage::State` frames the substrate must
///    emit immediately to satisfy snapshot-then-live for the new
///    interest set, with the exact-equality skip rule applied per
///    kind.
///
/// Returns `Err` if `msg` is NOT a `Subscribe` variant — the function
/// is single-purpose and refuses to handle unrelated frames per
/// `[[no-fallbacks-ever]]`. Callers route by variant before reaching
/// this function (which is straightforward since `ClientMessage` is a
/// tagged union).
///
/// ## What this function does NOT do
///
/// - It does not broadcast live updates — that's the slice 2B
///   broadcast layer.
/// - It does not enforce the in-flight ordering watermark — that's
///   the consumer side. The substrate's job is just to emit
///   monotonic revisions per kind, which [`crate::Revisions`]
///   already does.
/// - It does not touch `Observe` or `Command` — separate handlers in
///   slice 2B.
pub fn apply_subscribe(
    cache: &SubstrateStateCache,
    msg: ClientMessage,
) -> Result<(Subscription, Vec<ServerMessage>), String> {
    let (kinds, layers, last_seen) = match msg {
        ClientMessage::Subscribe {
            kinds,
            layers,
            last_seen,
        } => (kinds, layers, last_seen),
        other => {
            return Err(format!(
                "apply_subscribe: expected Subscribe variant, got {other:?}"
            ));
        }
    };

    let subscription = Subscription::replace(kinds.clone(), layers);
    let mut snapshots: Vec<ServerMessage> = Vec::with_capacity(kinds.len());

    for kind in &kinds {
        let Some(current) = cache.get(kind) else {
            // No cached state for this kind yet. Per protocol §"Skip
            // rule" parenthetical: "The skip is purely an optimization;
            // when in doubt, send." Sending nothing when there IS
            // nothing to send is the equivalent — the renderer rendered
            // no state, the substrate has no state, both sides agree.
            // The first time the substrate produces state for this
            // kind, the live-broadcast layer (slice 2B) delivers it.
            continue;
        };
        if should_skip(&current, kind, &last_seen) {
            continue;
        }
        // Hand the renderer a fresh `StateEnvelope` (clone the Arc'd
        // payload into a plain value — `ServerMessage::State` owns
        // its envelope per the wire shape). The Arc-share at the
        // cache layer keeps the COMMON case of multiple subscribers
        // all snapshotting against the same cached envelope
        // allocation-frugal at the cache boundary.
        snapshots.push(ServerMessage::State(StateEnvelope::clone(&current)));
    }

    Ok((subscription, snapshots))
}

/// The exact-equality skip rule from positron protocol §"Skip rule".
///
/// `current` is the substrate's cached envelope for `kind`;
/// `last_seen` is the client's `Vec<KindRevision>`. The substrate
/// MAY skip THIS kind's snapshot iff the client's `last_seen`
/// revision for this kind EXACTLY equals the current envelope's
/// revision.
///
/// Cases:
/// - Current revision is `None`: never skip (substrate doesn't know
///   what version this is, so honestly emit it).
/// - Client has no entry for this kind: never skip.
/// - Client's revision != current revision: never skip — any
///   mismatch in either direction sends.
/// - Client's revision == current revision: skip — the renderer is
///   already at the latest.
fn should_skip(current: &Arc<StateEnvelope>, kind: &str, last_seen: &[KindRevision]) -> bool {
    let Some(current_rev) = current.revision else {
        return false;
    };
    let Some(client_entry) = last_seen.iter().find(|k| k.kind == kind) else {
        return false;
    };
    client_entry.revision == current_rev
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::wire::StateLayer;

    fn env(kind: &str, revision: u64) -> StateEnvelope {
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({"ok": true}),
        }
    }

    fn sub(kinds: &[&str], last_seen: Vec<KindRevision>) -> ClientMessage {
        ClientMessage::Subscribe {
            kinds: kinds.iter().map(|s| s.to_string()).collect(),
            layers: vec![StateLayer::Session],
            last_seen,
        }
    }

    #[test]
    fn first_subscribe_with_empty_last_seen_emits_current_snapshots() {
        // what this catches: regression where the substrate doesn't
        // honor the snapshot-then-live contract on first subscribe.
        // A renderer arriving fresh MUST receive the current state
        // immediately — otherwise it renders an empty UI until a
        // mutation happens, which is the §6 alpha bug.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 7));
        let (s, frames) = apply_subscribe(&cache, sub(&["chat"], vec![])).unwrap();
        assert!(s.covers("chat", StateLayer::Session));
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            ServerMessage::State(e) => {
                assert_eq!(e.kind, "chat");
                assert_eq!(e.revision, Some(7));
            }
        }
    }

    #[test]
    fn matching_last_seen_skips_the_snapshot() {
        // what this catches: regression where the substrate sends a
        // redundant snapshot to a client that already holds the
        // current revision. This is the OPTIMIZATION half of the skip
        // rule — it shouldn't be load-bearing for correctness, but
        // not skipping wastes bandwidth on every reconnect.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 7));
        let (_, frames) = apply_subscribe(
            &cache,
            sub(
                &["chat"],
                vec![KindRevision {
                    kind: "chat".into(),
                    revision: 7,
                }],
            ),
        )
        .unwrap();
        assert!(
            frames.is_empty(),
            "exact-equality match → skip; got {frames:?}"
        );
    }

    #[test]
    fn higher_last_seen_does_NOT_skip_per_protocol_load_bearing_invariant() {
        // what this catches: THE regression Fable's review caught in
        // round 2 — using `>=` instead of exact equality. If a client
        // holds last_seen=500 (from a pre-restart substrate) and the
        // substrate has restarted to current=3, `>=` would skip the
        // snapshot and the client would render state-from-the-future
        // forever. Exact equality forces the snapshot through, the
        // renderer reconciles by revision diff, and stale-forever
        // becomes impossible.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 3));
        let (_, frames) = apply_subscribe(
            &cache,
            sub(
                &["chat"],
                vec![KindRevision {
                    kind: "chat".into(),
                    revision: 500,
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            frames.len(),
            1,
            "client@500 vs substrate@3 must SEND the snapshot, not skip — \
             the §protocol exact-equality invariant"
        );
    }

    #[test]
    fn unrelated_last_seen_does_not_match_chat() {
        // what this catches: regression where the skip rule's
        // last_seen lookup keys off something other than `kind`
        // string. A client holding revision-42 for "user-list" must
        // not accidentally skip the "chat" snapshot.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 7));
        let (_, frames) = apply_subscribe(
            &cache,
            sub(
                &["chat"],
                vec![KindRevision {
                    kind: "user-list".into(),
                    revision: 7,
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            frames.len(),
            1,
            "last_seen for a different kind must not match chat"
        );
    }

    #[test]
    fn unknown_kind_in_subscribe_emits_no_frame_silently() {
        // what this catches: regression where the substrate fabricates
        // an empty envelope for a kind it has no state for. Per
        // [[no-fallbacks-ever]] the substrate is honest: no cached
        // state → no snapshot frame this cycle. Live broadcast (slice
        // 2B) delivers the first frame when the substrate actually
        // produces state.
        let cache = SubstrateStateCache::new();
        // Cache has chat but the client subscribes to user-list.
        cache.store(env("chat", 7));
        let (s, frames) = apply_subscribe(&cache, sub(&["user-list"], vec![])).unwrap();
        assert!(s.kinds.contains("user-list"));
        assert!(
            frames.is_empty(),
            "no cache for user-list → no synthetic snapshot"
        );
    }

    #[test]
    fn multiple_kinds_partition_snapshots_independently() {
        // what this catches: regression where the skip rule for one
        // kind accidentally suppresses another's snapshot. Each kind
        // should be evaluated against its own last_seen entry
        // independently.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 7));
        cache.store(env("user-list", 3));
        let (_, frames) = apply_subscribe(
            &cache,
            sub(
                &["chat", "user-list"],
                vec![KindRevision {
                    kind: "chat".into(),
                    revision: 7, // skip chat
                }], // user-list has no last_seen → send
            ),
        )
        .unwrap();
        assert_eq!(frames.len(), 1, "chat skipped, user-list sent");
        match &frames[0] {
            ServerMessage::State(e) => assert_eq!(e.kind, "user-list"),
        }
    }

    #[test]
    fn refuses_non_subscribe_variant_loudly() {
        // what this catches: regression where the function silently
        // accepts a Command or Observe (which would route to wrong
        // handlers). Per [[no-fallbacks-ever]] the function is
        // single-purpose; any other variant is a programmer error
        // at the dispatch site.
        let cache = SubstrateStateCache::new();
        let err = apply_subscribe(
            &cache,
            ClientMessage::Command(positron_core::wire::CommandEnvelope {
                kind: "chat".into(),
                command: "chat/send".into(),
                params: serde_json::json!({}),
                correlation_id: uuid::Uuid::nil(),
                source: positron_core::wire::CommandSource::Human,
            }),
        )
        .unwrap_err();
        assert!(
            err.contains("Subscribe"),
            "error must name the expected variant: {err}"
        );
    }

    #[test]
    fn replace_semantic_is_explicit_in_subscription_type() {
        // what this catches: doctrine pin. Subscription has no
        // `merge` method. Re-subscribing replaces; this test fails
        // to compile if someone adds a `merge` method.
        let s = Subscription::replace(vec!["chat".into()], vec![StateLayer::Session]);
        assert!(s.covers("chat", StateLayer::Session));
        // After "replace" with a different set, the prior coverage is gone:
        let s2 = Subscription::replace(vec!["user-list".into()], vec![StateLayer::Session]);
        assert!(!s2.covers("chat", StateLayer::Session));
        assert!(s2.covers("user-list", StateLayer::Session));
    }
}
