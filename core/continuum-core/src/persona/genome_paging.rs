//! Genome Paging Engine
//!
//! LRU eviction scoring, memory budget tracking, and skill activation
//! decisions in Rust. Actual GPU load/unload stays in TypeScript.
//!
//! Eviction formula: score = age_seconds / (priority * 10)
//! Critical adapters (priority > 0.9) are never evicted.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use ts_rs::TS;

use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuAllocationGuard, GpuMemoryManager, GpuPriority, GpuSubsystem};

// =============================================================================
// TYPES (ts-rs generated)
// =============================================================================

/// Per-adapter state for genome paging decisions.
/// Extended from AdapterInfo with size_mb and last_used_ms for LRU.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/GenomeAdapterInfo.ts"
)]
pub struct GenomeAdapterInfo {
    /// Adapter name (e.g. "typescript-expertise")
    pub name: String,
    /// Skill domain (e.g. "code", "chat", "creative")
    pub domain: String,
    /// Size in MB when loaded
    #[ts(type = "number")]
    pub size_mb: f32,
    /// LRU priority (0.0-1.0, default 0.5). >0.9 = never evict.
    pub priority: f32,
    /// Whether this adapter is currently loaded in GPU memory
    pub is_loaded: bool,
    /// Epoch ms when last used (for LRU age calculation)
    #[ts(type = "number")]
    pub last_used_ms: u64,
    /// Trained model name for inference (if available after LoRA fine-tuning)
    #[ts(optional)]
    pub trained_model_name: Option<String>,
    /// Plasticity compaction metadata (present if adapter uses a compacted base model)
    #[ts(optional)]
    pub compaction: Option<CompactionMetadata>,
}

/// Compaction metadata for plasticity-optimized adapters.
/// When present, the adapter targets a compacted base model with fewer attention heads.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/CompactionMetadata.ts"
)]
pub struct CompactionMetadata {
    /// Fraction of parameters removed (e.g., 0.30 = 30% smaller base model)
    pub parameter_reduction: f32,
    /// Path to head_topology.json relative to adapter directory
    pub topology_path: String,
    /// Whether the compacted base model weights exist alongside the adapter
    pub has_compacted_weights: bool,
    /// Original base model size in MB (before compaction)
    #[ts(type = "number")]
    pub original_size_mb: f32,
    /// Compacted base model size in MB
    #[ts(type = "number")]
    pub compacted_size_mb: f32,
}

/// Full genome paging state for a single persona.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/GenomePagingState.ts"
)]
pub struct GenomePagingState {
    /// Soft memory budget in MB
    #[ts(type = "number")]
    pub memory_budget_mb: f32,
    /// Current memory usage in MB
    #[ts(type = "number")]
    pub memory_used_mb: f32,
    /// Memory pressure: used/budget (0.0-1.0)
    pub memory_pressure: f32,
    /// Adapters currently loaded in GPU
    pub active_adapters: Vec<GenomeAdapterInfo>,
    /// Adapters available on disk but not loaded
    pub available_adapters: Vec<GenomeAdapterInfo>,
}

/// Result of a skill activation decision.
/// Tells TypeScript what to load/unload — Rust decides, TS executes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ActivateSkillResult.ts"
)]
pub struct ActivateSkillResult {
    /// Whether activation is proceeding
    pub activated: bool,
    /// Name of the adapter being activated
    pub adapter_name: String,
    /// Adapters that must be unloaded first (in order)
    pub evicted: Vec<String>,
    /// Adapter to load (None if already loaded / cache hit)
    #[ts(optional)]
    pub to_load: Option<String>,
    /// How long the decision took (microseconds)
    #[ts(type = "number")]
    pub decision_time_us: u64,
}

// =============================================================================
// DOMAIN ACTIVITY TYPES
// =============================================================================

/// Activity tracking for a single domain.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/DomainActivity.ts"
)]
pub struct DomainActivity {
    /// Domain name
    pub domain: String,
    /// Total interaction count
    #[ts(type = "number")]
    pub interaction_count: u64,
    /// Successful interaction count
    #[ts(type = "number")]
    pub success_count: u64,
    /// Failed interaction count
    #[ts(type = "number")]
    pub failure_count: u64,
    /// Epoch ms of last activity
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    /// Whether this domain has a trained adapter
    pub has_adapter: bool,
    /// Adapter name if one exists
    #[ts(optional)]
    pub adapter_name: Option<String>,
}

