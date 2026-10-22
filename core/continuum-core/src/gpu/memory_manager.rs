//! GpuMemoryManager — singleton VRAM tracker with Metal/CUDA detection.
//!
//! Budget model:
//!   Reserve: 5% — driver/OS headroom, held back from the usable pool.
//!   Every subsystem (inference / tts / rendering) shares the FULL usable pool;
//!   there is NO static per-subsystem partition. Contention is arbitrated live by
//!   the dynamic pressure gate below — a subsystem is admitted while total physical
//!   pressure sits under its priority's threshold, not because it fits a fixed slice.
//!   (The old static 75/10/10 split was demand-blind and the soft-limit path already
//!   ignored it; see the constants comment. Full convergence to leases from the one
//!   ResourceDaemon authority is task #56.)
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

use super::eviction_registry::EvictionRegistry;
use crate::{log_error, log_info};

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
#[ts(export, export_to = "../../../protocol/typescript/gpu/GpuPriority.ts")]
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
            Self::Realtime => PRESSURE_CRITICAL,  // 0.95 — only OOM
            Self::Interactive => PRESSURE_HIGH,   // 0.80
            Self::Background => PRESSURE_WARNING, // 0.60
            Self::Batch => 0.50,                  // yields at 50%
        }
    }

    /// Weight for eviction scoring — lower priority evicts first.
    pub fn eviction_weight(self) -> f32 {
        match self {
            Self::Realtime => f32::INFINITY, // never evictable
            Self::Interactive => 0.7,
            Self::Background => 0.4,
            Self::Batch => 0.2,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::Interactive => "interactive",
            Self::Background => "background",
            Self::Batch => "batch",
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
        self.used_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_sub(bytes))
            })
            .ok();
    }
}

// =============================================================================
// GPU MEMORY MANAGER
// =============================================================================

/// Driver/OS headroom held back from the usable pool unconditionally.
const RESERVE_PCT: f64 = 0.05;

// The old static 75/10/10 inference/tts/rendering partition is GONE (memory-authority
// slice 4, `[[memory-system-is-fully-dynamic-nothing-static]]`). It was a demand-blind
// split of `usable` that the real admission path already ignored: `SubsystemBudget`
// is a SOFT limit ("allocation proceeds even if over-budget" — see `SubsystemBudget::allocate`),
// and the only hard gate is the DYNAMIC pressure gate (`new_pressure >= priority.pressure_gate()`
// in `allocate`), read off the same physical GpuMonitor the one-authority board reads.
// So a static fraction never governed anything — it only skewed a warning log + telemetry,
// and it lied that inference could never exceed 75% of VRAM while TTS/rendering sat idle.
// Every subsystem now draws from the FULL shared `usable` pool; the pressure gate arbitrates
// contention live. (The full convergence — Bevy/TTS/inference LEASE from the ResourceDaemon
// board instead of keeping this parallel pressure ledger — is task #56.)

// CPU_FALLBACK_RAM_PCT removed (#964 series PR #3 / #980 GPU-fallback
// audit). Per Joel's architectural rule "lack of GPU integration is
// forbidden", continuum-core refuses to start when no GPU is detected
// rather than silently degrading to a CPU-budget pretend-GPU. Same shape
// as install.sh's hard-fail on `IC_GPU_PATH=unsupported` — surface the
// problem at startup with an actionable error instead of a slow-and-bad
// runtime.

/// Pressure thresholds.
pub const PRESSURE_WARNING: f32 = 0.60;
pub const PRESSURE_HIGH: f32 = 0.80;
pub const PRESSURE_CRITICAL: f32 = 0.95;

/// Number of priority levels (Realtime, Interactive, Background, Batch).
const PRIORITY_LEVELS: usize = 4;

