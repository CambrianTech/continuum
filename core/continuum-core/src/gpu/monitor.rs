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
//!   - `CpuMonitor` (no-GPU fallback) as the first concrete adapter
//!   - `MockMonitor` for unit testing the policy without a real GPU
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
/// sanity check and the learned-policy training corpus capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSnapshot {
    pub platform: String,
    pub device_name: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub process_bytes: u64,
    pub utilization: f32,
    pub temperature_c: Option<f32>,
    pub power_watts: Option<f32>,
    pub pressure: f32,
}

// ─── CpuMonitor — no-GPU fallback ────────────────────────────────────

/// The "no GPU detected" fallback adapter. Reports system RAM as the
/// "total" budget and never claims utilization (CPU inference still
/// works, we just can't measure GPU stats). Used on Linux servers
/// without GPUs, in test harnesses that want a deterministic monitor,
/// and as the safety floor when GPU detection fails.
pub struct CpuMonitor {
    device_name: String,
    total_bytes: u64,
    pressure_tx: watch::Sender<f32>,
    pressure_rx: watch::Receiver<f32>,
}

impl CpuMonitor {
    pub fn new(total_ram_bytes: u64) -> Self {
        let (pressure_tx, pressure_rx) = watch::channel(0.0);
        Self {
            device_name: "CPU (no GPU)".to_string(),
            total_bytes: total_ram_bytes,
            pressure_tx,
            pressure_rx,
        }
    }

    /// Update the pressure signal from caller-supplied accounting.
    /// CPU-only setup has no live OS-level pressure source for "GPU
    /// memory", so the caller (typically the FootprintRegistry's own
    /// sum) becomes the proxy. Not as good as a real OS signal but
    /// preserves the trait shape so the policy code doesn't change.
    pub fn update_pressure(&self, p: f32) {
        let _ = self.pressure_tx.send(p.clamp(0.0, 1.0));
    }
}

impl GpuMonitor for CpuMonitor {
    fn platform(&self) -> &'static str {
        "cpu"
    }
    fn device_name(&self) -> &str {
        &self.device_name
    }
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    fn free_bytes(&self) -> u64 {
        // Without an OS query, "free" = total minus the policy's
        // own accounting reflected in the pressure signal.
        let pressure = *self.pressure_rx.borrow();
        let used = (self.total_bytes as f64 * pressure as f64) as u64;
        self.total_bytes.saturating_sub(used)
    }
    fn process_bytes(&self) -> u64 {
        // Same source as free: derived from accounted pressure.
        let pressure = *self.pressure_rx.borrow();
        (self.total_bytes as f64 * pressure as f64) as u64
    }
    fn utilization(&self) -> f32 {
        0.0 // No GPU compute utilization to report.
    }
    fn temperature_c(&self) -> Option<f32> {
        None
    }
    fn power_watts(&self) -> Option<f32> {
        None
    }
    fn pressure_rx(&self) -> watch::Receiver<f32> {
        self.pressure_rx.clone()
    }
}

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
/// **capability gap**, NOT a CPU fallback and NOT a hidden failure:
/// the caller (the governor boot site) MUST decide explicitly and
/// loudly what to do without a live VRAM signal — never silently
/// treat `None` as "no governance needed". (Contrast
/// [`detect_gpu`](crate::gpu::memory_manager) which hard-fails on a
/// genuinely GPU-less host; absence of a *live monitor adapter* on a
/// GPU host that has a working `GpuMemoryManager` is a different,
/// non-fatal condition.)
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
    // CUDA (NVML) and Vulkan (VK_EXT_memory_budget) live monitors are
    // not built yet — the governor logs this gap by name at its boot
    // site rather than silently degrading here.
    None
}

#[cfg(target_os = "macos")]
use super::MetalMonitor;

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: CpuMonitor declaring itself a non-cpu platform
    /// (would mislead the policy into trying GPU-specific code paths).
    ///
    /// Validated 2026-04-21: returned "cuda" from platform(), test fails.
    #[test]
    fn cpu_monitor_identifies_as_cpu_platform() {
        let m = CpuMonitor::new(8 * 1024 * 1024 * 1024);
        assert_eq!(m.platform(), "cpu");
        assert!(m.device_name().contains("CPU"));
    }

    /// What this catches: CpuMonitor's free_bytes not adjusting with
    /// pressure updates. Without this, the fallback monitor reports
    /// constant free=total and the policy thinks RAM is infinite.
    ///
    /// Validated 2026-04-21: removed pressure subtraction in free_bytes,
    /// test fails because free stays at total after pressure update.
    #[test]
    fn cpu_monitor_free_bytes_decreases_with_pressure() {
        let total = 8 * 1024 * 1024 * 1024u64;
        let m = CpuMonitor::new(total);
        assert_eq!(m.free_bytes(), total, "no pressure → all free");

        m.update_pressure(0.5);
        let half_used = m.free_bytes();
        assert!(
            half_used < total && half_used > total / 4,
            "50% pressure → roughly half free; got {half_used} of {total}"
        );

        m.update_pressure(1.0);
        assert!(
            m.free_bytes() < total / 10,
            "full pressure → near-zero free"
        );
    }

    /// What this catches: pressure value escaping the 0.0..1.0 range
    /// when caller pushes nonsense (e.g. update_pressure(2.5)). Clamping
    /// is the trait invariant; downstream policy assumes it.
    ///
    /// Validated 2026-04-21: removed clamp in update_pressure, test
    /// fails because pressure_rx returns 2.5 directly.
    #[test]
    fn cpu_monitor_clamps_pressure_to_unit_range() {
        let m = CpuMonitor::new(1024);
        m.update_pressure(2.5);
        assert!((0.0..=1.0).contains(&*m.pressure_rx().borrow()));
        m.update_pressure(-1.0);
        assert!((0.0..=1.0).contains(&*m.pressure_rx().borrow()));
    }

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
        let m = CpuMonitor::new(1024);
        let rx = m.pressure_rx();
        m.update_pressure(0.42);
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