/// Coverage report: what's covered, what's missing.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/CoverageReport.ts"
)]
pub struct CoverageReport {
    /// Domains with trained adapters
    pub covered: Vec<DomainActivity>,
    /// Domains with activity but no adapter
    pub gaps: Vec<DomainActivity>,
    /// Total interactions across all domains
    #[ts(type = "number")]
    pub total_interactions: u64,
    /// Ratio: covered_interactions / total_interactions
    pub coverage_ratio: f32,
}

// =============================================================================
// GENOME PAGING ENGINE
// =============================================================================

/// Per-persona genome paging engine.
/// Tracks adapter state, makes eviction/activation decisions,
/// and monitors domain activity for gap detection.
#[derive(Debug)]
pub struct GenomePagingEngine {
    pub memory_budget_mb: f32,
    pub memory_used_mb: f32,
    /// Loaded adapters keyed by name
    active: HashMap<String, GenomeAdapterInfo>,
    /// Available (not loaded) adapters keyed by name
    available: HashMap<String, GenomeAdapterInfo>,
    /// Domain activity tracking for gap detection
    domain_activity: HashMap<String, DomainActivityInternal>,
    /// GPU memory manager for real VRAM allocation tracking
    gpu_manager: Option<Arc<GpuMemoryManager>>,
    /// RAII guards for VRAM allocations (adapter_name → guard)
    allocation_guards: HashMap<String, GpuAllocationGuard>,
}

/// Internal activity tracking (not exported — CoverageReport is the public API).
#[derive(Debug, Clone)]
struct DomainActivityInternal {
    interaction_count: u64,
    success_count: u64,
    failure_count: u64,
    last_activity_ms: u64,
}

impl GenomePagingEngine {
    pub fn new(memory_budget_mb: f32) -> Self {
        Self {
            memory_budget_mb,
            memory_used_mb: 0.0,
            active: HashMap::new(),
            available: HashMap::new(),
            domain_activity: HashMap::new(),
            gpu_manager: None,
            allocation_guards: HashMap::new(),
        }
    }

    /// Set GPU memory manager for real VRAM allocation tracking.
    pub fn set_gpu_manager(&mut self, mgr: Arc<GpuMemoryManager>) {
        self.gpu_manager = Some(mgr);
    }

    /// Sync full adapter state from TypeScript.
    /// Replaces both active and available maps entirely.
    pub fn sync_state(&mut self, adapters: Vec<GenomeAdapterInfo>) {
        self.active.clear();
        self.available.clear();
        self.allocation_guards.clear(); // Release all VRAM guards on resync
        self.memory_used_mb = 0.0;

        for adapter in adapters {
            if adapter.is_loaded {
                self.memory_used_mb += adapter.size_mb;
                // Re-allocate GPU guard for already-loaded adapters
                if let Some(mgr) = &self.gpu_manager {
                    let bytes = (adapter.size_mb * 1024.0 * 1024.0) as u64;
                    if let Ok(guard) =
                        mgr.allocate(GpuSubsystem::Inference, bytes, GpuPriority::Interactive)
                    {
                        mgr.eviction_registry.register(make_entry(
                            &format!("genome:adapter:{}", adapter.name),
                            &format!("Genome {}", adapter.name),
                            GpuPriority::Interactive,
                            bytes,
                        ));
                        self.allocation_guards.insert(adapter.name.clone(), guard);
                    }
                }
                self.active.insert(adapter.name.clone(), adapter);
            } else {
                self.available.insert(adapter.name.clone(), adapter);
            }
        }
    }

    /// Memory pressure: 0.0-1.0 (used/budget).
    pub fn memory_pressure(&self) -> f32 {
        if self.memory_budget_mb <= 0.0 {
            return 0.0;
        }
        (self.memory_used_mb / self.memory_budget_mb).min(1.0)
    }

