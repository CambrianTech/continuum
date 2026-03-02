//! GpuMemoryManager — singleton VRAM tracker with Metal/CUDA detection.
//!
//! Budget allocation (startup defaults):
//!   Inference: 75%  — model weights, KV cache, LoRA adapters
//!   TTS:       10%  — TTS model weights
//!   Rendering: 10%  — Bevy render targets, avatar models
//!   Reserve:    5%  — headroom to prevent OOM
//!
//! Pressure levels:
//!   0-60%   Normal   — no action
//!   60-80%  Warning  — log warnings, genome evicts non-critical
//!   80-95%  High     — refuse new model loads, aggressive eviction
//!   95%+    Critical — refuse all allocations, force evictions

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use ts_rs::TS;

use crate::{log_info, log_error};

// =============================================================================
// SUBSYSTEM ENUM
// =============================================================================

/// GPU memory consumer categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuSubsystem {
    Rendering = 0,
    Inference = 1,
    Tts = 2,
}

impl GpuSubsystem {
    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rendering => "rendering",
            Self::Inference => "inference",
            Self::Tts => "tts",
        }
    }
}

// =============================================================================
// SUBSYSTEM BUDGET (lock-free)
// =============================================================================

/// Per-subsystem budget + usage tracking via atomics.
/// No mutex needed — AtomicU64 for concurrent reads/writes.
#[derive(Debug)]
struct SubsystemBudget {
    budget_bytes: AtomicU64,
    used_bytes: AtomicU64,
}

impl SubsystemBudget {
    fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes: AtomicU64::new(budget_bytes),
            used_bytes: AtomicU64::new(0),
        }
    }

    fn budget(&self) -> u64 {
        self.budget_bytes.load(Ordering::Relaxed)
    }

    fn used(&self) -> u64 {
        self.used_bytes.load(Ordering::Relaxed)
    }

    fn set_budget(&self, bytes: u64) {
        self.budget_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Try to allocate bytes. Returns true if within budget, false if over.
    /// Allocation proceeds even if over-budget (soft limit) — caller checks pressure.
    fn allocate(&self, bytes: u64) -> bool {
        let prev = self.used_bytes.fetch_add(bytes, Ordering::Relaxed);
        let budget = self.budget_bytes.load(Ordering::Relaxed);
        (prev + bytes) <= budget
    }

    fn release(&self, bytes: u64) {
        // Saturating subtract to prevent underflow
        self.used_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_sub(bytes))
        }).ok();
    }
}

// =============================================================================
// GPU MEMORY MANAGER
// =============================================================================

/// Budget allocation percentages (of usable VRAM after reserve).
const INFERENCE_BUDGET_PCT: f64 = 0.75;
const TTS_BUDGET_PCT: f64 = 0.10;
const RENDERING_BUDGET_PCT: f64 = 0.10;
const RESERVE_PCT: f64 = 0.05;

/// CPU-only fallback: use 25% of system RAM as "GPU" budget.
const CPU_FALLBACK_RAM_PCT: f64 = 0.25;

/// Pressure thresholds.
pub const PRESSURE_WARNING: f32 = 0.60;
pub const PRESSURE_HIGH: f32 = 0.80;
pub const PRESSURE_CRITICAL: f32 = 0.95;

pub struct GpuMemoryManager {
    total_vram_bytes: u64,
    gpu_name: String,
    subsystems: [SubsystemBudget; 3], // Rendering=0, Inference=1, Tts=2
    reserve_bytes: u64,
    pressure_tx: watch::Sender<f32>,
    pressure_rx: watch::Receiver<f32>,
}

impl std::fmt::Debug for GpuMemoryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuMemoryManager")
            .field("gpu_name", &self.gpu_name)
            .field("total_vram_mb", &(self.total_vram_bytes / (1024 * 1024)))
            .finish()
    }
}

