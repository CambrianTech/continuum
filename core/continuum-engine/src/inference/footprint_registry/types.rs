//! Pure data shapes for the per-component memory footprint registry.
//!
//! Isolated into its own module so the registry's data model stays legible
//! without wading through the registry's behavior. Everything here is
//! Serialize + Deserialize so snapshots can ship over IPC / logs.
//!
//! Behavior (reading, writing, eviction planning, sanity checking) lives
//! in `mod.rs`. Cost heuristics live in `costs.rs`. Keep this file data-only.

use crate::inference::kv_quant::Residency;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;

/// What kind of memory the entry represents. Each variant has its own
/// reload-cost characteristics that the policy uses for eviction
/// planning. `Other(String)` is the extension hatch — new resource
/// types (vision-encoder cache, MoE expert weights, etc.) land
/// without touching the enum core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// Per-sequence KV cache (the §16 quantizable resource).
    KvCache,
    /// LoRA / genome adapter weights (the §11 paging target).
    LoraAdapter,
    /// Base model weights (rarely evictable — reload is multi-second).
    ModelWeights,
    /// Bevy render buffers, avatar models, animation state.
    RenderBuffer,
    /// Tokenizer vocab + merges cache.
    TokenizerCache,
    /// Live audio pipeline buffers (STT, TTS).
    AudioPipeline,
    /// Live video pipeline frames + GPU upload buffers.
    VideoPipeline,
    /// Extension hatch — variants not yet promoted to first-class.
    Other(String),
}

/// Composite key — every dimension the policy might want to project on.
/// `Option<Uuid>` for persona/recipe means "persona-agnostic" or
/// "outside any recipe" (model weights, tokenizer cache).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FootprintKey {
    pub persona_id: Option<Uuid>,
    pub recipe_id: Option<Uuid>,
    pub backend_id: Option<String>,
    pub resource_type: ResourceType,
    pub residency: Residency,
}

impl FootprintKey {
    /// Construct a key with the most common shape: persona + resource
    /// type + residency. Recipe and backend default to None.
    pub fn for_persona(
        persona_id: Uuid,
        resource_type: ResourceType,
        residency: Residency,
    ) -> Self {
        Self {
            persona_id: Some(persona_id),
            recipe_id: None,
            backend_id: None,
            resource_type,
            residency,
        }
    }

    /// Construct a persona-agnostic key (e.g., model weights, tokenizer).
    pub fn shared(resource_type: ResourceType, residency: Residency) -> Self {
        Self {
            persona_id: None,
            recipe_id: None,
            backend_id: None,
            resource_type,
            residency,
        }
    }

    /// Construct a backend-scoped key. Used when multiple backends/models
    /// are loaded concurrently and each one's `model_weights` (or
    /// tokenizer cache, etc.) needs distinct accounting. Without the
    /// backend_id discriminator a second `report_authoritative` would
    /// overwrite the first model's bytes — silently making the second
    /// load look free.
    pub fn for_backend(
        backend_id: impl Into<String>,
        resource_type: ResourceType,
        residency: Residency,
    ) -> Self {
        Self {
            persona_id: None,
            recipe_id: None,
            backend_id: Some(backend_id.into()),
            resource_type,
            residency,
        }
    }
}

/// One entry's accounting state. `bytes` updates as the resource
/// grows/shrinks; cost estimates start as heuristics and refine from
/// observed spill/reload measurements (Phase 4.0 telemetry feedback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootprintEntry {
    pub bytes: u64,
    pub last_active: SystemTime,
    /// True if `bytes` was set by the backend's authoritative
    /// `seq_bytes()` call (ground truth) vs our internal accounting.
    /// Drift between the two = a bug to chase via `sanity_check`.
    pub backend_reported: bool,
    /// Estimated cost to spill this entry (transition from current
    /// residency to a colder tier). Microseconds. Starts as heuristic;
    /// updated from real spill measurements.
    pub spill_cost_micros: u64,
    /// Estimated cost to bring this entry back to Active. Microseconds.
    pub reload_cost_micros: u64,
}

impl FootprintEntry {
    /// Construct with default cost heuristics for the resource type.
    /// Backends can refine via `report_with_costs` once their actual
    /// spill/reload latencies are measured.
    pub fn new(bytes: u64, resource_type: &ResourceType) -> Self {
        let (spill_us, reload_us) = super::costs::default_costs_for(resource_type, bytes);
        Self {
            bytes,
            last_active: SystemTime::now(),
            backend_reported: false,
            spill_cost_micros: spill_us,
            reload_cost_micros: reload_us,
        }
    }
}

/// An eviction plan: the cheapest combination of registry entries that,
/// if evicted, would free at least `target_bytes`. Returned by
/// `cheapest_eviction_for`; the policy applies it via the backend's
/// PageableBackend lever (Phase 3.0).
#[derive(Debug, Clone)]
pub struct EvictionPlan {
    pub entries: Vec<(FootprintKey, FootprintEntry)>,
    pub bytes_freed: u64,
    pub estimated_cost_micros: u64,
}

