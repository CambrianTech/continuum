//! Substrate-side `Observe` handler — AI-observer resync mirror of
//! [`crate::session::apply_subscribe`].
//!
//! Positron protocol §"Observers resync identically":
//!
//! > [`ClientMessage::Observe`] is declarative per `observer_id`
//! > (re-observe REPLACES that observer's registration) and triggers
//! > the same snapshot-then-live with the same exact-equality skip on
//! > its `last_seen`. A reconnecting AI observer rebuilds its
//! > perceived world exactly like a renderer does — there is one
//! > resync contract, not a human one and an AI one.
//!
//! This module is that mirror at the substrate seam. Same exact-
//! equality skip rule (via [`crate::session::should_skip`]),
//! declarative replace at the type level (no `merge` method on
//! [`ObserverRegistration`]).
//!
//! ## What `ObserverRegistration` captures
//!
//! - `observer_id` — substrate-routed identity (e.g. a persona UUID
//!   string). Re-observe under the same id REPLACES the prior
//!   registration for THAT observer; other observers stay attached.
//! - `budget_hz` — the observer's requested perception rate. Honored
//!   by the perception-budget enforcement layer (slice 2D live
//!   broadcast); slice 2B/C just records the field.
//! - `kinds` + `layers` — what this observer perceives. Same shape
//!   as [`crate::session::Subscription`] so the broadcast layer can
//!   gate envelope forwarding through the same `covers()` predicate.
//!
//! ## Why budget_hz is captured here and not enforced yet
//!
//! Per positron docs §"Cognition budget": the substrate quantizes an
//! observer's perception under load. Enforcement is a broadcast-time
//! concern (drop / coalesce envelopes that would exceed the
//! observer's per-second budget). This module just records the
//! declared budget; the live-broadcast layer (slice 2D) reads it
//! when wiring the per-observer fan-out.

use std::collections::HashSet;

use positron_core::session::{ClientMessage, ServerMessage};
use positron_core::wire::{StateEnvelope, StateLayer};

use crate::cache::SubstrateStateCache;
use crate::session::should_skip;

/// One AI observer's substrate-recorded registration. Built by
/// [`apply_observe`]; consulted by the live-broadcast layer to
/// gate per-envelope forwarding and to enforce the `budget_hz`
/// quantization.
///
/// `kinds` uses `HashSet` for O(1) membership tests; `layers` uses
/// `Vec` for symmetry with [`crate::session::Subscription`] (the
/// broadcast-time `covers(kind, layer)` predicate keys off both).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverRegistration {
    pub observer_id: String,
    pub budget_hz: u32,
    pub kinds: HashSet<String>,
    pub layers: Vec<StateLayer>,
}

impl ObserverRegistration {
    /// True iff this observer perceives `(kind, layer)`. The live-
    /// broadcast layer calls this to gate per-envelope forwarding
    /// before the per-observer `budget_hz` quantization runs.
    pub fn covers(&self, kind: &str, layer: StateLayer) -> bool {
        self.kinds.contains(kind) && self.layers.contains(&layer)
    }
}