impl GpuMemoryManager {
    /// Detect GPU and create manager with real VRAM budgets.
    /// Tries Metal (macOS), then CUDA, then falls back to CPU (fraction of RAM).
    pub fn detect() -> Self {
        let (total_bytes, gpu_name) = detect_gpu();

        let reserve_bytes = (total_bytes as f64 * RESERVE_PCT) as u64;
        let usable = total_bytes.saturating_sub(reserve_bytes);

        let inference_budget = (usable as f64 * INFERENCE_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;
        let tts_budget = (usable as f64 * TTS_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;
        let rendering_budget = (usable as f64 * RENDERING_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;

        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);

        let total_mb = total_bytes / (1024 * 1024);
        let inference_mb = inference_budget / (1024 * 1024);
        let tts_mb = tts_budget / (1024 * 1024);
        let rendering_mb = rendering_budget / (1024 * 1024);
        let reserve_mb = reserve_bytes / (1024 * 1024);

        log_info!("gpu", "manager",
            "GPU detected: {} — {}MB VRAM",
            gpu_name, total_mb
        );
        log_info!("gpu", "manager",
            "Budget: inference={}MB, tts={}MB, rendering={}MB, reserve={}MB",
            inference_mb, tts_mb, rendering_mb, reserve_mb
        );

        Self {
            total_vram_bytes: total_bytes,
            gpu_name,
            subsystems: [
                SubsystemBudget::new(rendering_budget), // index 0
                SubsystemBudget::new(inference_budget),  // index 1
                SubsystemBudget::new(tts_budget),        // index 2
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
        }
    }

    // ── Allocation ──────────────────────────────────────────────────────

    /// Allocate VRAM for a subsystem. Returns an RAII guard that releases on drop.
    /// Logs a warning if allocation exceeds budget (soft limit — does not reject).
    /// Returns Err only at CRITICAL pressure (>95%) to prevent OOM.
    pub fn allocate(
        self: &Arc<Self>,
        subsystem: GpuSubsystem,
        bytes: u64,
    ) -> Result<GpuAllocationGuard, GpuError> {
        let pressure = self.pressure();

        // Hard reject at critical pressure
        if pressure >= PRESSURE_CRITICAL {
            let mb = bytes as f64 / (1024.0 * 1024.0);
            log_error!("gpu", "manager",
                "CRITICAL: Rejecting {}MB allocation for {} (pressure={:.0}%)",
                mb, subsystem.name(), pressure * 100.0
            );
            return Err(GpuError::CriticalPressure {
                subsystem: subsystem.name(),
                requested_mb: mb,
                pressure,
            });
        }

        let within_budget = self.subsystems[subsystem.index()].allocate(bytes);
        let new_pressure = self.pressure();

        // Broadcast updated pressure
        let _ = self.pressure_tx.send(new_pressure);

        let mb = bytes as f64 / (1024.0 * 1024.0);
        if !within_budget {
            log_info!("gpu", "manager",
                "WARNING: {} allocation {:.0}MB exceeds budget (pressure={:.0}%)",
                subsystem.name(), mb, new_pressure * 100.0
            );
        } else {
            log_info!("gpu", "manager",
                "GPU: Allocated {:.0}MB for {} (pressure={:.0}%)",
                mb, subsystem.name(), new_pressure * 100.0
            );
        }

        Ok(GpuAllocationGuard {
            manager: Arc::clone(self),
            subsystem,
            bytes,
            released: false,
        })
    }

    /// Manual release (when RAII guard isn't suitable, e.g. non-Drop contexts).
    pub fn release(&self, subsystem: GpuSubsystem, bytes: u64) {
        self.subsystems[subsystem.index()].release(bytes);

        let pressure = self.pressure();
        let _ = self.pressure_tx.send(pressure);

        let mb = bytes as f64 / (1024.0 * 1024.0);
        log_info!("gpu", "manager",
            "GPU: Released {:.0}MB from {} (pressure={:.0}%)",
            mb, subsystem.name(), pressure * 100.0
        );
    }

    // ── Query ───────────────────────────────────────────────────────────

    /// Overall pressure: total_used / (total_vram - reserve). Range 0.0-1.0.
    pub fn pressure(&self) -> f32 {
        let usable = self.total_vram_bytes.saturating_sub(self.reserve_bytes);
        if usable == 0 {
            return 0.0;
        }
        let total_used: u64 = self.subsystems.iter().map(|s| s.used()).sum();
        (total_used as f64 / usable as f64).min(1.0) as f32
    }

    /// Subscribe to pressure updates (watch channel receiver).
    pub fn pressure_rx(&self) -> watch::Receiver<f32> {
        self.pressure_rx.clone()
    }

    /// Total detected VRAM in bytes.
    pub fn total_vram_bytes(&self) -> u64 {
        self.total_vram_bytes
    }

    /// GPU device name.
    pub fn gpu_name(&self) -> &str {
        &self.gpu_name
    }

    /// Inference subsystem budget in bytes.
    pub fn inference_budget_bytes(&self) -> u64 {
        self.subsystems[GpuSubsystem::Inference.index()].budget()
    }

    /// Inference subsystem budget in MB.
    pub fn inference_budget_mb(&self) -> f32 {
        self.inference_budget_bytes() as f32 / (1024.0 * 1024.0)
    }

    /// Per-persona inference budget: inference_budget / active_persona_count.
    pub fn per_persona_inference_budget_mb(&self, persona_count: usize) -> f32 {
        let count = persona_count.max(1) as f32;
        self.inference_budget_mb() / count
    }

    /// Set subsystem budget (for runtime rebalancing).
    pub fn set_budget(&self, subsystem: GpuSubsystem, bytes: u64) {
        self.subsystems[subsystem.index()].set_budget(bytes);
    }

    /// Full stats snapshot for IPC.
    pub fn stats(&self) -> GpuStats {
        let mb = |b: u64| b as f32 / (1024.0 * 1024.0);
        let total_used: u64 = self.subsystems.iter().map(|s| s.used()).sum();

        GpuStats {
            gpu_name: self.gpu_name.clone(),
            total_vram_mb: mb(self.total_vram_bytes),
            total_used_mb: mb(total_used),
            pressure: self.pressure(),
            rendering: SubsystemStats {
                budget_mb: mb(self.subsystems[0].budget()),
                used_mb: mb(self.subsystems[0].used()),
            },
            inference: SubsystemStats {
                budget_mb: mb(self.subsystems[1].budget()),
                used_mb: mb(self.subsystems[1].used()),
            },
            tts: SubsystemStats {
                budget_mb: mb(self.subsystems[2].budget()),
                used_mb: mb(self.subsystems[2].used()),
            },
            reserve_mb: mb(self.reserve_bytes),
        }
    }
}

// =============================================================================
// RAII ALLOCATION GUARD
// =============================================================================

/// RAII guard that releases GPU memory on drop.
/// Like SlotGuard in bevy_renderer.rs — deterministic cleanup.
pub struct GpuAllocationGuard {
    manager: Arc<GpuMemoryManager>,
    subsystem: GpuSubsystem,
    bytes: u64,
    released: bool,
}

impl GpuAllocationGuard {
    /// Manually release before drop (e.g., when ownership transfer is needed).
    pub fn release(mut self) {
        if !self.released {
            self.manager.release(self.subsystem, self.bytes);
            self.released = true;
        }
    }

    /// Bytes this guard is tracking.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Subsystem this guard belongs to.
    pub fn subsystem(&self) -> GpuSubsystem {
        self.subsystem
    }
}

impl Drop for GpuAllocationGuard {
    fn drop(&mut self) {
        if !self.released {
            self.manager.release(self.subsystem, self.bytes);
            self.released = true;
        }
    }
}

// =============================================================================
// ERROR TYPE
// =============================================================================

#[derive(Debug)]
pub enum GpuError {
    CriticalPressure {
        subsystem: &'static str,
        requested_mb: f64,
        pressure: f32,
    },
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CriticalPressure { subsystem, requested_mb, pressure } => {
                write!(
                    f,
                    "GPU critical pressure ({:.0}%): cannot allocate {:.0}MB for {}",
                    pressure * 100.0, requested_mb, subsystem
                )
            }
        }
    }
}

impl std::error::Error for GpuError {}

// =============================================================================
// STATS (ts-rs exported)
// =============================================================================

/// Per-subsystem stats for IPC response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/SubsystemStats.ts")]
pub struct SubsystemStats {
    #[ts(type = "number")]
    pub budget_mb: f32,
    #[ts(type = "number")]
    pub used_mb: f32,
}

/// Full GPU stats snapshot — returned by `gpu/stats` IPC command.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/GpuStats.ts")]
pub struct GpuStats {
    pub gpu_name: String,
    #[ts(type = "number")]
    pub total_vram_mb: f32,
    #[ts(type = "number")]
    pub total_used_mb: f32,
    pub pressure: f32,
    pub rendering: SubsystemStats,
    pub inference: SubsystemStats,
    pub tts: SubsystemStats,
    #[ts(type = "number")]
    pub reserve_mb: f32,
}

// =============================================================================
// GPU DETECTION
// =============================================================================

/// Detect GPU and return (total_bytes, device_name).
fn detect_gpu() -> (u64, String) {
    // Try Metal on macOS
    #[cfg(target_os = "macos")]
    {
        if let Some(result) = detect_metal() {
            return result;
        }
    }

    // Try CUDA
    #[cfg(feature = "cuda")]
    {
        if let Some(result) = detect_cuda() {
            return result;
        }
    }

    // CPU fallback
    detect_cpu_fallback()
}

/// Metal detection via metal-rs crate.
/// `recommendedMaxWorkingSetSize` returns the GPU's recommended VRAM budget.
#[cfg(target_os = "macos")]
fn detect_metal() -> Option<(u64, String)> {
    let device = metal::Device::system_default()?;
    let total = device.recommended_max_working_set_size();
    let name = device.name().to_string();

    if total == 0 {
        return None;
    }

    Some((total, name))
}

/// CUDA detection via candle's device info.
#[cfg(feature = "cuda")]
fn detect_cuda() -> Option<(u64, String)> {
    // candle_core doesn't expose device memory directly.
    // Use cudarc if available, otherwise estimate from device properties.
    // For now, we'll try to read from nvidia-smi output.
    use std::process::Command;

    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total,name", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    let stdout = String::from_utf8(output.stdout).ok()?;
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(", ").collect();
    if parts.len() < 2 {
        return None;
    }

    // nvidia-smi reports in MiB
    let total_mib: u64 = parts[0].trim().parse().ok()?;
    let name = parts[1].trim().to_string();
    let total_bytes = total_mib * 1024 * 1024;

    Some((total_bytes, name))
}

/// CPU fallback: use 25% of system RAM.
fn detect_cpu_fallback() -> (u64, String) {
    let total_ram = get_system_ram();
    let budget = (total_ram as f64 * CPU_FALLBACK_RAM_PCT) as u64;

    log_info!("gpu", "manager",
        "No GPU detected — using CPU fallback: {}MB of {}MB system RAM",
        budget / (1024 * 1024),
        total_ram / (1024 * 1024)
    );

    (budget, "CPU (no GPU)".to_string())
}

/// Get total system RAM.
#[cfg(target_os = "macos")]
fn get_system_ram() -> u64 {
    // sysctl hw.memsize returns total physical memory
    use std::process::Command;

    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output();

    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.trim().parse::<u64>().unwrap_or(8 * 1024 * 1024 * 1024) // 8GB fallback
        }
        Err(_) => 8 * 1024 * 1024 * 1024,
    }
}

