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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use ts_rs::TS;

use crate::{log_info, log_error};
use super::eviction_registry::EvictionRegistry;

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

    pub fn name(self) -> &'static str {
        match self {
            Self::Rendering => "rendering",
            Self::Inference => "inference",
            Self::Tts => "tts",
        }
    }

    /// Parse subsystem from string name (as sent by TypeScript IPC).
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "rendering" => Some(Self::Rendering),
            "inference" => Some(Self::Inference),
            "tts" => Some(Self::Tts),
            _ => None,
        }
    }
}

// =============================================================================
// GPU PRIORITY (RTOS-style interrupt levels)
// =============================================================================

/// Priority levels for GPU allocations — RTOS-style scheduling.
///
/// Higher priority = higher pressure gate = harder to reject.
/// Realtime (render loop, audio) only stops at OOM.
/// Batch (training) yields the bus when anyone else needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/GpuPriority.ts")]
pub enum GpuPriority {
    /// Render loop, audio pipeline — only OOM stops it
    Realtime = 0,
    /// User-facing inference, TTS, embeddings
    Interactive = 1,
    /// LoRA rebuild spikes, adapter pre-load
    Background = 2,
    /// Training, conversion — lowest priority, yields first
    Batch = 3,
}

impl GpuPriority {
    /// Pressure threshold at which this priority level gets rejected.
    /// Lower priority = lower gate = rejected sooner.
    pub fn pressure_gate(self) -> f32 {
        match self {
            Self::Realtime    => PRESSURE_CRITICAL,  // 0.95 — only OOM
            Self::Interactive => PRESSURE_HIGH,       // 0.80
            Self::Background  => PRESSURE_WARNING,    // 0.60
            Self::Batch       => 0.50,                // yields at 50%
        }
    }

