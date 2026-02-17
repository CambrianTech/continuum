//! Genome Paging Engine
//!
//! LRU eviction scoring, memory budget tracking, and skill activation
//! decisions in Rust. Actual GPU load/unload stays in TypeScript.
//!
//! Eviction formula: score = age_seconds / (priority * 10)
//! Critical adapters (priority > 0.9) are never evicted.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use ts_rs::TS;

// =============================================================================
// TYPES (ts-rs generated)
// =============================================================================

/// Per-adapter state for genome paging decisions.
/// Extended from AdapterInfo with size_mb and last_used_ms for LRU.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/GenomeAdapterInfo.ts")]
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
    /// Ollama model name for inference (if available)
    #[ts(optional)]
    pub ollama_model_name: Option<String>,
}

/// Full genome paging state for a single persona.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/GenomePagingState.ts")]
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
#[ts(export, export_to = "../../../shared/generated/persona/ActivateSkillResult.ts")]
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
// GENOME PAGING ENGINE
// =============================================================================

/// Per-persona genome paging engine.
/// Tracks adapter state, makes eviction/activation decisions.
#[derive(Debug)]
pub struct GenomePagingEngine {
    pub memory_budget_mb: f32,
    pub memory_used_mb: f32,
    /// Loaded adapters keyed by name
    active: HashMap<String, GenomeAdapterInfo>,
    /// Available (not loaded) adapters keyed by name
    available: HashMap<String, GenomeAdapterInfo>,
}

impl GenomePagingEngine {
    pub fn new(memory_budget_mb: f32) -> Self {
        Self {
            memory_budget_mb,
            memory_used_mb: 0.0,
            active: HashMap::new(),
            available: HashMap::new(),
        }
    }

