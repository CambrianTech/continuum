//! GPU/memory monitor — adapter trait per platform.
//!
//! Per §12 of docs/architecture/PERSONA-CONTEXT-PAGING.md: the
//! current `GpuMemoryManager` is the symptom of an anti-pattern —
//! one struct with `#[cfg]` branches, each platform doing different
//! (and uneven) things. The Metal path returns
//! `recommendedMaxWorkingSetSize` (a static lifetime hint, NOT live
//! free memory); pressure is computed from internal accounting only;
//! a video game grabbing VRAM doesn't register.
//!
//! This module defines the right shape: a `GpuMonitor` trait per
//! platform. Each implementation talks to its platform's actual
//! monitoring API. The `PagingPolicy` (and the existing
//! `GpuMemoryManager` once retrofitted) holds an `Arc<dyn GpuMonitor>`
//! and never branches on platform.
//!
//! Phase 2.0 ships:
//!   - The trait
//!   - `MockMonitor` (test-only) for unit testing the policy without a real GPU
//!
//! There is NO CpuMonitor / no-GPU fallback adapter: GPU acceleration is
//! required (`detect_gpu()` hard-fails without it, #980), and a CPU stand-in
//! would only mask a missing device with silently-wrong numbers — turning a
//! GPU build "all CPU again" without anyone noticing. Absent device → fail
//! loud at the seam, never substitute.
//!
//! Phase 2.0a (follow-up):
//!   - `MetalMonitor` via IOReport FFI (the actual fix for the
//!     macbook monitoring bug that motivated §12). Requires a small
//!     IOReport FFI shim — not in any maintained crate.
//!   - `NvidiaMonitor` via NVML (`nvml-wrapper` crate)
//!   - `VulkanMonitor` via VK_EXT_memory_budget for cross-vendor

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::watch;
use ts_rs::TS;

/// Live, fast-to-read memory + utilization signals for the policy.
/// Each implementation talks to its platform's actual monitoring API.
/// The trait normalizes the shape so the policy doesn't care which
/// platform produced the signals.
pub trait GpuMonitor: Send + Sync {
    /// Platform identifier — "metal" | "cuda" | "vulkan" | "cpu" | "mock".
    fn platform(&self) -> &'static str;

    /// Human-readable device name (e.g. "Apple M5 Pro", "NVIDIA RTX 5090",
    /// "CPU (no GPU)"). For logs and the policy's "what hardware are we
    /// on" decisions.
    fn device_name(&self) -> &str;

    /// Total physical VRAM in bytes (or, for unified-memory architectures
    /// like Apple Silicon, the share of unified memory the GPU can address).
    fn total_bytes(&self) -> u64;

    /// CURRENTLY free bytes — observed from the platform, NOT from our
    /// internal allocation accounting. This is the signal that lets the
    /// policy detect a video game grabbing our headroom.
    fn free_bytes(&self) -> u64;

    /// Bytes allocated by OUR process specifically. Lets the policy
    /// distinguish "system is tight" from "we are tight" and react
    /// differently (system-tight → spill our slots; we-tight → just
    /// rebalance internally).
    fn process_bytes(&self) -> u64;

    /// Compute utilization (0.0..1.0). Important for the policy's
    /// latency model — if the GPU is already busy with something else,
    /// our inference latency goes up. High utilization with low memory
    /// pressure still means "now is a bad time to start a heavy turn."
    fn utilization(&self) -> f32;

    /// Optional thermals in Celsius. Throttling kicks in around 90-95°C
    /// on most GPUs; the policy should downgrade non-critical work
    /// when approaching throttle.
    fn temperature_c(&self) -> Option<f32>;

    /// Optional current power draw (watts). Battery scenarios: policy
    /// can prefer cheaper-paged states when on battery vs plugged-in.
    fn power_watts(&self) -> Option<f32>;

    /// Subscribe to live pressure updates (free→used ratio + utilization
    /// blend). Tick rate is platform-specific (Metal: ~1Hz cheap;
    /// NVML: 10Hz cheap; nvidia-smi: 1Hz expensive — implementation
    /// hides the cost). The policy reads from this on its rebalance loop.
    fn pressure_rx(&self) -> watch::Receiver<f32>;