/// Upper bound on any GPU-detection subprocess (#3732).
///
/// These run PRE-BIND — before the IPC socket exists and before the first
/// module is registered — so the cost of an unbounded one is the whole boot,
/// not a degraded subsystem. 10 s matches the bound #3719 put on the serving
/// `--version` probe: long enough that a cold driver query on a loaded host
/// still answers, short enough that a wedged one does not look like a hang.
///
/// NOTE what this does NOT cover: `detect_metal()` is a direct metal-rs call,
/// not a subprocess, so no timeout can be wrapped around it from here. That
/// same Metal initialisation is what wedged an Intel Mac uninterruptibly on
/// 2026-09-05 (card cd8f0bc7) — inside `llama-server`, where the fix was to
/// stop linking Metal on that hardware. If it ever wedges in-process here, the
/// bound has to come from the platform gate, not from this constant.
#[cfg_attr(
    not(any(feature = "cuda", feature = "vulkan")),
    allow(dead_code) // only the subprocess backends consume it; macOS exits at detect_metal
)]
const GPU_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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

        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);

        let total_mb = total_bytes / (1024 * 1024);
        let usable_mb = usable / (1024 * 1024);
        let reserve_mb = reserve_bytes / (1024 * 1024);

        log_info!(
            "gpu",
            "manager",
            "GPU detected: {} — {}MB VRAM",
            gpu_name,
            total_mb
        );
        log_info!(
            "gpu",
            "manager",
            "Shared usable pool: {}MB (reserve {}MB) — no static per-subsystem split; \
             the dynamic pressure gate arbitrates inference/tts/rendering contention",
            usable_mb,
            reserve_mb
        );

        Self {
            total_vram_bytes: total_bytes,
            gpu_name,
            // No static partition: every subsystem may draw the full shared `usable`
            // pool; the pressure gate is the live arbiter (see the constants comment).
            subsystems: [
                SubsystemBudget::new(usable), // index 0 — rendering
                SubsystemBudget::new(usable), // index 1 — inference
                SubsystemBudget::new(usable), // index 2 — tts
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
            allocation_counts: [
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
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

            log_error!(
                "gpu",
                "manager",
                "PRESSURE GATE: Rejecting {:.0}MB {} allocation for {} \
                 (pressure={:.0}% >= {:.0}% gate)",
                mb,
                priority.name(),
                subsystem.name(),
                new_pressure * 100.0,
                gate * 100.0
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
            log_info!(
                "gpu",
                "manager",
                "WARNING: {} {} allocation {:.0}MB exceeds budget (pressure={:.0}%)",
                priority.name(),
                subsystem.name(),
                mb,
                new_pressure * 100.0
            );
        } else {
            log_info!(
                "gpu",
                "manager",
                "GPU: Allocated {:.0}MB for {} [{}] (pressure={:.0}%)",
                mb,
                subsystem.name(),
                priority.name(),
                new_pressure * 100.0
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
        log_info!(
            "gpu",
            "manager",
            "GPU: Accounted {:.0}MB external in {} (pressure={:.0}%)",
            mb,
            subsystem.name(),
            pressure * 100.0
        );
    }

    /// Manual release (when RAII guard isn't suitable, e.g. non-Drop contexts).
    pub fn release(&self, subsystem: GpuSubsystem, bytes: u64) {
        self.subsystems[subsystem.index()].release(bytes);

        let pressure = self.pressure();
        let _ = self.pressure_tx.send(pressure);

        let mb = bytes as f64 / (1024.0 * 1024.0);
        log_info!(
            "gpu",
            "manager",
            "GPU: Released {:.0}MB from {} (pressure={:.0}%)",
            mb,
            subsystem.name(),
            pressure * 100.0
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

    /// Inference subsystem soft budget in bytes. With the static partition gone this
    /// is the full shared usable pool (unless a test/command narrowed it via
    /// `set_budget`); the pressure gate, not this number, is the real admission bound.
    pub fn inference_budget_bytes(&self) -> u64 {
        self.subsystems[GpuSubsystem::Inference.index()].budget()
    }

    /// Bytes currently allocated to one subsystem — the same atomic `stats()`
    /// reports, in bytes rather than a rounded MB float. This is the honest,
    /// measured residency source a `ResourceConsumer::footprint()` reads (e.g.
    /// the voice consumer reporting its resident TTS weights).
    pub fn used_bytes(&self, subsystem: GpuSubsystem) -> u64 {
        self.subsystems[subsystem.index()].used()
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

    /// Simulated GPU for tests — just name + total VRAM, default budget split.
    #[cfg(test)]
    pub fn simulated(gpu_name: &str, total_vram_bytes: u64) -> Self {
        let reserve_bytes = (total_vram_bytes as f64 * RESERVE_PCT) as u64;
        let usable = total_vram_bytes.saturating_sub(reserve_bytes);
        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);
        Self {
            total_vram_bytes,
            gpu_name: gpu_name.to_string(),
            subsystems: [
                SubsystemBudget::new(usable),
                SubsystemBudget::new(usable),
                SubsystemBudget::new(usable),
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
            allocation_counts: [
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
            ],
            eviction_registry: EvictionRegistry::new(),
        }
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
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
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
                batch: self.allocation_counts[GpuPriority::Batch.index()].load(Ordering::Relaxed),
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
        self.manager.allocation_counts[self.priority.index()].fetch_sub(1, Ordering::Relaxed);
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
            Self::PressureGate {
                subsystem,
                priority,
                requested_mb,
                pressure,
                gate,
            } => {
                write!(
                    f,
                    "GPU pressure gate ({:.0}% >= {:.0}% {} threshold): \
                     cannot allocate {:.0}MB for {}",
                    pressure * 100.0,
                    gate * 100.0,
                    priority.name(),
                    requested_mb,
                    subsystem
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/SubsystemStats.ts"
)]
pub struct SubsystemStats {
    #[ts(type = "number")]
    pub budget_mb: f32,
    #[ts(type = "number")]
    pub used_mb: f32,
}

/// Live allocation counts per priority level.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/AllocationsByPriority.ts"
)]
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
#[ts(export, export_to = "../../../protocol/typescript/gpu/GpuStats.ts")]
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

    // Try Vulkan. Until 2026-05-04 detect_gpu() had no vulkan branch even
    // though `vulkan` was listed as a supported path in the panic message
    // and Cargo features. Result: continuum-core-vulkan binary panicked at
    // boot on every host because the loader was never queried, regardless
    // of whether a Vulkan ICD was present (NVIDIA, mesa-llvmpipe sw,
    // mesa-radv, etc). Caught live by Carl-Windows install retest of the
    // vulkan variant on bigmama-1 (continuum-b69f, 2026-05-04) — the
    // image had libvulkan1 + mesa-vulkan-drivers + vulkan-tools but the
    // binary never asked the loader. detect_vulkan() below mirrors the
    // detect_cuda() subprocess shape, parsing `vulkaninfo --summary`
    // (already in the runtime image via the vulkan-tools apt package).
    #[cfg(feature = "vulkan")]
    {
        if let Some(result) = detect_vulkan() {
            return result;
        }
    }

    // No GPU detected. Per architecture, CPU fallback is forbidden
    // (#964 series / #980 GPU-fallback audit). Hard-fail with the same
    // shape install.sh's `IC_GPU_PATH=unsupported` branch uses: name
    // what's supported, point at the diagnostic command, exit cleanly.
    panic!(
        "No GPU detected (Metal on macOS / CUDA on Linux+Nvidia). \
         continuum-core requires GPU acceleration — CPU fallback is forbidden \
         per architectural rule. Supported paths: macos:metal, linux:cuda, \
         linux:rocm, linux:vulkan, wsl:cuda, wsl:vulkan, windows:cuda, \
         windows:vulkan. If your hardware IS one of those, the detector \
         missed something. Diagnose: \
         - macOS: 'system_profiler SPDisplaysDataType' should list a Metal device \
         - Linux/WSL CUDA: 'nvidia-smi' should print GPU info \
         - Linux ROCm: 'rocminfo' should print GPU info \
         - Linux/WSL/Windows Vulkan: 'vulkaninfo --summary' should list a deviceName \
         If your hardware truly isn't supported, continuum-core can't run \
         reliably on this machine. File an issue at \
         https://github.com/CambrianTech/continuum/issues with the output of \
         'uname -a' + nvidia-smi/rocminfo/vulkaninfo as applicable."
    );
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
    //
    // BOUNDED (#3732): this runs PRE-BIND, six lines after boot.registry_init
    // and before the first runtime.register, so an unbounded wait here does not
    // hang a subsystem — it hangs the whole boot, silently, until the watchdog
    // kills it. A hung `nvidia-smi` is ordinary on Windows (driver mid-recovery,
    // a WDDM/TCC transition, another process holding the driver, an ECC query on
    // a busy card). macOS never reaches this branch — detect_metal() answers
    // from a direct metal-rs call with no subprocess — which is why both Macs on
    // the grid were structurally unable to reproduce it.
    let probed = crate::system_resources::bounded_command::probe(
        "nvidia-smi",
        &[
            "--query-gpu=memory.total,name",
            "--format=csv,noheader,nounits",
        ],
        GPU_PROBE_TIMEOUT,
    );
    continuum_base::probe!(
        class = "boot.gpu_detect",
        backend = "cuda",
        outcome = probed.outcome(),
        "pre-bind GPU probe (bounded; an unbounded one hangs the boot)"
    );

    let stdout = probed.stdout_if_ok()?;
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

/// Vulkan detection via vulkaninfo subprocess.
///
/// Mirrors detect_cuda's nvidia-smi approach. The vulkan-tools apt package
/// (already in continuum-core-vulkan.Dockerfile's runtime stage) ships
/// vulkaninfo. Parsing --summary gives us a deviceName, which is enough
/// to satisfy the architectural rule "Vulkan loader produced a usable
/// device" — be it NVIDIA's ICD on a GPU host, mesa-radv on AMD, or
/// llvmpipe (mesa software ICD) on a no-/dev/dri runner like
/// ubuntu-latest CI.
///
/// Memory size is conservative because vulkaninfo --summary doesn't
/// always report device-local heap totals reliably; runtime allocations
/// query the loader directly via candle/llama-cpp's vulkan backend
/// anyway, so this number is only used for the budget estimator.
#[cfg(feature = "vulkan")]
fn detect_vulkan() -> Option<(u64, String)> {
    // BOUNDED (#3732): same pre-bind path as detect_cuda above, and reached
    // precisely when nvidia-smi already declined — so on a host whose GPU stack
    // is unwell this is the SECOND unbounded wait in a row on the boot thread.
    let probed =
        crate::system_resources::bounded_command::probe("vulkaninfo", &["--summary"], GPU_PROBE_TIMEOUT);
    continuum_base::probe!(
        class = "boot.gpu_detect",
        backend = "vulkan",
        outcome = probed.outcome(),
        "pre-bind GPU probe (bounded; an unbounded one hangs the boot)"
    );

    let stdout = probed.stdout_if_ok()?;

    // vulkaninfo --summary format (excerpt):
    //   Devices:
    //   ========
    //   GPU0:
    //           apiVersion         = 1.3.260
    //           driverVersion      = 0x0
    //           vendorID           = 0x10005
    //           deviceID           = 0x0
    //           deviceType         = PHYSICAL_DEVICE_TYPE_CPU
    //           deviceName         = llvmpipe (LLVM 17.0.6, 256 bits)
    //
    // Take the FIRST deviceName (vulkaninfo orders discrete > integrated > CPU
    // by default on most loaders). If absent, no usable ICD.
    let device_name = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("deviceName"))
        .and_then(|l| l.split('=').nth(1))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;

    // Conservative VRAM budget: 4 GiB. Real allocations go through the
    // Vulkan loader at runtime; this only seeds the GpuMemoryManager
    // budget estimator. For a CUDA host we get exact memory.total via
    // nvidia-smi; for Vulkan there's no equivalent single-line query
    // that handles all ICDs uniformly without pulling in `ash`.
    let total_bytes: u64 = 4 * 1024 * 1024 * 1024;

    Some((total_bytes, device_name))
}

// detect_cpu_fallback() removed — see detect_gpu()'s panic for rationale.
// CPU fallback is forbidden architecturally; absent GPU = absent system.

/// Total physical system RAM, cross-platform via `sysinfo` (Windows / Linux / macOS).
///
/// The old hand-rolled probes were per-OS: macOS shelled out to `sysctl hw.memsize`, and the
/// `#[cfg(not(target_os = "macos"))]` branch read `/proc/meminfo` — which is LINUX-ONLY. On
/// Windows there is no `/proc/meminfo`, so the read failed and the function fell through to an
/// 8 GiB fallback. Every downstream fit decision (which model fits, what serving budget) was
/// then made against a phantom 8 GiB box regardless of the real machine — a 5090 workstation
/// with 63 GiB looked like an 8 GiB laptop and refused to serve anything bigger than a toy.
/// `sysinfo` (already a dependency) reports real physical memory uniformly on all three OSes.
fn get_system_ram() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    let total = sys.total_memory(); // bytes in sysinfo 0.30+
    if total > 0 {
        total
    } else {
        8 * 1024 * 1024 * 1024 // last-resort only if the OS probe genuinely returns 0
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests;