    /// Sync full adapter state from TypeScript.
    /// Replaces both active and available maps entirely.
    pub fn sync_state(&mut self, adapters: Vec<GenomeAdapterInfo>) {
        self.active.clear();
        self.available.clear();
        self.memory_used_mb = 0.0;

        for adapter in adapters {
            if adapter.is_loaded {
                self.memory_used_mb += adapter.size_mb;
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
        self.active.insert(loaded.name.clone(), loaded);

        ActivateSkillResult {
            activated: true,
            adapter_name: skill_name.to_string(),
            evicted,
            to_load: Some(skill_name.to_string()),
            decision_time_us: start.elapsed().as_micros() as u64,
        }
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

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

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
mod tests {
    use super::*;

    fn make_adapter(name: &str, domain: &str, size_mb: f32, priority: f32, loaded: bool, last_used_ms: u64) -> GenomeAdapterInfo {
        GenomeAdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            size_mb,
            priority,
            is_loaded: loaded,
            last_used_ms,
            ollama_model_name: Some(format!("{}:7b", name)),
        }
    }

    // ── Eviction Scoring ──────────────────────────────────────────────

    #[test]
    fn test_critical_adapter_never_evicted() {
        let adapter = make_adapter("critical", "code", 50.0, 0.95, true, 0);
        let score = calculate_eviction_score_at(&adapter, 100_000);
        assert!(score.is_infinite(), "Critical adapter should return INFINITY");
    }

    #[test]
    fn test_eviction_score_formula() {
        // age = 60s, priority = 0.5 → score = 60 / (0.5 * 10) = 12.0
        let adapter = make_adapter("test", "code", 50.0, 0.5, true, 0);
        let score = calculate_eviction_score_at(&adapter, 60_000);
        assert!((score - 12.0).abs() < 0.001, "Score should be 12.0, got {score}");
    }

    #[test]
    fn test_eviction_score_higher_priority_harder_to_evict() {
        let low = make_adapter("low", "code", 50.0, 0.3, true, 0);
        let high = make_adapter("high", "code", 50.0, 0.8, true, 0);
        let now = 60_000;

        let low_score = calculate_eviction_score_at(&low, now);
        let high_score = calculate_eviction_score_at(&high, now);

        assert!(
            low_score > high_score,
            "Low priority ({low_score}) should have higher eviction score than high priority ({high_score})"
        );
    }

    #[test]
    fn test_eviction_score_older_more_evictable() {
        let old = make_adapter("old", "code", 50.0, 0.5, true, 0);
        let recent = make_adapter("recent", "code", 50.0, 0.5, true, 50_000);
        let now = 60_000;

        let old_score = calculate_eviction_score_at(&old, now);
        let recent_score = calculate_eviction_score_at(&recent, now);

        assert!(
            old_score > recent_score,
            "Older adapter ({old_score}) should be more evictable than recent ({recent_score})"
        );
    }

    #[test]
    fn test_eviction_score_just_used_is_zero() {
        let just_used = make_adapter("fresh", "code", 50.0, 0.5, true, 60_000);
        let score = calculate_eviction_score_at(&just_used, 60_000);
        assert!((score - 0.0).abs() < 0.001, "Just-used adapter should have score ≈ 0, got {score}");
    }

    // ── Engine: Cache Hit ─────────────────────────────────────────────

    #[test]
    fn test_activate_skill_cache_hit() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert("ts-expert".into(), make_adapter("ts-expert", "code", 50.0, 0.5, true, 1000));

        let result = engine.activate_skill("ts-expert", 2000);

        assert!(result.activated);
        assert_eq!(result.adapter_name, "ts-expert");
        assert!(result.evicted.is_empty());
        assert!(result.to_load.is_none(), "Cache hit should not need to load");
        // Verify last_used was updated
        assert_eq!(engine.active.get("ts-expert").unwrap().last_used_ms, 2000);
    }

    // ── Engine: Unknown Skill ─────────────────────────────────────────

    #[test]
    fn test_activate_unknown_skill() {
        let mut engine = GenomePagingEngine::new(200.0);
        let result = engine.activate_skill("nonexistent", 1000);

        assert!(!result.activated);
        assert!(result.to_load.is_none());
    }

    // ── Engine: Simple Load ───────────────────────────────────────────

    #[test]
    fn test_activate_skill_loads_from_available() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.available.insert("ts-expert".into(), make_adapter("ts-expert", "code", 50.0, 0.5, false, 0));

        let result = engine.activate_skill("ts-expert", 5000);

        assert!(result.activated);
        assert_eq!(result.to_load, Some("ts-expert".to_string()));
        assert!(result.evicted.is_empty());
        // Verify moved to active
        assert!(engine.active.contains_key("ts-expert"));
        assert!(!engine.available.contains_key("ts-expert"));
        assert!((engine.memory_used_mb - 50.0).abs() < 0.001);
    }

    // ── Engine: Eviction Required ─────────────────────────────────────

    #[test]
    fn test_activate_skill_evicts_lru_when_full() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Load two 50MB adapters (fills 100MB budget)
        engine.active.insert("old-adapter".into(), make_adapter("old-adapter", "chat", 50.0, 0.5, true, 1000));
        engine.active.insert("newer-adapter".into(), make_adapter("newer-adapter", "code", 50.0, 0.5, true, 5000));
        engine.memory_used_mb = 100.0;
        // Want to load a third
        engine.available.insert("incoming".into(), make_adapter("incoming", "creative", 50.0, 0.5, false, 0));

        let result = engine.activate_skill("incoming", 10_000);

        assert!(result.activated);
        assert_eq!(result.to_load, Some("incoming".to_string()));
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0], "old-adapter", "Should evict oldest (last_used=1000)");
        // old-adapter moved to available
        assert!(engine.available.contains_key("old-adapter"));
        assert!(!engine.active.contains_key("old-adapter"));
        // incoming now active
        assert!(engine.active.contains_key("incoming"));
        assert!((engine.memory_used_mb - 100.0).abs() < 0.001, "Should still be at 100MB");
    }

    #[test]
    fn test_activate_skill_evicts_multiple_if_needed() {
        let mut engine = GenomePagingEngine::new(200.0);
        // 4 × 50MB = 200MB (full)
        engine.active.insert("a1".into(), make_adapter("a1", "code", 50.0, 0.3, true, 1000));
        engine.active.insert("a2".into(), make_adapter("a2", "chat", 50.0, 0.4, true, 2000));
        engine.active.insert("a3".into(), make_adapter("a3", "creative", 50.0, 0.5, true, 3000));
        engine.active.insert("a4".into(), make_adapter("a4", "social", 50.0, 0.6, true, 4000));
        engine.memory_used_mb = 200.0;
        // Big adapter needs 120MB → need used + 120 <= 200 → need to free 120MB → 3 × 50MB
        engine.available.insert("big".into(), make_adapter("big", "analysis", 120.0, 0.5, false, 0));

        let result = engine.activate_skill("big", 10_000);

        assert!(result.activated);
        assert_eq!(result.evicted.len(), 3, "Should evict 3 adapters to free 150MB for 120MB incoming");
        // a1 (priority=0.3, oldest) should be first evicted
        assert!(result.evicted.contains(&"a1".to_string()), "a1 (priority=0.3, oldest) should be evicted");
    }

    #[test]
    fn test_critical_adapters_survive_eviction() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Critical adapter + normal adapter fill budget
        engine.active.insert("critical".into(), make_adapter("critical", "code", 50.0, 0.95, true, 1000));
        engine.active.insert("normal".into(), make_adapter("normal", "chat", 50.0, 0.5, true, 2000));
        engine.memory_used_mb = 100.0;
        engine.available.insert("incoming".into(), make_adapter("incoming", "creative", 50.0, 0.5, false, 0));

        let result = engine.activate_skill("incoming", 10_000);

        assert!(result.activated);
        assert_eq!(result.evicted, vec!["normal".to_string()]);
        assert!(engine.active.contains_key("critical"), "Critical adapter should survive");
    }

    // ── Engine: Sync State ────────────────────────────────────────────

    #[test]
    fn test_sync_state_replaces_all() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert("old".into(), make_adapter("old", "code", 50.0, 0.5, true, 1000));
        engine.memory_used_mb = 50.0;

        engine.sync_state(vec![
            make_adapter("new-active", "code", 60.0, 0.5, true, 5000),
            make_adapter("new-available", "chat", 40.0, 0.5, false, 0),
        ]);

        assert!(!engine.active.contains_key("old"));
        assert!(engine.active.contains_key("new-active"));
        assert!(engine.available.contains_key("new-available"));
        assert!((engine.memory_used_mb - 60.0).abs() < 0.001);
    }

    // ── Engine: Memory Pressure ───────────────────────────────────────

    #[test]
    fn test_memory_pressure_calculation() {
        let mut engine = GenomePagingEngine::new(200.0);
        assert!((engine.memory_pressure() - 0.0).abs() < 0.001);

        engine.memory_used_mb = 100.0;
        assert!((engine.memory_pressure() - 0.5).abs() < 0.001);

        engine.memory_used_mb = 200.0;
        assert!((engine.memory_pressure() - 1.0).abs() < 0.001);

        // Over budget capped at 1.0
        engine.memory_used_mb = 250.0;
        assert!((engine.memory_pressure() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_memory_pressure_zero_budget() {
        let engine = GenomePagingEngine::new(0.0);
        assert!((engine.memory_pressure() - 0.0).abs() < 0.001);
    }

    // ── Engine: State Snapshot ─────────────────────────────────────────

    #[test]
    fn test_state_snapshot() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert("loaded".into(), make_adapter("loaded", "code", 50.0, 0.5, true, 1000));
        engine.available.insert("disk".into(), make_adapter("disk", "chat", 40.0, 0.5, false, 0));
        engine.memory_used_mb = 50.0;

        let state = engine.state();

        assert!((state.memory_budget_mb - 200.0).abs() < 0.001);
        assert!((state.memory_used_mb - 50.0).abs() < 0.001);
        assert!((state.memory_pressure - 0.25).abs() < 0.001);
        assert_eq!(state.active_adapters.len(), 1);
        assert_eq!(state.available_adapters.len(), 1);
    }

    // ── Engine: Decision Time ─────────────────────────────────────────

    #[test]
    fn test_decision_time_is_fast() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.available.insert("test".into(), make_adapter("test", "code", 50.0, 0.5, false, 0));

        let result = engine.activate_skill("test", 1000);

        assert!(
            result.decision_time_us < 100,
            "Decision should be <100μs, was {}μs",
            result.decision_time_us
        );
    }

    // ── ts-rs binding tests ───────────────────────────────────────────

    #[test]
    fn export_bindings_genomeadapterinfo() {
        GenomeAdapterInfo::export_all().unwrap();
    }

    #[test]
    fn export_bindings_genomepagingstate() {
        GenomePagingState::export_all().unwrap();
    }

    #[test]
    fn export_bindings_activateskillresult() {
        ActivateSkillResult::export_all().unwrap();
    }
}