    /// Snapshot of all the signals at one moment, for telemetry capture
    /// (the FootprintRegistry sanity check, the learned policy's training
    /// corpus). Default impl synthesizes from the individual getters; a
    /// platform-native impl can return them atomically (single OS call
    /// → all fields) for slightly cheaper sampling.
    fn snapshot(&self) -> GpuSnapshot {
        GpuSnapshot {
            platform: self.platform().to_string(),
            device_name: self.device_name().to_string(),
            total_bytes: self.total_bytes(),
            free_bytes: self.free_bytes(),
            process_bytes: self.process_bytes(),
            utilization: self.utilization(),
            temperature_c: self.temperature_c(),
            power_watts: self.power_watts(),
            pressure: *self.pressure_rx().borrow(),
        }
    }
}

/// Atomic snapshot of all monitor signals. Used by the FootprintRegistry
/// sanity check, the learned-policy training corpus capture, and — as a
/// ts-rs wire type — the Positron SYS-gauge GPU series (device-wide
/// `used = total_bytes - free_bytes`, the same system-wide framing the
/// CPU/MEM series already use; `process_bytes` is our-process-only and is
/// deliberately kept distinct).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/gpu/GpuSnapshot.ts")]
pub struct GpuSnapshot {
    pub platform: String,
    pub device_name: String,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub free_bytes: u64,
    #[ts(type = "number")]
    pub process_bytes: u64,
    pub utilization: f32,
    #[ts(optional)]
    pub temperature_c: Option<f32>,
    #[ts(optional)]
    pub power_watts: Option<f32>,
    pub pressure: f32,
}

// NOTE: there is deliberately NO `CpuMonitor`. A "no-GPU fallback"
// monitor is the exact plague `detect_gpu()` already outlaws (#980): GPU
// acceleration is REQUIRED, and a CPU stand-in only masks a missing real
// device with silently-wrong numbers. Absent GPU → fail loud, never
// substitute. `MockMonitor` below is the test double; it is NOT a
// production fallback (it lives for `#[cfg(test)]` policy scenarios only).

// ─── MockMonitor — for unit tests of the policy ──────────────────────

/// Scriptable monitor for unit-testing policy behavior under specific
/// memory/utilization scenarios. Each field can be set independently;
/// pressure can be driven via the channel for time-series tests
/// ("game starts at t=10s, ends at t=30s").
pub struct MockMonitor {
    device_name: String,
    total_bytes: u64,
    free_bytes: std::sync::atomic::AtomicU64,
    process_bytes: std::sync::atomic::AtomicU64,
    utilization_x1000: std::sync::atomic::AtomicU32,
    temperature_c: std::sync::atomic::AtomicI32,
    power_watts: std::sync::atomic::AtomicI32,
    pressure_tx: watch::Sender<f32>,
    pressure_rx: watch::Receiver<f32>,
}

impl MockMonitor {
    pub fn new(total_bytes: u64) -> Self {
        let (pressure_tx, pressure_rx) = watch::channel(0.0);
        Self {
            device_name: "Mock GPU".to_string(),
            total_bytes,
            free_bytes: std::sync::atomic::AtomicU64::new(total_bytes),
            process_bytes: std::sync::atomic::AtomicU64::new(0),
            utilization_x1000: std::sync::atomic::AtomicU32::new(0),
            temperature_c: std::sync::atomic::AtomicI32::new(i32::MIN), // sentinel = None
            power_watts: std::sync::atomic::AtomicI32::new(i32::MIN),
            pressure_tx,
            pressure_rx,
        }
    }

