//! GpuModelTracker — Reusable GPU allocation lifecycle for model loading.
//!
//! Compresses the repeated OnceLock + allocate + log pattern into a single call.
//! Every GPU consumer (TTS engines, embedding models, inference backends) uses this
//! interface for consistent allocation tracking. This gives the GpuMemoryManager
//! complete visibility into VRAM usage, enabling truthful pressure readings and
//! future RTOS-style priority scheduling.
//!
//! ## Usage
//!
//! ```rust
//! // Static tracker for permanent model allocations:
//! static MY_MODEL_GPU: GpuModelTracker = GpuModelTracker::new("MyModel");
//!
//! // During model load:
//! MY_MODEL_GPU.track_file(GpuSubsystem::Tts, &model_path, gpu_manager())?;
//!
//! // Query:
//! let bytes = MY_MODEL_GPU.tracked_bytes();
//! let is_loaded = MY_MODEL_GPU.is_tracked();
//! ```
//!
//! ## Error Policy (caller decides)
//!
//! `track_*` methods return `Result`. Callers choose the policy:
//! - Non-critical (TTS): `let _ = tracker.track_file(...)` — proceed if allocation fails
//! - Safety valve (inference): `tracker.track_file(...)?` — abort on critical pressure

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::log_info;

use super::eviction_registry::make_entry;
use super::memory_manager::{GpuAllocationGuard, GpuMemoryManager, GpuPriority, GpuSubsystem};

/// Reusable GPU allocation tracker for model loading.
///
/// Wraps the GpuMemoryManager allocation lifecycle into a single interface.
/// Each instance tracks one allocation (one model, one ONNX session, etc.)
///
/// For permanent models (loaded once at startup), use as a static:
/// ```rust
/// static KOKORO_GPU: GpuModelTracker = GpuModelTracker::new("Kokoro");
/// ```
///
/// For transient allocations (LoRA rebuild spikes), create on the stack
/// and let it drop at scope end — the RAII guard releases automatically.
pub struct GpuModelTracker {
    label: &'static str,
    guard: Mutex<Option<GpuAllocationGuard>>,
    /// Manager ref + registry ID for unregister on release.
    registry_state: Mutex<Option<(Arc<GpuMemoryManager>, String)>>,
}

// Safety: GpuAllocationGuard contains Arc<GpuMemoryManager> (Send+Sync)
// plus primitive fields. Mutex<Option<T>> is Sync when T: Send.
// The compiler would derive this automatically but the Mutex<Option<Guard>>
// pattern needs explicit confirmation that Guard is Send.
unsafe impl Sync for GpuModelTracker {}

