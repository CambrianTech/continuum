//! Registry of live persona airc presences.
//!
//! When the substrate boots and personas come online, each one's
//! `PersonaAircRuntime` lands here. Cognition + dispatch + lifecycle
//! orchestration look up a persona's grid presence via its
//! `persona_id`.
//!
//! Per the substrate's Tron frame
//! ([[the-substrate-is-the-grid-tron-frame]]) this is the
//! continuum-core's roster of "programs currently in The Grid" —
//! who's awake, where to reach them, when they came online. It is
//! NOT the persona's identity store (that's the persona's own airc
//! home + keypair, per [[personas-are-citizens-airc-is-identity-
//! provider]]). It is NOT a broker that forwards messages on behalf
//! of personas (that anti-pattern is named for refusal in
//! [[personas-are-citizens-airc-is-identity-provider]] §
//! "anti-patterns"). It is a lookup table — `(persona_id) ->
//! Arc<PersonaAircRuntime>`.
//!
//! ### Concurrency
//!
//! `DashMap` for lock-free reads on the hot path (every cognition
//! turn looks up its persona's runtime). Per-key writes are
//! synchronized internally.
//!
//! ### What this registry holds
//!
//! `Arc<PersonaAircRuntime>` only. Never `LocalIdentity`, never
//! `Keypair`, never secret key bytes. The runtime owns the Arc
//! handle to `airc_lib::Airc`, which holds the identity internally.
//! Continuum-side code that needs to publish as a persona reaches
//! into `runtime.airc()` and calls airc-lib directly — no
//! `sendAs(persona_id, text)` wrapper here. The "id-keyed
//! dispatch" is just registry lookup + direct call on the resolved
//! handle.

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::persona::airc_runtime::PersonaAircRuntime;

/// Registry of personas currently online in The Grid.
///
/// Threadsafe by construction (`DashMap` for the inner map +
/// `Arc<PersonaAircRuntime>` for the values). Cheap to clone the
/// registry handle and pass it to N modules — each gets a view of
/// the same shared roster.
#[derive(Default, Clone)]
pub struct PersonaAircRuntimeRegistry {
    inner: Arc<DashMap<Uuid, Arc<PersonaAircRuntime>>>,
}

impl PersonaAircRuntimeRegistry {
    /// Empty roster — nobody's online yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a persona to the roster. Idempotent: if the persona is
    /// already present, the existing Arc is replaced with the new
    /// one (the caller is responsible for ensuring the old runtime
    /// is properly shut down first). Returns the inserted Arc so the
    /// caller can keep a reference for cognition wiring.
    pub fn register(&self, runtime: PersonaAircRuntime) -> Arc<PersonaAircRuntime> {
        let arc = Arc::new(runtime);
        let persona_id = arc.persona_id();
        let agent_name = arc.agent_name().to_string();
        self.inner.insert(persona_id, arc.clone());
        tracing::info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            "registry: {agent_name} entered The Grid (roster size now {})",
            self.inner.len(),
        );
        arc
    }

    /// Look up a persona's runtime by their continuum persona_id.
    /// Returns `None` if the persona isn't online (never registered,
    /// or already shut down).
    pub fn get(&self, persona_id: Uuid) -> Option<Arc<PersonaAircRuntime>> {
        self.inner.get(&persona_id).map(|entry| entry.clone())
    }

    /// Look up a persona by their airc agent_name. Scans the
    /// registry — O(N). Acceptable for the registry sizes we expect
    /// (tens, not millions) AND for the use cases this resolves
    /// (operator commands, ad-hoc inspection). Hot-path lookups
    /// should key on `persona_id` instead.
    pub fn get_by_agent_name(&self, agent_name: &str) -> Option<Arc<PersonaAircRuntime>> {
        self.inner
            .iter()
            .find(|entry| entry.value().agent_name() == agent_name)
            .map(|entry| entry.value().clone())
    }

    /// Remove a persona from the roster. The caller is responsible
    /// for orderly shutdown of the runtime (drop the Arc, await
    /// its tasks). Returns the removed Arc if present.
    pub fn remove(&self, persona_id: Uuid) -> Option<Arc<PersonaAircRuntime>> {
        self.inner.remove(&persona_id).map(|(_, arc)| {
            tracing::info!(
                persona_id = %persona_id,
                agent_name = %arc.agent_name(),
                "registry: {} left The Grid (roster size now {})",
                arc.agent_name(),
                self.inner.len(),
            );
            arc
        })
    }

    /// Iterate over all currently-online personas. Cheap snapshot
    /// — each yielded Arc is independent; iteration doesn't hold a
    /// lock on the map.
    pub fn iter(&self) -> impl Iterator<Item = Arc<PersonaAircRuntime>> + '_ {
        self.inner.iter().map(|entry| entry.value().clone())
    }

    /// Count of personas currently online.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True when no personas are online.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let registry = PersonaAircRuntimeRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn clone_shares_roster() {
        let registry = PersonaAircRuntimeRegistry::new();
        let cloned = registry.clone();
        // Both views point at the same underlying DashMap via Arc;
        // registration through one is visible through the other.
        // (We can't construct a PersonaAircRuntime here without a
        // real airc daemon, so this test just asserts the Arc-clone
        // semantics — both registries share `Arc::strong_count` >= 2.)
        assert_eq!(Arc::strong_count(&registry.inner), 2);
        drop(cloned);
        assert_eq!(Arc::strong_count(&registry.inner), 1);
    }
}