    /// Decide what to do for a skill activation request.
    /// Returns which adapters to evict and which to load.
    pub fn activate_skill(&mut self, skill_name: &str, now_ms: u64) -> ActivateSkillResult {
        let start = Instant::now();

        // Cache hit: already loaded
        if let Some(adapter) = self.active.get_mut(skill_name) {
            adapter.last_used_ms = now_ms;
            return ActivateSkillResult {
                activated: true,
                adapter_name: skill_name.to_string(),
                evicted: vec![],
                to_load: None,
                decision_time_us: start.elapsed().as_micros() as u64,
            };
        }

        // Not in available pool — unknown skill
        let adapter = match self.available.get(skill_name) {
            Some(a) => a.clone(),
            None => {
                return ActivateSkillResult {
                    activated: false,
                    adapter_name: skill_name.to_string(),
                    evicted: vec![],
                    to_load: None,
                    decision_time_us: start.elapsed().as_micros() as u64,
                };
            }
        };

        // Evict until there's room
        let mut evicted = vec![];
        while self.memory_used_mb + adapter.size_mb > self.memory_budget_mb {
            match self.select_eviction_victim() {
                Some(victim_name) => {
                    if let Some(victim) = self.active.remove(&victim_name) {
                        self.memory_used_mb -= victim.size_mb;
                        // Release GPU allocation guard (drops, releasing VRAM)
                        self.allocation_guards.remove(&victim_name);
                        // Unregister from eviction registry
                        if let Some(mgr) = &self.gpu_manager {
                            mgr.eviction_registry
                                .unregister(&format!("genome:adapter:{}", victim_name));
                        }
                        // Move to available
                        let mut unloaded = victim;
                        unloaded.is_loaded = false;
                        self.available.insert(unloaded.name.clone(), unloaded);
                        evicted.push(victim_name);
                    }
                }
                None => break, // No evictable adapters — budget exceeded
            }
        }

        // Move from available to active
        let mut loaded = self.available.remove(skill_name).unwrap_or(adapter);
        loaded.is_loaded = true;
        loaded.last_used_ms = now_ms;
        self.memory_used_mb += loaded.size_mb;

        // Allocate GPU guard for newly loaded adapter
        if let Some(mgr) = &self.gpu_manager {
            let bytes = (loaded.size_mb * 1024.0 * 1024.0) as u64;
            if let Ok(guard) =
                mgr.allocate(GpuSubsystem::Inference, bytes, GpuPriority::Interactive)
            {
                mgr.eviction_registry.register(make_entry(
                    &format!("genome:adapter:{}", loaded.name),
                    &format!("Genome {}", loaded.name),
                    GpuPriority::Interactive,
                    bytes,
                ));
                self.allocation_guards.insert(loaded.name.clone(), guard);
            }
        }

        self.active.insert(loaded.name.clone(), loaded);

        ActivateSkillResult {
            activated: true,
            adapter_name: skill_name.to_string(),
            evicted,
            to_load: Some(skill_name.to_string()),
            decision_time_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Drive eviction until memory_pressure ≤ `target_pressure`, returning
    /// total bytes freed. The lever the PressureBroker (paging::broker) calls
    /// when global resource pressure crosses a tier threshold and the broker
    /// decides this adapter pool should give some back. Pure addition over
    /// `activate_skill`'s implicit eviction — the formula and victim selection
    /// (`select_eviction_victim` → `calculate_eviction_score`) are the same;
    /// this just runs the loop without an activation request driving it.
    ///
    /// Critical adapters (priority > 0.9) are still untouchable. If every
    /// remaining adapter is critical and pressure is still above target, the
    /// loop terminates and reports the bytes already freed — never panics,
    /// never evicts a critical adapter, never returns a pressure higher than
    /// what was actually achieved.
    pub fn evict_under_pressure(&mut self, target_pressure: f32) -> u64 {
        let initial_used_mb = self.memory_used_mb;
        // No-op when already under target (cheap check before locking the
        // active map).
        if self.memory_pressure() <= target_pressure {
            return 0;
        }
        let target_used_mb = self.memory_budget_mb * target_pressure;
        loop {
            if self.memory_used_mb <= target_used_mb {
                break;
            }
            match self.select_eviction_victim() {
                Some(victim_name) => {
                    if let Some(victim) = self.active.remove(&victim_name) {
                        self.memory_used_mb -= victim.size_mb;
                        self.allocation_guards.remove(&victim_name);
                        if let Some(mgr) = &self.gpu_manager {
                            mgr.eviction_registry
                                .unregister(&format!("genome:adapter:{}", victim_name));
                        }
                        let mut unloaded = victim;
                        unloaded.is_loaded = false;
                        self.available.insert(unloaded.name.clone(), unloaded);
                    }
                }
                // No more evictable adapters — every remaining is critical.
                // Stop and report what we got. Broker's tier ladder will
                // decide whether to escalate elsewhere.
                None => break,
            }
        }
        let freed_mb = (initial_used_mb - self.memory_used_mb).max(0.0);
        (freed_mb * 1024.0 * 1024.0) as u64
    }

    /// Select the adapter with highest eviction score (most evictable).
    /// Returns None if no adapters can be evicted (all critical).
    fn select_eviction_victim(&self) -> Option<String> {
        let mut best_name: Option<String> = None;
        let mut best_score: f64 = f64::NEG_INFINITY;

        for (name, adapter) in &self.active {
            let score = calculate_eviction_score(adapter);
            if score < f64::INFINITY && score > best_score {
                best_score = score;
                best_name = Some(name.clone());
            }
        }

        best_name
    }

    /// Record domain activity (called after every inference).
    pub fn record_activity(&mut self, domain: &str, success: bool) {
        let now_ms = crate::time::EpochMs::now().millis();

        let entry =
            self.domain_activity
                .entry(domain.to_string())
                .or_insert(DomainActivityInternal {
                    interaction_count: 0,
                    success_count: 0,
                    failure_count: 0,
                    last_activity_ms: now_ms,
                });

        entry.interaction_count += 1;
        if success {
            entry.success_count += 1;
        } else {
            entry.failure_count += 1;
        }
        entry.last_activity_ms = now_ms;
    }

    /// Get coverage report — what domains are covered by adapters, what are gaps.
    pub fn coverage_report(&self) -> CoverageReport {
        // Build set of domains that have adapters
        let mut adapter_domains: HashMap<String, String> = HashMap::new();
        for adapter in self.active.values().chain(self.available.values()) {
            adapter_domains.insert(adapter.domain.clone(), adapter.name.clone());
        }

        let mut covered = Vec::new();
        let mut gaps = Vec::new();
        let mut total_interactions: u64 = 0;
        let mut covered_interactions: u64 = 0;

        for (domain, activity) in &self.domain_activity {
            total_interactions += activity.interaction_count;

            let has_adapter = adapter_domains.contains_key(domain);
            let adapter_name = adapter_domains.get(domain).cloned();

            let da = DomainActivity {
                domain: domain.clone(),
                interaction_count: activity.interaction_count,
                success_count: activity.success_count,
                failure_count: activity.failure_count,
                last_activity_ms: activity.last_activity_ms,
                has_adapter,
                adapter_name,
            };

            if has_adapter {
                covered_interactions += activity.interaction_count;
                covered.push(da);
            } else {
                gaps.push(da);
            }
        }

        // Sort gaps by interaction count (most active gaps first)
        gaps.sort_by(|a, b| b.interaction_count.cmp(&a.interaction_count));

        let coverage_ratio = if total_interactions > 0 {
            covered_interactions as f32 / total_interactions as f32
        } else {
            1.0 // No activity = fully covered (vacuous truth)
        };

        CoverageReport {
            covered,
            gaps,
            total_interactions,
            coverage_ratio,
        }
    }

    /// Get current state snapshot for IPC response.
    pub fn state(&self) -> GenomePagingState {
        GenomePagingState {
            memory_budget_mb: self.memory_budget_mb,
            memory_used_mb: self.memory_used_mb,
            memory_pressure: self.memory_pressure(),
            active_adapters: self.active.values().cloned().collect(),
            available_adapters: self.available.values().cloned().collect(),
        }
    }
}

// =============================================================================
// EVICTION SCORING
// =============================================================================

/// Calculate eviction score for an adapter.
/// Higher score = more evictable.
/// Critical adapters (priority > 0.9) return INFINITY (never evict).
///
/// Formula: age_seconds / (priority * 10)
pub fn calculate_eviction_score(adapter: &GenomeAdapterInfo) -> f64 {
    if adapter.priority > 0.9 {
        return f64::INFINITY; // Never evict critical adapters
    }

    let now_ms = crate::time::EpochMs::now().millis();

    let age_seconds = (now_ms.saturating_sub(adapter.last_used_ms)) as f64 / 1000.0;
    age_seconds / (adapter.priority as f64 * 10.0)
}

/// Calculate eviction score with explicit now_ms (for testing).
pub fn calculate_eviction_score_at(adapter: &GenomeAdapterInfo, now_ms: u64) -> f64 {
    if adapter.priority > 0.9 {
        return f64::INFINITY;
    }

    let age_seconds = (now_ms.saturating_sub(adapter.last_used_ms)) as f64 / 1000.0;
    age_seconds / (adapter.priority as f64 * 10.0)
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests;