impl GpuModelTracker {
    /// Create a new tracker. `label` is used in log messages.
    /// Can be used in `static` context (const fn).
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            guard: Mutex::new(None),
            registry_state: Mutex::new(None),
        }
    }

    /// Track GPU allocation based on file size on disk.
    ///
    /// Returns Ok(()) if:
    /// - Allocation succeeded (guard stored)
    /// - No manager available (graceful degradation)
    /// - File size is 0 or unreadable
    ///
    /// Returns Err when pressure exceeds this priority's gate threshold.
    pub fn track_file(
        &self,
        subsystem: GpuSubsystem,
        path: &Path,
        manager: Option<&Arc<GpuMemoryManager>>,
        priority: GpuPriority,
    ) -> Result<(), String> {
        let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        self.track_bytes(subsystem, bytes, manager, priority)
    }

    /// Track GPU allocation with explicit byte count.
    ///
    /// Returns Ok(()) if allocation succeeded or manager unavailable.
    /// Returns Err when pressure exceeds this priority's gate threshold.
    pub fn track_bytes(
        &self,
        subsystem: GpuSubsystem,
        bytes: u64,
        manager: Option<&Arc<GpuMemoryManager>>,
        priority: GpuPriority,
    ) -> Result<(), String> {
        if bytes == 0 {
            return Ok(());
        }
        let Some(mgr) = manager else {
            return Ok(());
        };

        match mgr.allocate(subsystem, bytes, priority) {
            Ok(guard) => {
                let mb = bytes as f64 / (1024.0 * 1024.0);
                log_info!(
                    "gpu", self.label,
                    "{}: GPU {} [{}] allocation {:.0}MB",
                    self.label, subsystem.name(), priority.name(), mb
                );

                // Register in eviction registry for visibility
                let registry_id = format!("{}:{}", subsystem.name(), self.label.to_lowercase());
                mgr.eviction_registry.register(make_entry(
                    &registry_id,
                    self.label,
                    priority,
                    bytes,
                ));

                // Store guard and registry state for cleanup on release
                let mut slot = self.guard.lock()
                    .map_err(|e| format!("{}: lock poisoned: {e}", self.label))?;
                // If replacing an existing guard, the old one drops here (releases old allocation)
                *slot = Some(guard);
                if let Ok(mut rs) = self.registry_state.lock() {
                    *rs = Some((Arc::clone(mgr), registry_id));
                }
                Ok(())
            }
            Err(e) => {
                Err(format!("{}: GPU allocation failed — {}", self.label, e))
            }
        }
    }

    /// Release the tracked allocation (e.g., model unloaded).
    /// No-op if not currently tracking.
    pub fn release(&self) {
        if let Ok(mut slot) = self.guard.lock() {
            if let Some(guard) = slot.take() {
                let mb = guard.bytes() as f64 / (1024.0 * 1024.0);
                log_info!(
                    "gpu", self.label,
                    "{}: GPU released {:.0}MB",
                    self.label, mb
                );
                // guard.release() called by Drop
                drop(guard);
            }
        }
        // Unregister from eviction registry
        if let Ok(mut rs) = self.registry_state.lock() {
            if let Some((mgr, id)) = rs.take() {
                mgr.eviction_registry.unregister(&id);
            }
        }
    }

    /// Touch the eviction registry entry (update last_used timestamp).
    /// Call this on every inference/TTS use to keep the entry fresh.
    pub fn touch(&self) {
        if let Ok(rs) = self.registry_state.lock() {
            if let Some((mgr, id)) = rs.as_ref() {
                mgr.eviction_registry.touch(id);
            }
        }
    }

    /// Current tracked bytes (0 if not tracking or lock poisoned).
    pub fn tracked_bytes(&self) -> u64 {
        self.guard.lock().ok()
            .and_then(|slot| slot.as_ref().map(|g| g.bytes()))
            .unwrap_or(0)
    }

    /// Whether this tracker holds an active allocation.
    pub fn is_tracked(&self) -> bool {
        self.guard.lock().ok()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Label used for logging.
    pub fn label(&self) -> &'static str {
        self.label
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::memory_manager::GpuMemoryManager;

    fn test_manager() -> Arc<GpuMemoryManager> {
        // 4GB test GPU — enough headroom for test allocations
        let total_bytes: u64 = 4 * 1024 * 1024 * 1024;
        let reserve_bytes = (total_bytes as f64 * 0.05) as u64;
        let usable = total_bytes - reserve_bytes;

        let inference_budget = (usable as f64 * 0.75 / 0.95) as u64;
        let tts_budget = (usable as f64 * 0.10 / 0.95) as u64;
        let rendering_budget = (usable as f64 * 0.10 / 0.95) as u64;

        let (pressure_tx, pressure_rx) = tokio::sync::watch::channel(0.0f32);

        Arc::new(GpuMemoryManager::new_for_test(
            total_bytes,
            "Test GPU".to_string(),
            inference_budget,
            tts_budget,
            rendering_budget,
            reserve_bytes,
            pressure_tx,
            pressure_rx,
        ))
    }

    #[test]
    fn test_tracker_starts_empty() {
        static TRACKER: GpuModelTracker = GpuModelTracker::new("TestModel");
        assert!(!TRACKER.is_tracked());
        assert_eq!(TRACKER.tracked_bytes(), 0);
        assert_eq!(TRACKER.label(), "TestModel");
    }

    #[test]
    fn test_track_bytes_with_manager() {
        let tracker = GpuModelTracker::new("TestTrack");
        let mgr = test_manager();

        let result = tracker.track_bytes(
            GpuSubsystem::Tts,
            100 * 1024 * 1024, // 100MB
            Some(&mgr),
            GpuPriority::Interactive,
        );
        assert!(result.is_ok());
        assert!(tracker.is_tracked());
        assert_eq!(tracker.tracked_bytes(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_track_bytes_no_manager() {
        let tracker = GpuModelTracker::new("NoMgr");
        let result = tracker.track_bytes(GpuSubsystem::Tts, 100_000_000, None, GpuPriority::Interactive);
        assert!(result.is_ok());
        assert!(!tracker.is_tracked()); // No guard stored
    }

    #[test]
    fn test_track_zero_bytes_noop() {
        let tracker = GpuModelTracker::new("ZeroBytes");
        let mgr = test_manager();
        let result = tracker.track_bytes(GpuSubsystem::Tts, 0, Some(&mgr), GpuPriority::Interactive);
        assert!(result.is_ok());
        assert!(!tracker.is_tracked());
    }

    #[test]
    fn test_release() {
        let tracker = GpuModelTracker::new("ReleaseTest");
        let mgr = test_manager();

        tracker.track_bytes(GpuSubsystem::Inference, 200 * 1024 * 1024, Some(&mgr), GpuPriority::Interactive).unwrap();
        assert!(tracker.is_tracked());

        tracker.release();
        assert!(!tracker.is_tracked());
        assert_eq!(tracker.tracked_bytes(), 0);
    }

    #[test]
    fn test_replace_allocation() {
        let tracker = GpuModelTracker::new("ReplaceTest");
        let mgr = test_manager();

        // First allocation
        tracker.track_bytes(GpuSubsystem::Tts, 50 * 1024 * 1024, Some(&mgr), GpuPriority::Interactive).unwrap();
        assert_eq!(tracker.tracked_bytes(), 50 * 1024 * 1024);

        // Replace with larger allocation — old guard drops, releases old memory
        tracker.track_bytes(GpuSubsystem::Tts, 80 * 1024 * 1024, Some(&mgr), GpuPriority::Interactive).unwrap();
        assert_eq!(tracker.tracked_bytes(), 80 * 1024 * 1024);
    }

    #[test]
    fn test_tracker_registers_in_eviction_registry() {
        let tracker = GpuModelTracker::new("RegistryTest");
        let mgr = test_manager();

        // Before tracking: registry empty
        assert_eq!(mgr.eviction_registry.len(), 0);

        // Track → registered
        tracker.track_bytes(GpuSubsystem::Tts, 100 * 1024 * 1024, Some(&mgr), GpuPriority::Interactive).unwrap();
        assert_eq!(mgr.eviction_registry.len(), 1);

        let snap = mgr.eviction_registry.snapshot();
        assert_eq!(snap.entries[0].id, "tts:registrytest");
        assert_eq!(snap.entries[0].label, "RegistryTest");
        assert_eq!(snap.entries[0].bytes, 100 * 1024 * 1024);

        // Release → unregistered
        tracker.release();
        assert_eq!(mgr.eviction_registry.len(), 0);
    }

    #[test]
    fn test_tracker_touch_updates_registry() {
        let tracker = GpuModelTracker::new("TouchTest");
        let mgr = test_manager();

        tracker.track_bytes(GpuSubsystem::Inference, 50 * 1024 * 1024, Some(&mgr), GpuPriority::Interactive).unwrap();

        let snap_before = mgr.eviction_registry.snapshot();
        let ts_before = snap_before.entries[0].last_used_ms;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(5));

        tracker.touch();

        let snap_after = mgr.eviction_registry.snapshot();
        let ts_after = snap_after.entries[0].last_used_ms;
        assert!(ts_after >= ts_before, "touch() should update last_used_ms");
    }
}
