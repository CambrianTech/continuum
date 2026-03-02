//! EvictionRegistry — Read-only visibility into GPU consumers for eviction decisions.
//!
//! The GpuGovernor (Layer 3, TypeScript) queries this registry to understand what's
//! loaded, how much VRAM it uses, when it was last used, and what priority it has.
//! This enables informed eviction recommendations without the registry itself
//! performing any eviction — that's future work requiring unload callbacks.
//!
//! ## Eviction Score
//!
//! `age_seconds / (priority_weight * 10)`
//!
//! Lower priority × older = higher score = evict first.
//! Realtime entries have infinite weight → score = 0 → never evictable.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use ts_rs::TS;

use super::memory_manager::GpuPriority;

// =============================================================================
// ENTRY
// =============================================================================

/// A registered GPU consumer visible to the eviction system.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/EvictableEntry.ts")]
pub struct EvictableEntry {
    /// Unique identifier (e.g., "candle:llama-3.2-3b", "tts:kokoro", "embed:bge-small")
    pub id: String,
    /// Human-readable label for display
    pub label: String,
    /// Priority level this consumer was allocated at
    pub priority: GpuPriority,
    /// VRAM bytes consumed
    #[ts(type = "number")]
    pub bytes: u64,
    /// Timestamp of initial allocation (ms since epoch)
    #[ts(type = "number")]
    pub allocated_at_ms: u64,
    /// Timestamp of last use (ms since epoch) — updated via touch()
    #[ts(type = "number")]
    pub last_used_ms: u64,
    /// Whether this consumer can be evicted (Realtime = false)
    pub evictable: bool,
}

impl EvictableEntry {
    /// Eviction score: higher = evict sooner.
    /// age_seconds / (priority_weight * 10)
    /// Realtime (infinite weight) → 0.0 → never evicted.
    pub fn eviction_score(&self) -> f64 {
        if !self.evictable {
            return 0.0;
        }
        let weight = self.priority.eviction_weight();
        if weight.is_infinite() {
            return 0.0;
        }
        let now_ms = now_ms();
        let age_seconds = (now_ms.saturating_sub(self.last_used_ms)) as f64 / 1000.0;
        age_seconds / (weight as f64 * 10.0)
    }
}

// =============================================================================
// SNAPSHOT (ts-rs exported)
// =============================================================================

/// Full registry snapshot for IPC.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/EvictionRegistrySnapshot.ts")]
pub struct EvictionRegistrySnapshot {
    /// All registered entries
    pub entries: Vec<EvictableEntry>,
    /// Total VRAM bytes tracked by registered consumers
    #[ts(type = "number")]
    pub total_tracked_bytes: u64,
    /// Number of evictable entries
    #[ts(type = "number")]
    pub evictable_count: u32,
}

// =============================================================================
// REGISTRY
// =============================================================================

/// Read-only registry of GPU consumers for visibility and eviction scoring.
///
/// Thread-safe via Mutex. Low contention — register/unregister are rare events
/// (model load/unload), touch() is called on inference use but is fast (HashMap lookup).
pub struct EvictionRegistry {
    entries: Mutex<HashMap<String, EvictableEntry>>,
}

impl EvictionRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register a GPU consumer. Replaces existing entry with same id.
    pub fn register(&self, entry: EvictableEntry) {
        if let Ok(mut map) = self.entries.lock() {
            map.insert(entry.id.clone(), entry);
        }
    }

    /// Remove a consumer (e.g., model unloaded).
    pub fn unregister(&self, id: &str) {
        if let Ok(mut map) = self.entries.lock() {
            map.remove(id);
        }
    }

    /// Update last_used timestamp — call on every inference/TTS use.
    pub fn touch(&self, id: &str) {
        if let Ok(mut map) = self.entries.lock() {
            if let Some(entry) = map.get_mut(id) {
                entry.last_used_ms = now_ms();
            }
        }
    }

    /// Eviction candidates sorted by eviction score (highest first = evict first).
    /// Excludes non-evictable entries (Realtime).
    pub fn candidates(&self) -> Vec<EvictableEntry> {
        let map = match self.entries.lock() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut candidates: Vec<EvictableEntry> = map.values()
            .filter(|e| e.evictable)
            .cloned()
            .collect();
        candidates.sort_by(|a, b| {
            b.eviction_score().partial_cmp(&a.eviction_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates
    }

    /// Full snapshot for IPC.
    pub fn snapshot(&self) -> EvictionRegistrySnapshot {
        let map = match self.entries.lock() {
            Ok(m) => m,
            Err(_) => return EvictionRegistrySnapshot {
                entries: Vec::new(),
                total_tracked_bytes: 0,
                evictable_count: 0,
            },
        };
        let entries: Vec<EvictableEntry> = map.values().cloned().collect();
        let total_tracked_bytes = entries.iter().map(|e| e.bytes).sum();
        let evictable_count = entries.iter().filter(|e| e.evictable).count() as u32;
        EvictionRegistrySnapshot {
            entries,
            total_tracked_bytes,
            evictable_count,
        }
    }

    /// Number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|m| m.len()).unwrap_or(0)
    }
}

impl std::fmt::Debug for EvictionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvictionRegistry")
            .field("entries", &self.len())
            .finish()
    }
}

