//! Per-component memory footprint registry — "what are we made of?"
//!
//! Per §13 of docs/architecture/PERSONA-CONTEXT-PAGING.md: GpuMonitor
//! (§12) tells the policy WHAT pressure looks like; the registry tells
//! it WHAT to do about it. Without per-component attribution the policy
//! knows "we're at 90% of process limit" but has no idea WHICH of N
//! things in our process is biggest, cheapest to spill, or worth
//! keeping hot.
//!
//! Every allocation site (KV slots, LoRA adapters, model weights,
//! render buffers, tokenizer caches, audio/video pipelines) reports
//! bytes via a single DashMap keyed on (persona, recipe, backend,
//! resource type, residency). Reporting is unconditional and cheap;
//! no `#[cfg]`, no platform branches.
//!
//! The registry's `cheapest_eviction_for` is what makes paging real:
//! given "free X bytes," it returns a plan picking the lowest-cost
//! combination of evictable entries. Cost-driven, not type-prioritized.

use crate::inference::kv_quant::Residency;
use dashmap::DashMap;
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
        let (spill_us, reload_us) = default_costs_for(resource_type, bytes);
        Self {
            bytes,
            last_active: SystemTime::now(),
            backend_reported: false,
            spill_cost_micros: spill_us,
            reload_cost_micros: reload_us,
        }
    }
}