    /// Weight for eviction scoring — lower priority evicts first.
    pub fn eviction_weight(self) -> f32 {
        match self {
            Self::Realtime    => f32::INFINITY, // never evictable
            Self::Interactive => 0.7,
            Self::Background  => 0.4,
            Self::Batch       => 0.2,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Realtime    => "realtime",
            Self::Interactive => "interactive",
            Self::Background  => "background",
            Self::Batch       => "batch",
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
        self.budget_bytes.load(Ordering::Acquire)
    }

    fn used(&self) -> u64 {
        self.used_bytes.load(Ordering::Acquire)
    }

    fn set_budget(&self, bytes: u64) {
        self.budget_bytes.store(bytes, Ordering::Release);
    }

    /// Try to allocate bytes. Returns true if within budget, false if over.
    /// Allocation proceeds even if over-budget (soft limit) — caller checks pressure.
    ///
    /// Uses AcqRel ordering: the fetch_add is a release (publishes our write),
    /// and subsequent loads see all prior writes (acquire). This ensures the
    /// pressure calculation in the manager sees the true total across subsystems.
    fn allocate(&self, bytes: u64) -> bool {
        let prev = self.used_bytes.fetch_add(bytes, Ordering::AcqRel);
        let budget = self.budget_bytes.load(Ordering::Acquire);
        (prev + bytes) <= budget
    }

    fn release(&self, bytes: u64) {
        // Saturating subtract to prevent underflow
        self.used_bytes.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
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

/// Number of priority levels (Realtime, Interactive, Background, Batch).
const PRIORITY_LEVELS: usize = 4;

pub struct GpuMemoryManager {
    total_vram_bytes: u64,
    gpu_name: String,
    subsystems: [SubsystemBudget; 3], // Rendering=0, Inference=1, Tts=2
    reserve_bytes: u64,
    pressure_tx: watch::Sender<f32>,
    pressure_rx: watch::Receiver<f32>,
    /// Live allocation count per priority level [Realtime, Interactive, Background, Batch].
    allocation_counts: [AtomicU32; PRIORITY_LEVELS],
    /// Registry of GPU consumers for eviction visibility.
    pub eviction_registry: EvictionRegistry,
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
            allocation_counts: [
                AtomicU32::new(0), AtomicU32::new(0),
                AtomicU32::new(0), AtomicU32::new(0),
            ],
            eviction_registry: EvictionRegistry::new(),
        }
    }

    // ── Allocation ──────────────────────────────────────────────────────

    /// Allocate VRAM for a subsystem at the given priority level.
    ///
    /// Priority gating (RTOS-style):
    /// - Realtime: only rejected at CRITICAL (95%) — render loop, audio
    /// - Interactive: rejected at HIGH (80%) — user-facing inference, TTS
    /// - Background: rejected at WARNING (60%) — LoRA rebuild spikes
    /// - Batch: rejected at 50% — training, yields the bus first
    ///
    /// Concurrency: Uses optimistic-allocate-then-rollback to avoid TOCTOU races.
    /// Two threads racing to allocate cannot both succeed if either would push
    /// pressure past their gate — the post-allocation check catches the overcommit
    /// and rolls back the losing thread's allocation atomically.
    pub fn allocate(
        self: &Arc<Self>,
        subsystem: GpuSubsystem,
        bytes: u64,
        priority: GpuPriority,
    ) -> Result<GpuAllocationGuard, GpuError> {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        let gate = priority.pressure_gate();

        // Optimistic allocation: commit bytes first, then check if result is acceptable.
        // This is the standard lock-free pattern — avoids the TOCTOU race where two
        // threads both pass a pre-check and both allocate, pushing past their gate.
        let within_budget = self.subsystems[subsystem.index()].allocate(bytes);
        let new_pressure = self.pressure();

        // Post-allocation priority gate: rollback if we pushed past this priority's threshold.
        // Because fetch_add is atomic, at most ONE concurrent allocator sees pre-gate
        // pressure — all others see the updated total and roll back.
        if new_pressure >= gate {
            // Rollback the optimistic allocation
            self.subsystems[subsystem.index()].release(bytes);

            log_error!("gpu", "manager",
                "PRESSURE GATE: Rejecting {:.0}MB {} allocation for {} \
                 (pressure={:.0}% >= {:.0}% gate)",
                mb, priority.name(), subsystem.name(),
                new_pressure * 100.0, gate * 100.0
            );
            return Err(GpuError::PressureGate {
                subsystem: subsystem.name(),
                priority,
                requested_mb: mb,
                pressure: new_pressure,
                gate,
            });
        }

        // Allocation accepted — increment priority counter
        self.allocation_counts[priority.index()].fetch_add(1, Ordering::Relaxed);

        // Broadcast updated pressure
        let _ = self.pressure_tx.send(new_pressure);

        if !within_budget {
            log_info!("gpu", "manager",
                "WARNING: {} {} allocation {:.0}MB exceeds budget (pressure={:.0}%)",
                priority.name(), subsystem.name(), mb, new_pressure * 100.0
            );
        } else {
            log_info!("gpu", "manager",
                "GPU: Allocated {:.0}MB for {} [{}] (pressure={:.0}%)",
                mb, subsystem.name(), priority.name(), new_pressure * 100.0
            );
        }

        Ok(GpuAllocationGuard {
            manager: Arc::clone(self),
            subsystem,
            bytes,
            priority,
            released: false,
        })
    }

    /// Account for external memory usage (e.g., training subprocess).
    /// Unlike `allocate()`, this doesn't check pressure gates or return a guard.
    /// The caller MUST call `release()` when the external process finishes.
    pub fn account_external(&self, subsystem: GpuSubsystem, bytes: u64) {
        self.subsystems[subsystem.index()].allocate(bytes);
        let pressure = self.pressure();
        let _ = self.pressure_tx.send(pressure);
        let mb = bytes as f64 / (1024.0 * 1024.0);
        log_info!("gpu", "manager",
            "GPU: Accounted {:.0}MB external in {} (pressure={:.0}%)",
            mb, subsystem.name(), pressure * 100.0
        );
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
    ///
    /// Uses Acquire ordering to ensure we see the latest writes from all subsystems.
    /// Critical for the post-allocation safety check in `allocate()`.
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

    /// Test-only constructor for creating managers with known budgets.
    #[cfg(test)]
    pub fn new_for_test(
        total_vram_bytes: u64,
        gpu_name: String,
        inference_budget: u64,
        tts_budget: u64,
        rendering_budget: u64,
        reserve_bytes: u64,
        pressure_tx: watch::Sender<f32>,
        pressure_rx: watch::Receiver<f32>,
    ) -> Self {
        Self {
            total_vram_bytes,
            gpu_name,
            subsystems: [
                SubsystemBudget::new(rendering_budget),
                SubsystemBudget::new(inference_budget),
                SubsystemBudget::new(tts_budget),
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
            allocation_counts: [
                AtomicU32::new(0), AtomicU32::new(0),
                AtomicU32::new(0), AtomicU32::new(0),
            ],
            eviction_registry: EvictionRegistry::new(),
        }
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
            warning_threshold: PRESSURE_WARNING,
            high_threshold: PRESSURE_HIGH,
            critical_threshold: PRESSURE_CRITICAL,
            allocations_by_priority: AllocationsByPriority {
                realtime: self.allocation_counts[GpuPriority::Realtime.index()]
                    .load(Ordering::Relaxed),
                interactive: self.allocation_counts[GpuPriority::Interactive.index()]
                    .load(Ordering::Relaxed),
                background: self.allocation_counts[GpuPriority::Background.index()]
                    .load(Ordering::Relaxed),
                batch: self.allocation_counts[GpuPriority::Batch.index()]
                    .load(Ordering::Relaxed),
            },
        }
    }

    /// Live allocation count for a specific priority level.
    pub fn allocation_count(&self, priority: GpuPriority) -> u32 {
        self.allocation_counts[priority.index()].load(Ordering::Relaxed)
    }
}

// =============================================================================
// RAII ALLOCATION GUARD
// =============================================================================

/// RAII guard that releases GPU memory on drop.
/// Like SlotGuard in bevy_renderer.rs — deterministic cleanup.
/// Tracks both subsystem and priority for the allocation counter bookkeeping.
pub struct GpuAllocationGuard {
    manager: Arc<GpuMemoryManager>,
    subsystem: GpuSubsystem,
    bytes: u64,
    priority: GpuPriority,
    released: bool,
}

impl std::fmt::Debug for GpuAllocationGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuAllocationGuard")
            .field("subsystem", &self.subsystem)
            .field("priority", &self.priority)
            .field("bytes", &self.bytes)
            .field("released", &self.released)
            .finish()
    }
}