/// Realized outcome of a pressure-driven lease-revocation pass
/// (`FootprintRegistry::revoke_leases_for`). The pressure-driven sibling
/// of the clock-driven `expire_leases`: where that reclaims leases whose
/// clock ran out, this reclaims leases the *pressure policy* chose to
/// revoke (expired → Hard → Graceful, gated by tier).
///
/// `revoked` lists the `(lease_id, bytes)` actually released, ordered
/// least-disruptive first. `bytes_freed` is their sum — the footprint
/// returned to the pool for a subsequent `cheapest_eviction_for` pass. An
/// empty `revoked` with `bytes_freed == 0` means the demand was zero (a
/// no-op pass), never "policy couldn't act" — that case is `None` from
/// `revoke_leases_for`, which the caller must treat as "escalate".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRevocationOutcome {
    pub revoked: Vec<(String, u64)>,
    pub bytes_freed: u64,
}

/// Health report from `sanity_check`. `Healthy` = registry total within
/// `drift_pct_threshold` of the monitor's process_bytes; `Drifted` =
/// something allocates without reporting (bug to chase).
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryHealth {
    Healthy {
        drift_pct: f32,
    },
    Drifted {
        registry_total: u64,
        monitor_process_bytes: u64,
        drift_pct: f32,
    },
}

/// Point-in-time snapshot of the registry, suitable for serialization to
/// logs, jtag commands, or telemetry sinks. Everything is owned (no
/// borrows into the live DashMap) so callers can hold onto a snapshot
/// across awaits without contending with concurrent allocators.
///
/// The snapshot is a passive view — mutating it does not mutate the
/// live registry. To affect state, use the `add` / `remove` /
/// `report_authoritative` methods on the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    /// Total bytes across every entry. Cross-check against monitor's
    /// `process_bytes` for drift detection.
    pub total_bytes: u64,
    /// Number of distinct entries. A growing entry count without growing
    /// total_bytes suggests fragmentation (lots of small allocations);
    /// a shrinking count with stable bytes suggests entries are being
    /// merged.
    pub entry_count: usize,
    /// Bytes broken down by resource type. Usually `ModelWeights`
    /// dominates; if `KvCache` overtakes weights, the conversation has
    /// gotten very long or n_seq_max is high.
    pub by_resource_type: HashMap<ResourceType, u64>,
    /// Per-persona total bytes. Empty entries (persona reported nothing)
    /// don't appear; absence is meaningful.
    pub by_persona: HashMap<Uuid, u64>,
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: `for_backend` setting fields on the wrong axis
    /// (e.g., putting backend_id into persona_id). Two reports for two
    /// different backends MUST land in two different entries — otherwise
    /// loading model B silently overwrites model A's bytes.
    ///
    /// Validated 2026-04-21: swapped backend_id into persona_id in the
    /// constructor; test fails because both keys collapse to the same
    /// hash (PartialEq + Hash impls compare all 5 fields); reverted.
    #[test]
    fn for_backend_keys_are_distinct_per_backend_id() {
        let key_a =
            FootprintKey::for_backend("qwen3.5-4b", ResourceType::ModelWeights, Residency::Active);
        let key_b =
            FootprintKey::for_backend("qwen3.5-7b", ResourceType::ModelWeights, Residency::Active);
        assert_ne!(
            key_a, key_b,
            "different backends must produce distinct keys"
        );
        assert_eq!(key_a.backend_id.as_deref(), Some("qwen3.5-4b"));
        assert!(key_a.persona_id.is_none());
    }

    /// What this catches: `for_persona` leaking persona_id into the wrong
    /// field, or `shared` not zeroing persona/recipe/backend. Confirms
    /// each constructor populates exactly its declared axis.
    ///
    /// Validated 2026-04-21: set backend_id in for_persona's output;
    /// test fails on assert(backend_id.is_none()); reverted.
    #[test]
    fn constructors_set_only_their_declared_axis() {
        let p = Uuid::new_v4();
        let for_p = FootprintKey::for_persona(p, ResourceType::KvCache, Residency::Active);
        assert_eq!(for_p.persona_id, Some(p));
        assert!(for_p.recipe_id.is_none());
        assert!(for_p.backend_id.is_none());

        let shared = FootprintKey::shared(ResourceType::ModelWeights, Residency::Active);
        assert!(shared.persona_id.is_none());
        assert!(shared.recipe_id.is_none());
        assert!(shared.backend_id.is_none());
    }

    /// What this catches: `FootprintEntry::new` leaving spill/reload costs
    /// at their zero initializers instead of populating from the resource
    /// type's heuristic. A zero-cost entry would always be cheapest to
    /// evict — eviction policy would starve on it.
    ///
    /// Validated 2026-04-21: hardcoded spill_us=0 in FootprintEntry::new;
    /// test fails on spill_cost_micros > 0 for ModelWeights; reverted.
    #[test]
    fn new_populates_costs_from_resource_type() {
        let e = FootprintEntry::new(2_500_000_000, &ResourceType::ModelWeights);
        assert!(
            e.spill_cost_micros > 0,
            "ModelWeights spill cost must be > 0 — policy needs a real number to reason about"
        );
        assert!(!e.backend_reported);
    }
}