/// Apply a `ClientMessage::Observe` against the substrate's current
/// state cache and produce:
///
/// 1. The new [`ObserverRegistration`] for this observer (REPLACES
///    the prior registration under the same `observer_id` — see the
///    declarative-replace doctrine in [`crate::session`] module
///    docs).
/// 2. The Vec of `ServerMessage::State` frames the substrate must
///    emit immediately to satisfy snapshot-then-live for the new
///    perception scope, with the exact-equality skip rule applied
///    per kind.
///
/// Returns `Err` if `msg` is NOT an `Observe` variant — single-
/// purpose handler per [[no-fallbacks-ever]].
pub fn apply_observe(
    cache: &SubstrateStateCache,
    msg: ClientMessage,
) -> Result<(ObserverRegistration, Vec<ServerMessage>), String> {
    let (spec, last_seen) = match msg {
        ClientMessage::Observe { spec, last_seen } => (spec, last_seen),
        other => {
            return Err(format!(
                "apply_observe: expected Observe variant, got {other:?}"
            ));
        }
    };

    let mut layers = spec.layers;
    layers.sort_by_key(|l| *l as u8);
    layers.dedup();

    let kinds_set: HashSet<String> = spec.kinds.iter().cloned().collect();
    let mut snapshots: Vec<ServerMessage> = Vec::with_capacity(spec.kinds.len());

    for kind in &spec.kinds {
        let Some(current) = cache.get(kind) else {
            // Same honest-silence semantic as subscribe: no cached
            // state for this kind → no snapshot to send this cycle.
            continue;
        };
        if should_skip(&current, kind, &last_seen) {
            continue;
        }
        snapshots.push(ServerMessage::State(StateEnvelope::clone(&current)));
    }

    Ok((
        ObserverRegistration {
            observer_id: spec.observer_id,
            budget_hz: spec.budget_hz,
            kinds: kinds_set,
            layers,
        },
        snapshots,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::session::KindRevision;
    use positron_core::wire::{ObserverSpec, StateLayer};

    fn env(kind: &str, revision: u64) -> StateEnvelope {
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({"ok": true}),
        }
    }

    fn obs(
        observer_id: &str,
        budget_hz: u32,
        kinds: &[&str],
        last_seen: Vec<KindRevision>,
    ) -> ClientMessage {
        ClientMessage::Observe {
            spec: ObserverSpec {
                observer_id: observer_id.to_string(),
                budget_hz,
                kinds: kinds.iter().map(|s| s.to_string()).collect(),
                layers: vec![StateLayer::Session],
            },
            last_seen,
        }
    }

    #[test]
    fn first_observe_with_empty_last_seen_emits_current_snapshots() {
        // what this catches: regression where the observer resync
        // contract diverges from the renderer one. Per protocol
        // §"Observers resync identically" there is ONE resync
        // contract — observe and subscribe must behave the same way.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 11));
        let (r, frames) = apply_observe(&cache, obs("maya", 4, &["chat"], vec![])).unwrap();
        assert_eq!(r.observer_id, "maya");
        assert_eq!(r.budget_hz, 4);
        assert!(r.covers("chat", StateLayer::Session));
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            ServerMessage::State(e) => {
                assert_eq!(e.kind, "chat");
                assert_eq!(e.revision, Some(11));
            }
            other => panic!("expected State, got {other:?}"),
        }
    }

    #[test]
    fn matching_last_seen_skips_the_snapshot_same_as_subscribe() {
        // what this catches: regression where the observer skip rule
        // diverges from the subscriber skip rule. Both must key off
        // the same `should_skip` helper so a future protocol clarif-
        // ication only needs one edit.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 11));
        let (_, frames) = apply_observe(
            &cache,
            obs(
                "maya",
                4,
                &["chat"],
                vec![KindRevision {
                    kind: "chat".into(),
                    revision: 11,
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
    fn higher_last_seen_does_NOT_skip_for_observers_either() {
        // what this catches: the SAME load-bearing invariant as the
        // subscriber test. An observer holding last_seen=500 from a
        // pre-restart substrate must re-receive the snapshot, never
        // skip. Drift between Observer and Subscribe handling is the
        // exact bug class one-resync-contract exists to prevent.
        let cache = SubstrateStateCache::new();
        cache.store(env("chat", 3));
        let (_, frames) = apply_observe(
            &cache,
            obs(
                "maya",
                4,
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
            "observer@500 vs substrate@3 must SEND the snapshot — \
             same exact-equality invariant as the subscriber path"
        );
    }

    #[test]
    fn refuses_non_observe_variant_loudly() {
        let cache = SubstrateStateCache::new();
        let err = apply_observe(
            &cache,
            ClientMessage::Subscribe {
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session],
                last_seen: vec![],
            },
        )
        .unwrap_err();
        assert!(
            err.contains("Observe"),
            "error must name the expected variant: {err}"
        );
    }
}