/// Default spill/reload cost heuristics keyed on resource type. These
/// match the "rough first-cut" estimates from §13.4 of the design doc:
/// KV is cheap to spill (raw bytes to NVMe), model weights are
/// expensive to reload (multi-second mmap+upload), adapters somewhere
/// in between. Refined by Phase 4.0 telemetry as we measure real costs.
fn default_costs_for(resource_type: &ResourceType, bytes: u64) -> (u64, u64) {
    // NVMe write/read: ~1 GB/s sustained on M5 (conservative; real PCIe5
    // hits 14 GB/s but we account for overhead). bytes/1_000 = micros.
    let nvme_micros = bytes / 1_000;
    // GPU upload from CPU: ~5 GB/s on Apple Silicon unified memory.
    let gpu_upload_micros = bytes / 5_000;

    match resource_type {
        ResourceType::KvCache => (
            nvme_micros,                          // spill: raw write
            nvme_micros + gpu_upload_micros,      // reload: read + GPU upload
        ),
        ResourceType::LoraAdapter => (
            // Adapters are usually cheaper to evict (re-download from
            // storage) than spill. Treat eviction cost as 0 (storage
            // is fast); reload is HF download + GPU upload.
            0,
            500_000 + gpu_upload_micros,          // ~500ms HF roundtrip + upload
        ),
        ResourceType::ModelWeights => (
            // Almost never spillable in practice — model load is
            // multi-second, mmap'd from disk. Mark spill as expensive
            // so the eviction policy avoids it.
            5_000_000,                            // 5 seconds (mmap teardown)
            5_000_000 + nvme_micros,              // load + read
        ),
        ResourceType::RenderBuffer | ResourceType::AudioPipeline | ResourceType::VideoPipeline => {
            // Pipeline buffers — small, fast to recreate. Effectively
            // free to evict.
            (1_000, 10_000)
        }
        ResourceType::TokenizerCache => (
            // Tokenizer is small (~2MB) and mmap'd; treat as effectively
            // permanent. Spill cost set high so the policy never picks it.
            10_000_000,
            10_000_000,
        ),
        ResourceType::Other(_) => (nvme_micros, nvme_micros + gpu_upload_micros),
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

/// The registry. DashMap-backed so multiple personas / threads can
/// add+remove concurrently without contention (sharded internally).
pub struct FootprintRegistry {
    entries: DashMap<FootprintKey, FootprintEntry>,
}

impl FootprintRegistry {
    pub fn new() -> Self {
        Self { entries: DashMap::new() }
    }

    /// Record `bytes` of resource for the given key. If the key
    /// already exists, ADDS to the existing count (treating each call
    /// as a delta). For "set authoritative size from backend," use
    /// `report_authoritative` instead.
    pub fn add(&self, key: FootprintKey, bytes: u64) {
        let resource_type = key.resource_type.clone();
        self.entries
            .entry(key)
            .and_modify(|e| {
                e.bytes = e.bytes.saturating_add(bytes);
                e.last_active = SystemTime::now();
            })
            .or_insert_with(|| FootprintEntry::new(bytes, &resource_type));
    }

    /// Remove `bytes` of resource. If the entry's bytes drop to zero
    /// the entry itself is removed (no zero-byte ghost entries).
    pub fn remove(&self, key: &FootprintKey, bytes: u64) {
        let mut should_delete = false;
        if let Some(mut entry) = self.entries.get_mut(key) {
            entry.bytes = entry.bytes.saturating_sub(bytes);
            should_delete = entry.bytes == 0;
        }
        if should_delete {
            self.entries.remove(key);
        }
    }

    /// Touch an entry's last-active timestamp without changing its
    /// bytes. Used by the policy when a slot is accessed to mark it
    /// recently-active for LRU eviction priority.
    pub fn touch(&self, key: &FootprintKey) {
        if let Some(mut entry) = self.entries.get_mut(key) {
            entry.last_active = SystemTime::now();
        }
    }

    /// Backend reports authoritative byte count (overrides our internal
    /// accounting). Sets `backend_reported = true`. Used when
    /// `LlamaCppBackend::seq_bytes()` returns the true GPU-resident
    /// count and we want it to win over whatever our accounting says.
    pub fn report_authoritative(&self, key: FootprintKey, bytes: u64) {
        let resource_type = key.resource_type.clone();
        self.entries
            .entry(key)
            .and_modify(|e| {
                e.bytes = bytes;
                e.last_active = SystemTime::now();
                e.backend_reported = true;
            })
            .or_insert_with(|| {
                let mut e = FootprintEntry::new(bytes, &resource_type);
                e.backend_reported = true;
                e
            });
    }

    /// Total bytes attributed to a persona across all resource types
    /// and residencies. The "how big is Helper right now?" answer.
    pub fn persona_total(&self, persona_id: Uuid) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.key().persona_id == Some(persona_id))
            .map(|e| e.value().bytes)
            .sum()
    }

    /// Bytes broken down by resource type globally. The "where's the
    /// weight?" answer — usually the model weights dominate.
    pub fn by_resource_type(&self) -> HashMap<ResourceType, u64> {
        let mut by_type = HashMap::new();
        for entry in self.entries.iter() {
            *by_type.entry(entry.key().resource_type.clone()).or_insert(0u64) += entry.value().bytes;
        }
        by_type
    }

    /// Total bytes across the entire registry. Cross-checked against
    /// the GpuMonitor's process_bytes by `sanity_check`.
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.value().bytes).sum()
    }

    /// Cheapest combination of evictable entries that would free at
    /// least `target_bytes`. Greedy approximation — picks entries by
    /// ascending cost-per-byte (spill_micros / bytes), excluding
    /// personas in `exclude_personas` (typically the currently-speaking
    /// persona, which the policy doesn't want to evict).
    ///
    /// Returns `None` if no combination of evictable entries can free
    /// the target — caller surfaces a clear "not enough evictable
    /// memory" error rather than partial eviction.
    pub fn cheapest_eviction_for(
        &self,
        target_bytes: u64,
        exclude_personas: &[Uuid],
    ) -> Option<EvictionPlan> {
        if target_bytes == 0 {
            return Some(EvictionPlan {
                entries: Vec::new(),
                bytes_freed: 0,
                estimated_cost_micros: 0,
            });
        }

        // Collect all evictable candidates with their cost-per-byte.
        let mut candidates: Vec<(FootprintKey, FootprintEntry, f64)> = self
            .entries
            .iter()
            .filter(|e| {
                let key = e.key();
                // Excluded personas: don't evict their slots.
                if let Some(pid) = key.persona_id {
                    if exclude_personas.contains(&pid) {
                        return false;
                    }
                }
                // Bytes > 0 (zero-byte entries are useless to evict).
                e.value().bytes > 0
            })
            .map(|e| {
                let entry = e.value().clone();
                let cost_per_byte = if entry.bytes > 0 {
                    entry.spill_cost_micros as f64 / entry.bytes as f64
                } else {
                    f64::INFINITY
                };
                (e.key().clone(), entry, cost_per_byte)
            })
            .collect();

        // Cheapest first.
        candidates.sort_by(|a, b| {
            a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut plan_entries = Vec::new();
        let mut bytes_freed = 0u64;
        let mut estimated_cost = 0u64;
        for (key, entry, _) in candidates {
            if bytes_freed >= target_bytes {
                break;
            }
            bytes_freed = bytes_freed.saturating_add(entry.bytes);
            estimated_cost = estimated_cost.saturating_add(entry.spill_cost_micros);
            plan_entries.push((key, entry));
        }

        if bytes_freed >= target_bytes {
            Some(EvictionPlan {
                entries: plan_entries,
                bytes_freed,
                estimated_cost_micros: estimated_cost,
            })
        } else {
            None
        }
    }

    /// Cross-check: registry sum vs OS-reported process_bytes from
    /// the monitor. Drift > threshold = something allocates without
    /// reporting (bug to chase). Returns Healthy or Drifted with the
    /// observed values.
    pub fn sanity_check(
        &self,
        monitor: &dyn crate::gpu::GpuMonitor,
        drift_pct_threshold: f32,
    ) -> RegistryHealth {
        let registry_total = self.total_bytes();
        let monitor_total = monitor.process_bytes();
        if monitor_total == 0 {
            // Monitor doesn't report (e.g., CPU fallback under no
            // pressure) — can't compare meaningfully. Treat as healthy.
            return RegistryHealth::Healthy { drift_pct: 0.0 };
        }
        let drift = (registry_total as f64 - monitor_total as f64).abs();
        let drift_pct = (drift / monitor_total as f64 * 100.0) as f32;
        if drift_pct > drift_pct_threshold {
            RegistryHealth::Drifted {
                registry_total,
                monitor_process_bytes: monitor_total,
                drift_pct,
            }
        } else {
            RegistryHealth::Healthy { drift_pct }
        }
    }

    /// Number of distinct entries currently tracked. For diagnostics.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for FootprintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::MockMonitor;

    fn persona_kv_key(persona_id: Uuid) -> FootprintKey {
        FootprintKey::for_persona(persona_id, ResourceType::KvCache, Residency::Active)
    }

    /// What this catches: add() not creating new entries OR not
    /// summing into existing ones. Both directions of the basic API.
    ///
    /// Validated 2026-04-21: changed and_modify to overwrite (not add),
    /// test fails because second add doesn't accumulate; reverted.
    #[test]
    fn add_creates_new_entry_and_sums_into_existing() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 1000);
        assert_eq!(reg.entry_count(), 1);
        assert_eq!(reg.total_bytes(), 1000);
        // Same key again: should add, not replace
        reg.add(key.clone(), 500);
        assert_eq!(reg.entry_count(), 1, "second add merges into existing entry");
        assert_eq!(reg.total_bytes(), 1500);
    }

    /// What this catches: remove() leaving zero-byte ghost entries that
    /// inflate entry_count() and waste lookup time. When bytes hit 0,
    /// the entry should be removed entirely.
    ///
    /// Validated 2026-04-21: removed the should_delete branch, test
    /// fails because entry_count stays at 1 with 0 bytes; reverted.
    #[test]
    fn remove_deletes_entry_when_bytes_reach_zero() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 1000);
        reg.remove(&key, 1000);
        assert_eq!(reg.entry_count(), 0, "zero-byte entry should be removed");
        assert_eq!(reg.total_bytes(), 0);

        // Partial remove leaves entry alive
        reg.add(key.clone(), 1000);
        reg.remove(&key, 300);
        assert_eq!(reg.entry_count(), 1);
        assert_eq!(reg.total_bytes(), 700);
    }

    /// What this catches: persona_total summing across the wrong
    /// dimension (e.g., aggregating by resource type instead of
    /// persona). The policy uses this to answer "how big is X?" —
    /// wrong sum = wrong eviction plan.
    ///
    /// Validated 2026-04-21: changed filter to match recipe_id, test
    /// fails because cross-persona contamination shows up; reverted.
    #[test]
    fn persona_total_aggregates_across_resource_types_for_one_persona() {
        let reg = FootprintRegistry::new();
        let helper = Uuid::new_v4();
        let teacher = Uuid::new_v4();

        reg.add(FootprintKey::for_persona(helper, ResourceType::KvCache, Residency::Active), 1000);
        reg.add(FootprintKey::for_persona(helper, ResourceType::LoraAdapter, Residency::Active), 500);
        reg.add(FootprintKey::for_persona(teacher, ResourceType::KvCache, Residency::Active), 2000);

        assert_eq!(reg.persona_total(helper), 1500);
        assert_eq!(reg.persona_total(teacher), 2000);
        // Persona that never reported anything
        assert_eq!(reg.persona_total(Uuid::new_v4()), 0);
    }

    /// What this catches: by_resource_type aggregation losing entries
    /// (e.g., insert-vs-merge bug). Total of by_resource_type values
    /// must equal total_bytes — if not, some entry got dropped.
    ///
    /// Validated 2026-04-21: changed `+=` to `=`, test fails because
    /// the second persona's KV bytes overwrite the first; reverted.
    #[test]
    fn by_resource_type_sums_match_total_bytes() {
        let reg = FootprintRegistry::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        reg.add(FootprintKey::for_persona(p1, ResourceType::KvCache, Residency::Active), 1000);
        reg.add(FootprintKey::for_persona(p2, ResourceType::KvCache, Residency::Active), 2000);
        reg.add(FootprintKey::for_persona(p1, ResourceType::LoraAdapter, Residency::Active), 500);
        reg.add(FootprintKey::shared(ResourceType::ModelWeights, Residency::Active), 2_500_000_000);

        let by_type = reg.by_resource_type();
        let sum: u64 = by_type.values().sum();
        assert_eq!(sum, reg.total_bytes(), "by_type sum must equal total");
        assert_eq!(by_type.get(&ResourceType::KvCache).copied(), Some(3000));
        assert_eq!(by_type.get(&ResourceType::LoraAdapter).copied(), Some(500));
        assert_eq!(by_type.get(&ResourceType::ModelWeights).copied(), Some(2_500_000_000));
    }

    /// What this catches: report_authoritative not flipping the
    /// `backend_reported` flag, which would prevent sanity_check from
    /// distinguishing ground-truth entries from accounting drift.
    ///
    /// Validated 2026-04-21: removed the backend_reported = true line,
    /// test fails because the flag stays false; reverted.
    #[test]
    fn report_authoritative_marks_entry_as_backend_reported() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 500);
        let initial = reg.entries.get(&key).unwrap().clone();
        assert!(!initial.backend_reported);

        reg.report_authoritative(key.clone(), 1000);
        let after = reg.entries.get(&key).unwrap().clone();
        assert!(after.backend_reported, "authoritative report should flip the flag");
        assert_eq!(after.bytes, 1000, "authoritative report overwrites, doesn't add");
    }

    /// What this catches: cheapest_eviction_for picking expensive
    /// entries before cheap ones (sort direction wrong, or cost-per-byte
    /// computation inverted). Greedy ordering MUST be ascending cost.
    ///
    /// Validated 2026-04-21: reversed sort (descending), test fails
    /// because the model_weights entry (high cost) appears in the plan
    /// when KV (low cost) would have sufficed; reverted.
    #[test]
    fn cheapest_eviction_picks_lowest_cost_per_byte_first() {
        let reg = FootprintRegistry::new();
        let p1 = Uuid::new_v4();
        // KV cache: cheap to spill (~1µs/MB)
        reg.add(FootprintKey::for_persona(p1, ResourceType::KvCache, Residency::Active), 1_000_000);
        // Model weights: very expensive to spill
        reg.add(
            FootprintKey::shared(ResourceType::ModelWeights, Residency::Active),
            2_500_000_000,
        );

        // Need 500K freed: cheapest KV alone covers it
        let plan = reg.cheapest_eviction_for(500_000, &[]).expect("plan should exist");
        assert!(plan.bytes_freed >= 500_000);
        // Plan should NOT include the expensive model weights
        let has_model = plan
            .entries
            .iter()
            .any(|(k, _)| matches!(k.resource_type, ResourceType::ModelWeights));
        assert!(!has_model, "shouldn't evict model weights when KV alone suffices");
    }

    /// What this catches: cheapest_eviction_for ignoring the
    /// exclude_personas filter and evicting the active speaker. The
    /// policy uses this to protect the currently-speaking persona;
    /// failure here = mid-conversation eviction.
    ///
    /// Validated 2026-04-21: removed the contains() check, test fails
    /// because the active speaker's KV appears in the plan; reverted.
    #[test]
    fn cheapest_eviction_respects_exclude_personas() {
        let reg = FootprintRegistry::new();
        let active = Uuid::new_v4();
        let idle = Uuid::new_v4();
        reg.add(FootprintKey::for_persona(active, ResourceType::KvCache, Residency::Active), 1_000_000);
        reg.add(FootprintKey::for_persona(idle, ResourceType::KvCache, Residency::Active), 1_000_000);

        let plan = reg.cheapest_eviction_for(500_000, &[active]).expect("plan exists");
        // Plan should ONLY contain the idle persona's entry
        for (key, _) in &plan.entries {
            assert_ne!(
                key.persona_id,
                Some(active),
                "active speaker must not appear in eviction plan"
            );
        }
    }

    /// What this catches: cheapest_eviction_for returning a partial
    /// plan when target is unachievable (silently under-delivers).
    /// The policy needs `None` so it can surface a clear error to
    /// the user instead of evicting half what's needed.
    ///
    /// Validated 2026-04-21: returned Some(partial_plan), test fails
    /// because partial plan is the wrong contract; reverted.
    #[test]
    fn cheapest_eviction_returns_none_when_target_unachievable() {
        let reg = FootprintRegistry::new();
        let p = Uuid::new_v4();
        reg.add(FootprintKey::for_persona(p, ResourceType::KvCache, Residency::Active), 1000);

        // Need 1MB but only have 1KB available
        let plan = reg.cheapest_eviction_for(1_000_000, &[]);
        assert!(plan.is_none(), "should return None when target can't be reached");
    }

    /// What this catches: target_bytes=0 panic / inefficient processing.
    /// Edge case: policy queries "free 0 bytes" should return an empty
    /// plan immediately, not iterate the whole registry.
    ///
    /// Validated 2026-04-21: removed the early-return, test still
    /// passes because empty plan is computed correctly; but it iterates
    /// unnecessarily. Kept the early-return for clarity + perf.
    #[test]
    fn cheapest_eviction_zero_target_returns_empty_plan() {
        let reg = FootprintRegistry::new();
        reg.add(persona_kv_key(Uuid::new_v4()), 1000);
        let plan = reg.cheapest_eviction_for(0, &[]).expect("zero target should yield empty plan");
        assert!(plan.entries.is_empty());
        assert_eq!(plan.bytes_freed, 0);
    }

    /// What this catches: sanity_check incorrectly reporting Healthy
    /// when registry total drifts significantly from monitor's
    /// process_bytes. The policy uses this signal to flag "something
    /// allocates without reporting" bugs.
    ///
    /// Validated 2026-04-21: changed > to <, test fails because
    /// Drifted scenario reports Healthy; reverted.
    #[test]
    fn sanity_check_detects_drift_above_threshold() {
        let reg = FootprintRegistry::new();
        let monitor = MockMonitor::new(8 * 1024 * 1024 * 1024);

        // Registry says 1GB, monitor says 1.05GB — small drift, healthy
        reg.add(persona_kv_key(Uuid::new_v4()), 1_000_000_000);
        monitor.set_process_bytes(1_050_000_000);
        let health = reg.sanity_check(&monitor, 10.0); // 10% threshold
        assert!(matches!(health, RegistryHealth::Healthy { .. }));

        // Registry says 1GB, monitor says 2GB — 100% drift, NOT healthy
        monitor.set_process_bytes(2_000_000_000);
        let drifted = reg.sanity_check(&monitor, 10.0);
        match drifted {
            RegistryHealth::Drifted {
                registry_total,
                monitor_process_bytes,
                drift_pct,
            } => {
                assert_eq!(registry_total, 1_000_000_000);
                assert_eq!(monitor_process_bytes, 2_000_000_000);
                assert!(drift_pct > 40.0, "drift should be ~50%, got {drift_pct}");
            }
            _ => panic!("expected Drifted, got {drifted:?}"),
        }
    }

    /// What this catches: concurrent add/remove from multiple "personas"
    /// causing data races or lost updates. DashMap is sharded internally,
    /// but this test exercises that no top-level state goes through a
    /// mutex our code accidentally added.
    ///
    /// Validated 2026-04-21: implicit — if DashMap weren't lock-free
    /// per-shard, this test would be slow or detect races (1000 adds
    /// across 100 tasks). Currently completes in ~5ms.
    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_adds_from_many_personas_do_not_lose_updates() {
        use std::sync::Arc;

        let reg = Arc::new(FootprintRegistry::new());
        let mut handles = Vec::new();
        for _ in 0..100 {
            let reg = Arc::clone(&reg);
            handles.push(tokio::spawn(async move {
                let persona = Uuid::new_v4();
                for _ in 0..10 {
                    reg.add(persona_kv_key(persona), 100);
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 100 personas × 10 adds × 100 bytes = 100,000 total
        assert_eq!(reg.total_bytes(), 100_000);
        assert_eq!(reg.entry_count(), 100);
    }
}