#[cfg(not(target_os = "macos"))]
fn get_system_ram() -> u64 {
    // Linux: read /proc/meminfo
    use std::fs;

    if let Ok(contents) = fs::read_to_string("/proc/meminfo") {
        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                // Format: "MemTotal:       16384000 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb * 1024; // kB to bytes
                    }
                }
            }
        }
    }

    8 * 1024 * 1024 * 1024 // 8GB fallback
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(total_mb: u64) -> Arc<GpuMemoryManager> {
        let total_bytes = total_mb * 1024 * 1024;
        let reserve_bytes = (total_bytes as f64 * RESERVE_PCT) as u64;
        let usable = total_bytes - reserve_bytes;

        let inference_budget = (usable as f64 * INFERENCE_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;
        let tts_budget = (usable as f64 * TTS_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;
        let rendering_budget = (usable as f64 * RENDERING_BUDGET_PCT / (1.0 - RESERVE_PCT)) as u64;

        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);

        Arc::new(GpuMemoryManager {
            total_vram_bytes: total_bytes,
            gpu_name: "Test GPU".to_string(),
            subsystems: [
                SubsystemBudget::new(rendering_budget),
                SubsystemBudget::new(inference_budget),
                SubsystemBudget::new(tts_budget),
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
        })
    }

    #[test]
    fn test_detect_returns_nonzero() {
        let (total, name) = detect_gpu();
        assert!(total > 0, "Total VRAM should be > 0");
        assert!(!name.is_empty(), "GPU name should not be empty");
    }

    #[test]
    fn test_initial_pressure_zero() {
        let mgr = test_manager(36_864); // 36GB like M3 Max
        assert!((mgr.pressure() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_allocate_and_release() {
        let mgr = test_manager(1024); // 1GB
        let inference_budget = mgr.inference_budget_bytes();

        // Allocate half the inference budget
        let half = inference_budget / 2;
        let guard = mgr.allocate(GpuSubsystem::Inference, half).unwrap();
        assert!(mgr.pressure() > 0.0);

        // Release
        drop(guard);
        assert!((mgr.pressure() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_raii_guard_releases_on_drop() {
        let mgr = test_manager(1024);
        let used_before = mgr.subsystems[GpuSubsystem::Inference.index()].used();

        {
            let _guard = mgr.allocate(GpuSubsystem::Inference, 100 * 1024 * 1024).unwrap();
            let used_during = mgr.subsystems[GpuSubsystem::Inference.index()].used();
            assert_eq!(used_during, used_before + 100 * 1024 * 1024);
        }

        let used_after = mgr.subsystems[GpuSubsystem::Inference.index()].used();
        assert_eq!(used_after, used_before);
    }

    #[test]
    fn test_manual_release() {
        let mgr = test_manager(1024);
        let guard = mgr.allocate(GpuSubsystem::Tts, 50 * 1024 * 1024).unwrap();
        assert!(mgr.subsystems[GpuSubsystem::Tts.index()].used() > 0);

        guard.release();
        assert_eq!(mgr.subsystems[GpuSubsystem::Tts.index()].used(), 0);
    }

    #[test]
    fn test_budget_percentages() {
        let mgr = test_manager(36_864); // 36GB
        let stats = mgr.stats();

        // Inference should be ~75% of usable
        let inference_pct = stats.inference.budget_mb / stats.total_vram_mb;
        assert!(inference_pct > 0.70 && inference_pct < 0.80,
            "Inference should be ~75%, got {:.1}%", inference_pct * 100.0);

        // TTS should be ~10%
        let tts_pct = stats.tts.budget_mb / stats.total_vram_mb;
        assert!(tts_pct > 0.08 && tts_pct < 0.12,
            "TTS should be ~10%, got {:.1}%", tts_pct * 100.0);

        // Rendering should be ~10%
        let rendering_pct = stats.rendering.budget_mb / stats.total_vram_mb;
        assert!(rendering_pct > 0.08 && rendering_pct < 0.12,
            "Rendering should be ~10%, got {:.1}%", rendering_pct * 100.0);
    }

    #[test]
    fn test_per_persona_budget() {
        let mgr = test_manager(36_864); // 36GB
        let total_inference = mgr.inference_budget_mb();

        let per_1 = mgr.per_persona_inference_budget_mb(1);
        assert!((per_1 - total_inference).abs() < 1.0);

        let per_5 = mgr.per_persona_inference_budget_mb(5);
        assert!((per_5 - total_inference / 5.0).abs() < 1.0);

        // 0 personas treated as 1
        let per_0 = mgr.per_persona_inference_budget_mb(0);
        assert!((per_0 - total_inference).abs() < 1.0);
    }

    #[test]
    fn test_stats_snapshot() {
        let mgr = test_manager(1024);
        let _guard = mgr.allocate(GpuSubsystem::Inference, 100 * 1024 * 1024).unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.gpu_name, "Test GPU");
        assert!((stats.total_vram_mb - 1024.0).abs() < 1.0);
        assert!(stats.inference.used_mb > 99.0);
        assert!(stats.pressure > 0.0);
    }

    #[test]
    fn test_pressure_watch_channel() {
        let mgr = test_manager(1024);
        let rx = mgr.pressure_rx();

        // Initial pressure is 0
        assert!(*rx.borrow() < 0.01);

        // Allocate should update pressure
        let _guard = mgr.allocate(GpuSubsystem::Inference, 500 * 1024 * 1024).unwrap();
        assert!(rx.has_changed().unwrap_or(false) || *rx.borrow() > 0.0);
    }

    #[test]
    fn test_over_budget_soft_limit() {
        let mgr = test_manager(1024);
        let budget = mgr.inference_budget_bytes();

        // Allocate more than budget — should succeed (soft limit)
        let guard = mgr.allocate(GpuSubsystem::Inference, budget + 100 * 1024 * 1024);
        assert!(guard.is_ok(), "Over-budget allocation should succeed (soft limit)");
    }

    #[test]
    fn test_release_saturating() {
        let mgr = test_manager(1024);
        // Release without prior allocation — should not underflow
        mgr.release(GpuSubsystem::Inference, 999 * 1024 * 1024);
        assert_eq!(mgr.subsystems[GpuSubsystem::Inference.index()].used(), 0);
    }

    #[test]
    fn test_multiple_subsystem_pressure() {
        let mgr = test_manager(1024);

        let _g1 = mgr.allocate(GpuSubsystem::Rendering, 50 * 1024 * 1024).unwrap();
        let _g2 = mgr.allocate(GpuSubsystem::Inference, 200 * 1024 * 1024).unwrap();
        let _g3 = mgr.allocate(GpuSubsystem::Tts, 30 * 1024 * 1024).unwrap();

        let stats = mgr.stats();
        assert!(stats.rendering.used_mb > 49.0);
        assert!(stats.inference.used_mb > 199.0);
        assert!(stats.tts.used_mb > 29.0);

        // Total pressure from all subsystems
        let expected_total = 280.0; // 50+200+30
        assert!(stats.total_used_mb > expected_total - 1.0);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────────

    #[test]
    fn export_bindings_gpu_stats() {
        let cfg = ts_rs::Config::default();
        GpuStats::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_subsystem_stats() {
        let cfg = ts_rs::Config::default();
        SubsystemStats::export_all(&cfg).unwrap();
    }
}