impl GpuAllocationGuard {
    /// Manually release before drop (e.g., when ownership transfer is needed).
    pub fn release(mut self) {
        if !self.released {
            self.do_release();
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

    /// Priority this allocation was made at.
    pub fn priority(&self) -> GpuPriority {
        self.priority
    }

    /// Internal release: decrement allocation counter + release bytes.
    fn do_release(&mut self) {
        self.manager.allocation_counts[self.priority.index()]
            .fetch_sub(1, Ordering::Relaxed);
        self.manager.release(self.subsystem, self.bytes);
        self.released = true;
    }
}

impl Drop for GpuAllocationGuard {
    fn drop(&mut self) {
        if !self.released {
            self.do_release();
        }
    }
}

// =============================================================================
// ERROR TYPE
// =============================================================================

#[derive(Debug)]
pub enum GpuError {
    /// Priority-gated rejection: pressure exceeds this priority's threshold.
    /// Supersedes the old CriticalPressure — now every priority has its own gate.
    PressureGate {
        subsystem: &'static str,
        priority: GpuPriority,
        requested_mb: f64,
        pressure: f32,
        gate: f32,
    },
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PressureGate { subsystem, priority, requested_mb, pressure, gate } => {
                write!(
                    f,
                    "GPU pressure gate ({:.0}% >= {:.0}% {} threshold): \
                     cannot allocate {:.0}MB for {}",
                    pressure * 100.0, gate * 100.0, priority.name(),
                    requested_mb, subsystem
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

/// Live allocation counts per priority level.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/gpu/AllocationsByPriority.ts")]
pub struct AllocationsByPriority {
    #[ts(type = "number")]
    pub realtime: u32,
    #[ts(type = "number")]
    pub interactive: u32,
    #[ts(type = "number")]
    pub background: u32,
    #[ts(type = "number")]
    pub batch: u32,
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
    /// Pressure threshold: above this, log warnings and defer low-priority work
    pub warning_threshold: f32,
    /// Pressure threshold: above this, refuse new model loads
    pub high_threshold: f32,
    /// Pressure threshold: above this, refuse ALL allocations
    pub critical_threshold: f32,
    /// Live allocation counts per priority level
    pub allocations_by_priority: AllocationsByPriority,
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
            allocation_counts: [
                AtomicU32::new(0), AtomicU32::new(0),
                AtomicU32::new(0), AtomicU32::new(0),
            ],
            eviction_registry: EvictionRegistry::new(),
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

        // Allocate half the inference budget at Interactive priority
        let half = inference_budget / 2;
        let guard = mgr.allocate(GpuSubsystem::Inference, half, GpuPriority::Interactive).unwrap();
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
            let _guard = mgr.allocate(GpuSubsystem::Inference, 100 * 1024 * 1024, GpuPriority::Interactive).unwrap();
            let used_during = mgr.subsystems[GpuSubsystem::Inference.index()].used();
            assert_eq!(used_during, used_before + 100 * 1024 * 1024);
        }

        let used_after = mgr.subsystems[GpuSubsystem::Inference.index()].used();
        assert_eq!(used_after, used_before);
    }

    #[test]
    fn test_manual_release() {
        let mgr = test_manager(1024);
        let guard = mgr.allocate(GpuSubsystem::Tts, 50 * 1024 * 1024, GpuPriority::Interactive).unwrap();
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
        let _guard = mgr.allocate(GpuSubsystem::Inference, 100 * 1024 * 1024, GpuPriority::Interactive).unwrap();

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
        let _guard = mgr.allocate(GpuSubsystem::Inference, 500 * 1024 * 1024, GpuPriority::Realtime).unwrap();
        assert!(rx.has_changed().unwrap_or(false) || *rx.borrow() > 0.0);
    }

    #[test]
    fn test_over_budget_soft_limit() {
        let mgr = test_manager(1024);
        let budget = mgr.inference_budget_bytes();

        // Allocate more than budget — should succeed (soft limit) at Realtime priority
        let guard = mgr.allocate(GpuSubsystem::Inference, budget + 100 * 1024 * 1024, GpuPriority::Realtime);
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

        let _g1 = mgr.allocate(GpuSubsystem::Rendering, 50 * 1024 * 1024, GpuPriority::Realtime).unwrap();
        let _g2 = mgr.allocate(GpuSubsystem::Inference, 200 * 1024 * 1024, GpuPriority::Interactive).unwrap();
        let _g3 = mgr.allocate(GpuSubsystem::Tts, 30 * 1024 * 1024, GpuPriority::Interactive).unwrap();

        let stats = mgr.stats();
        assert!(stats.rendering.used_mb > 49.0);
        assert!(stats.inference.used_mb > 199.0);
        assert!(stats.tts.used_mb > 29.0);

        // Total pressure from all subsystems
        let expected_total = 280.0; // 50+200+30
        assert!(stats.total_used_mb > expected_total - 1.0);
    }

    // ── Priority gating tests ────────────────────────────────────────

    #[test]
    fn test_realtime_only_rejected_at_critical() {
        // Realtime priority: gate = 0.95 (PRESSURE_CRITICAL)
        // Fill to 90% — Realtime should still succeed
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.90) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();
        assert!(mgr.pressure() >= PRESSURE_HIGH);

        // Realtime at 90% pressure — should succeed
        let small = 1024 * 1024; // 1MB
        let result = mgr.allocate(GpuSubsystem::Rendering, small, GpuPriority::Realtime);
        assert!(result.is_ok(), "Realtime should succeed at 90% pressure");
    }

    #[test]
    fn test_interactive_rejected_at_high() {
        // Interactive priority: gate = 0.80 (PRESSURE_HIGH)
        // Fill to 82% — Interactive should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.82) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();
        assert!(mgr.pressure() >= PRESSURE_HIGH);

        // Interactive at 82% — should be rejected (gate = 0.80)
        let small = 1024 * 1024; // 1MB
        let result = mgr.allocate(GpuSubsystem::Tts, small, GpuPriority::Interactive);
        assert!(result.is_err(), "Interactive should be rejected at 82% pressure");
    }

    #[test]
    fn test_background_rejected_at_warning() {
        // Background priority: gate = 0.60 (PRESSURE_WARNING)
        // Fill to 62% — Background should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.62) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();
        assert!(mgr.pressure() >= PRESSURE_WARNING);

        // Background at 62% — should be rejected (gate = 0.60)
        let small = 1024 * 1024;
        let result = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Background);
        assert!(result.is_err(), "Background should be rejected at 62% pressure");

        // But Interactive should still succeed at 62% (gate = 0.80)
        let result2 = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Interactive);
        assert!(result2.is_ok(), "Interactive should succeed at 62% pressure");
    }