// =============================================================================
// HELPERS
// =============================================================================

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Create an EvictableEntry with sensible defaults.
pub fn make_entry(id: &str, label: &str, priority: GpuPriority, bytes: u64) -> EvictableEntry {
    let now = now_ms();
    EvictableEntry {
        id: id.to_string(),
        label: label.to_string(),
        priority,
        bytes,
        allocated_at_ms: now,
        last_used_ms: now,
        evictable: priority != GpuPriority::Realtime,
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_snapshot() {
        let reg = EvictionRegistry::new();
        assert_eq!(reg.len(), 0);

        reg.register(make_entry("model:llama", "Llama 3.2", GpuPriority::Interactive, 3_000_000_000));
        reg.register(make_entry("tts:kokoro", "Kokoro TTS", GpuPriority::Interactive, 500_000_000));

        assert_eq!(reg.len(), 2);

        let snap = reg.snapshot();
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.total_tracked_bytes, 3_500_000_000);
        assert_eq!(snap.evictable_count, 2);
    }

    #[test]
    fn test_unregister() {
        let reg = EvictionRegistry::new();
        reg.register(make_entry("model:test", "Test", GpuPriority::Interactive, 1000));
        assert_eq!(reg.len(), 1);

        reg.unregister("model:test");
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_touch_updates_last_used() {
        let reg = EvictionRegistry::new();
        let mut entry = make_entry("model:test", "Test", GpuPriority::Interactive, 1000);
        entry.last_used_ms = 1000; // Artificial old timestamp
        reg.register(entry);

        reg.touch("model:test");

        let snap = reg.snapshot();
        assert!(snap.entries[0].last_used_ms > 1000);
    }

    #[test]
    fn test_realtime_not_evictable() {
        let reg = EvictionRegistry::new();
        reg.register(make_entry("render:targets", "Render Targets", GpuPriority::Realtime, 100_000_000));
        reg.register(make_entry("model:llama", "Llama", GpuPriority::Interactive, 3_000_000_000));

        let candidates = reg.candidates();
        assert_eq!(candidates.len(), 1, "Realtime should not appear in candidates");
        assert_eq!(candidates[0].id, "model:llama");
    }

    #[test]
    fn test_eviction_score_batch_higher_than_interactive() {
        // Batch (weight 0.2) should have higher eviction score than Interactive (weight 0.7)
        // at the same age
        let mut batch = make_entry("train:lora", "LoRA Training", GpuPriority::Batch, 1000);
        batch.last_used_ms = batch.allocated_at_ms - 60_000; // 60s old

        let mut interactive = make_entry("model:llama", "Llama", GpuPriority::Interactive, 1000);
        interactive.last_used_ms = interactive.allocated_at_ms - 60_000; // same age

        assert!(batch.eviction_score() > interactive.eviction_score(),
            "Batch (score={:.1}) should score higher than Interactive (score={:.1})",
            batch.eviction_score(), interactive.eviction_score());
    }

    #[test]
    fn test_candidates_sorted_by_score_descending() {
        let reg = EvictionRegistry::new();

        let mut batch = make_entry("batch:train", "Training", GpuPriority::Batch, 1000);
        batch.last_used_ms = batch.allocated_at_ms - 120_000; // 2min old

        let mut bg = make_entry("bg:rebuild", "Rebuild", GpuPriority::Background, 1000);
        bg.last_used_ms = bg.allocated_at_ms - 120_000; // same age

        let mut interactive = make_entry("model:llama", "Llama", GpuPriority::Interactive, 1000);
        interactive.last_used_ms = interactive.allocated_at_ms - 120_000; // same age

        reg.register(interactive);
        reg.register(bg);
        reg.register(batch);

        let candidates = reg.candidates();
        assert_eq!(candidates.len(), 3);
        // Batch evicts first (lowest weight), then Background, then Interactive
        assert_eq!(candidates[0].id, "batch:train");
        assert_eq!(candidates[1].id, "bg:rebuild");
        assert_eq!(candidates[2].id, "model:llama");
    }

    #[test]
    fn test_replace_entry_on_duplicate_id() {
        let reg = EvictionRegistry::new();
        reg.register(make_entry("model:llama", "Llama v1", GpuPriority::Interactive, 1000));
        reg.register(make_entry("model:llama", "Llama v2", GpuPriority::Interactive, 2000));

        assert_eq!(reg.len(), 1);
        let snap = reg.snapshot();
        assert_eq!(snap.entries[0].label, "Llama v2");
        assert_eq!(snap.entries[0].bytes, 2000);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────

    #[test]
    fn export_bindings_evictable_entry() {
        let cfg = ts_rs::Config::default();
        EvictableEntry::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_eviction_registry_snapshot() {
        let cfg = ts_rs::Config::default();
        EvictionRegistrySnapshot::export_all(&cfg).unwrap();
    }
}