    pub fn set_free_bytes(&self, b: u64) {
        self.free_bytes
            .store(b, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn set_process_bytes(&self, b: u64) {
        self.process_bytes
            .store(b, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn set_utilization(&self, u: f32) {
        let scaled = (u.clamp(0.0, 1.0) * 1000.0) as u32;
        self.utilization_x1000
            .store(scaled, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn set_temperature_c(&self, t: f32) {
        self.temperature_c
            .store(t as i32, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn set_power_watts(&self, p: f32) {
        self.power_watts
            .store(p as i32, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn set_pressure(&self, p: f32) {
        let _ = self.pressure_tx.send(p.clamp(0.0, 1.0));
    }
}

impl GpuMonitor for MockMonitor {
    fn platform(&self) -> &'static str {
        "mock"
    }
    fn device_name(&self) -> &str {
        &self.device_name
    }
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    fn free_bytes(&self) -> u64 {
        self.free_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn process_bytes(&self) -> u64 {
        self.process_bytes
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    fn utilization(&self) -> f32 {
        self.utilization_x1000
            .load(std::sync::atomic::Ordering::Relaxed) as f32
            / 1000.0
    }
    fn temperature_c(&self) -> Option<f32> {
        let v = self
            .temperature_c
            .load(std::sync::atomic::Ordering::Relaxed);
        if v == i32::MIN {
            None
        } else {
            Some(v as f32)
        }
    }
    fn power_watts(&self) -> Option<f32> {
        let v = self.power_watts.load(std::sync::atomic::Ordering::Relaxed);
        if v == i32::MIN {
            None
        } else {
            Some(v as f32)
        }
    }
    fn pressure_rx(&self) -> watch::Receiver<f32> {
        self.pressure_rx.clone()
    }
}

// ─── detect — the live-monitor analog of GpuMemoryManager::detect ────

/// Detect and construct the **live-scanning** GPU monitor for THIS host.
///
/// This is the live-signal sibling of [`GpuMemoryManager::detect`](crate::gpu::memory_manager::GpuMemoryManager::detect):
/// where that one captures a *static* working-set hint at boot,
/// this one returns a monitor that re-samples free/process VRAM every
/// tick (the signal that lets the resource governor SEE a game or
/// renderer grabbing our headroom — the whole point of the
/// net-of-external ceiling in [`GpuCapacitySource`](crate::resources::capacity::GpuCapacitySource)).
///
/// Returns `None` when no live-scanning adapter exists *for this
/// platform yet* — `MetalMonitor` is the only one built today;
/// `NvidiaMonitor` (NVML) and `VulkanMonitor` (`VK_EXT_memory_budget`)
/// are the documented Phase 2.0a follow-ups. `None` is an honest
/// **capability gap**, and it is emphatically NOT a cue to substitute a
/// CPU monitor: a fake-GPU-monitor reporting RAM-as-VRAM is the exact
/// "all CPU again" plague — it would silently mask a real GPU we should
/// be using behind fabricated numbers. The caller (the governor boot
/// site) MUST surface `None` loudly by name and fail at the seam; it must
/// never swallow `None` into "use total" or "no governance needed".
///
/// To be precise about the one legitimate CPU scenario: the rule is GPU
/// for all models, with a SINGLE sanctioned exception — a standalone
/// install on a GPU-less Intel Mac with no grid peer to offload to. That
/// is a deliberate, explicitly-chosen, loudly-logged deployment path, not
/// a value this function ever silently returns. (Contrast
/// [`detect_gpu`](crate::gpu::memory_manager) which today hard-fails on a
/// genuinely GPU-less host; that absolute is the #980 stance and any CPU
/// deployment path must be its own explicit branch, never a swallowed
/// `None` here.)
///
/// On Apple Silicon the returned monitor is unified-memory aware, so
/// its `free_bytes()` tracks real system headroom — exactly the live
/// number serving's budget should be capped at.
pub fn detect() -> Option<Arc<dyn GpuMonitor>> {
    #[cfg(target_os = "macos")]
    {
        if let Some(m) = MetalMonitor::new() {
            return Some(m as Arc<dyn GpuMonitor>);
        }
    }
    // NVIDIA via `nvidia-smi` — covers CUDA hosts AND NVIDIA-on-Vulkan
    // hosts with a genuine live free-VRAM signal (not a static lie).
    // `new()` returns None on non-NVIDIA hosts, so this is safe to try
    // everywhere; on macOS the Metal branch above already returned.
    if let Some(m) = super::NvidiaMonitor::new() {
        return Some(m as Arc<dyn GpuMonitor>);
    }
    // Remaining real adapter still to build: a Vulkan live monitor
    // (AMD/Intel via VK_EXT_memory_budget — needs `ash` FFI). It is
    // deliberately NOT faked with a static heap size: a stale `free` that
    // never drops is the exact bug this whole live-monitor layer exists to
    // kill. On such a host detect() returns None and the governor boot site
    // fails loud naming the missing adapter — never a silent substitute.
    //
    // This adapter is double-duty: besides non-NVIDIA Linux GPUs, Vulkan is
    // the ONLY shape by which a Linux container on a Mac could ever see the
    // GPU — a Linux guest cannot call Metal directly, so any Mac-GPU-in-Docker
    // passthrough surfaces as paravirtualized Metal-through-Vulkan. Because
    // detect() is a real runtime PROBE (not platform inference), the day that
    // ships it lights up here with no other change. Today: Mac deploys NATIVE
    // (the Metal branch above, --features metal); Docker is the Linux/CUDA
    // artifact (the NvidiaMonitor branch). CUDA-in-Docker is the proven path.
    None
}

#[cfg(target_os = "macos")]
use super::MetalMonitor;

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: MockMonitor not actually being mutable
    /// (e.g. a typo storing into the wrong field, or atomics dropped).
    /// Tests of the policy depend on driving the mock's signals
    /// dynamically.
    ///
    /// Validated 2026-04-21: forgot to actually store free_bytes in
    /// set_free_bytes (no-op'd it), test fails because get returns initial.
    #[test]
    fn mock_monitor_setters_actually_update_observable_state() {
        let m = MockMonitor::new(16 * 1024 * 1024 * 1024);
        m.set_free_bytes(1024);
        m.set_process_bytes(8192);
        m.set_utilization(0.75);
        m.set_temperature_c(82.5);
        m.set_power_watts(45.0);
        m.set_pressure(0.6);

        assert_eq!(m.free_bytes(), 1024);
        assert_eq!(m.process_bytes(), 8192);
        assert!((m.utilization() - 0.75).abs() < 0.01);
        assert_eq!(m.temperature_c(), Some(82.0)); // i32 truncation
        assert_eq!(m.power_watts(), Some(45.0));
        assert!((*m.pressure_rx().borrow() - 0.6).abs() < 0.01);
    }

    /// What this catches: MockMonitor's optional fields (temperature,
    /// power) not properly defaulting to None when unset. The sentinel
    /// (i32::MIN) approach must survive the round-trip through atomics.
    ///
    /// Validated 2026-04-21: changed sentinel check to `== 0` (which 0°C
    /// would falsely match), test fails when set_temperature_c(0.0)
    /// returns None instead of Some(0.0).
    #[test]
    fn mock_monitor_temperature_and_power_default_to_none() {
        let m = MockMonitor::new(1024);
        assert_eq!(m.temperature_c(), None);
        assert_eq!(m.power_watts(), None);

        // After setting, returns Some(value) — including 0.0 boundary
        m.set_temperature_c(0.0);
        assert_eq!(m.temperature_c(), Some(0.0));
        m.set_power_watts(0.0);
        assert_eq!(m.power_watts(), Some(0.0));
    }

    /// What this catches: snapshot() composing fields incorrectly
    /// (e.g. swapping free/process or losing the pressure value).
    /// The default trait impl must faithfully reflect each getter.
    ///
    /// Validated 2026-04-21: swapped free_bytes and process_bytes in
    /// the default impl, test fails on the assertion below.
    #[test]
    fn snapshot_atomically_reflects_individual_getters() {
        let m = MockMonitor::new(1_000_000);
        m.set_free_bytes(700_000);
        m.set_process_bytes(200_000);
        m.set_utilization(0.4);
        m.set_pressure(0.3);

        let snap = m.snapshot();
        assert_eq!(snap.platform, "mock");
        assert_eq!(snap.total_bytes, 1_000_000);
        assert_eq!(snap.free_bytes, 700_000);
        assert_eq!(snap.process_bytes, 200_000);
        assert!((snap.utilization - 0.4).abs() < 0.01);
        assert!((snap.pressure - 0.3).abs() < 0.01);
    }

    /// What this catches: pressure_rx returning a stale receiver that
    /// doesn't see new pressure values. This would break the policy's
    /// rebalance loop (it'd never see updates).
    ///
    /// Validated 2026-04-21: returned a freshly-constructed receiver
    /// instead of cloning the stored one, test fails because the new
    /// receiver doesn't see the update.
    #[test]
    fn pressure_rx_receives_subsequent_updates() {
        let m = MockMonitor::new(1024);
        let rx = m.pressure_rx();
        m.set_pressure(0.42);
        // borrow() reads latest published value
        assert!((*rx.borrow() - 0.42).abs() < 0.01);
    }

    /// What this catches: detect() handing back a monitor with a
    /// garbage capacity (the `total == 0 → None` guard regressing) or a
    /// bogus platform tag. Deliberately environment-INDEPENDENT (task
    /// #72): a headless build env with no Metal device returns None, and
    /// that's a valid pass — we only assert the invariant that WHEN a
    /// live monitor is detected it reports real capacity and a known
    /// platform. `#[tokio::test]` because MetalMonitor::new spawns its
    /// sampling daemon on the shared runner, which needs a reactor.
    #[tokio::test]
    async fn detect_yields_a_sane_monitor_or_honest_none() {
        match detect() {
            Some(m) => {
                assert!(
                    m.total_bytes() > 0,
                    "a detected live monitor must report real VRAM capacity, not 0"
                );
                assert!(
                    matches!(m.platform(), "metal" | "cuda" | "vulkan"),
                    "unexpected live-monitor platform tag: {}",
                    m.platform()
                );
            }
            // No live-scanning adapter for this platform/host yet — an
            // honest capability gap, not a failure. The governor boot
            // site is responsible for surfacing it loudly.
            None => {}
        }
    }
}