    #[test]
    fn test_batch_rejected_at_50_percent() {
        // Batch priority: gate = 0.50
        // Fill to 52% — Batch should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.52) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();

        let small = 1024 * 1024;
        let result = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Batch);
        assert!(result.is_err(), "Batch should be rejected at 52% pressure");

        // Background should still succeed at 52% (gate = 0.60)
        let result2 = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Background);
        assert!(result2.is_ok(), "Background should succeed at 52% pressure");
    }

    #[test]
    fn test_pressure_gate_error_contains_priority_info() {
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 65%
        let fill = (usable as f64 * 0.65) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();

        // Background allocation should fail with PressureGate error
        let result = mgr.allocate(GpuSubsystem::Tts, 1024 * 1024, GpuPriority::Background);
        match result {
            Err(GpuError::PressureGate { priority, gate, .. }) => {
                assert_eq!(priority, GpuPriority::Background);
                assert!((gate - PRESSURE_WARNING).abs() < 0.001);
            }
            _ => panic!("Expected PressureGate error"),
        }
    }

    // ── Allocation counter tests ──────────────────────────────────────

    #[test]
    fn test_allocation_counters_increment_and_decrement() {
        let mgr = test_manager(1024);

        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 0);
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 0);

        let g1 = mgr.allocate(GpuSubsystem::Inference, 10 * 1024 * 1024, GpuPriority::Interactive).unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 1);

        let g2 = mgr.allocate(GpuSubsystem::Tts, 5 * 1024 * 1024, GpuPriority::Interactive).unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 2);

        let _g3 = mgr.allocate(GpuSubsystem::Rendering, 5 * 1024 * 1024, GpuPriority::Realtime).unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 1);

        // Drop g1 — Interactive should decrement
        drop(g1);
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 1);

        // Manual release g2 — Interactive should decrement again
        g2.release();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 0);

        // Realtime unchanged
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 1);
    }

    #[test]
    fn test_stats_includes_allocation_counts() {
        let mgr = test_manager(1024);

        let _g1 = mgr.allocate(GpuSubsystem::Rendering, 5 * 1024 * 1024, GpuPriority::Realtime).unwrap();
        let _g2 = mgr.allocate(GpuSubsystem::Inference, 10 * 1024 * 1024, GpuPriority::Interactive).unwrap();
        let _g3 = mgr.allocate(GpuSubsystem::Inference, 5 * 1024 * 1024, GpuPriority::Background).unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.allocations_by_priority.realtime, 1);
        assert_eq!(stats.allocations_by_priority.interactive, 1);
        assert_eq!(stats.allocations_by_priority.background, 1);
        assert_eq!(stats.allocations_by_priority.batch, 0);
    }

    #[test]
    fn test_rejected_allocation_does_not_increment_counter() {
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 65%
        let fill = (usable as f64 * 0.65) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();

        // Background allocation should fail — counter should NOT increment
        assert_eq!(mgr.allocation_count(GpuPriority::Background), 0);
        let _ = mgr.allocate(GpuSubsystem::Tts, 1024 * 1024, GpuPriority::Background);
        assert_eq!(mgr.allocation_count(GpuPriority::Background), 0,
            "Rejected allocation should not increment counter");
    }

    #[test]
    fn test_guard_stores_priority() {
        let mgr = test_manager(1024);
        let guard = mgr.allocate(GpuSubsystem::Inference, 10 * 1024 * 1024, GpuPriority::Background).unwrap();
        assert_eq!(guard.priority(), GpuPriority::Background);
    }

    // ── Concurrency tests ─────────────────────────────────────────────

    #[test]
    fn test_critical_pressure_rollback() {
        // Verify optimistic-allocate-then-rollback works:
        // Fill to near-critical, then allocate just enough to push past.
        let mgr = test_manager(1024); // 1GB
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100); // ~972MB usable

        // Fill to 94% of usable across subsystems (Realtime so it doesn't get gated)
        let fill_bytes = (usable as f64 * 0.94) as u64;
        let _fill = mgr.allocate(GpuSubsystem::Inference, fill_bytes, GpuPriority::Realtime).unwrap();
        assert!(mgr.pressure() < PRESSURE_CRITICAL);

        // This Realtime allocation should push past 95% — should be rejected and rolled back
        let overfill = (usable as f64 * 0.10) as u64; // 10% more → 104% total
        let result = mgr.allocate(GpuSubsystem::Tts, overfill, GpuPriority::Realtime);
        assert!(result.is_err(), "Should reject allocation that pushes past critical");

        // Verify the bytes were rolled back (only fill_bytes should remain)
        let tts_used = mgr.subsystems[GpuSubsystem::Tts.index()].used();
        assert_eq!(tts_used, 0, "TTS used should be 0 after rollback, got {}", tts_used);
    }

    #[test]
    fn test_concurrent_allocation_safety() {
        // Simulate the TOCTOU scenario: many threads racing to allocate.
        // With optimistic-rollback, at most one should succeed when total
        // would push past the gate.
        let mgr = test_manager(1024); // 1GB
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 90% (with Realtime so it doesn't get gated early)
        let fill = (usable as f64 * 0.90) as u64;
        let _fill_guard = mgr.allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime).unwrap();

        // Now 10 threads try Realtime allocation of 2% each — only some should succeed
        // (total would be 90% + 20% = 110%, well past critical)
        let chunk = (usable as f64 * 0.02) as u64;
        let mgr_ref = &mgr;

        let results: Vec<bool> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..10).map(|_| {
                s.spawn(move || {
                    mgr_ref.allocate(GpuSubsystem::Tts, chunk, GpuPriority::Realtime).is_ok()
                })
            }).collect();

            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let successes = results.iter().filter(|&&ok| ok).count();
        let final_pressure = mgr.pressure();

        // Some allocations should succeed, but final pressure must stay below critical.
        // (The exact number depends on scheduling, but pressure must be safe.)
        assert!(
            final_pressure < PRESSURE_CRITICAL,
            "Final pressure {:.1}% should be below critical {:.0}% — {} of 10 allocations succeeded",
            final_pressure * 100.0, PRESSURE_CRITICAL * 100.0, successes
        );
    }

    // ── Priority ordering tests ──────────────────────────────────────

    #[test]
    fn test_priority_ordering() {
        assert!(GpuPriority::Realtime < GpuPriority::Interactive);
        assert!(GpuPriority::Interactive < GpuPriority::Background);
        assert!(GpuPriority::Background < GpuPriority::Batch);
    }

    #[test]
    fn test_pressure_gates_are_monotonically_decreasing() {
        // Higher priority = higher gate = harder to reject
        assert!(GpuPriority::Realtime.pressure_gate() > GpuPriority::Interactive.pressure_gate());
        assert!(GpuPriority::Interactive.pressure_gate() > GpuPriority::Background.pressure_gate());
        assert!(GpuPriority::Background.pressure_gate() > GpuPriority::Batch.pressure_gate());
    }

    #[test]
    fn test_eviction_weights_match_priority() {
        assert!(GpuPriority::Realtime.eviction_weight() > GpuPriority::Interactive.eviction_weight());
        assert!(GpuPriority::Interactive.eviction_weight() > GpuPriority::Background.eviction_weight());
        assert!(GpuPriority::Background.eviction_weight() > GpuPriority::Batch.eviction_weight());
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

    #[test]
    fn export_bindings_gpu_priority() {
        let cfg = ts_rs::Config::default();
        GpuPriority::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_allocations_by_priority() {
        let cfg = ts_rs::Config::default();
        AllocationsByPriority::export_all(&cfg).unwrap();
    }
}
