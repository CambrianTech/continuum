//! `MetalMonitor` — `GpuMonitor` impl for macOS.
//!
//! Per §12 of `docs/architecture/PERSONA-CONTEXT-PAGING.md`: the prior
//! `GpuMemoryManager`'s Metal path treated `recommendedMaxWorkingSetSize`
//! as live free memory. It isn't — it's a STATIC lifetime hint from the
//! driver about the total budget the GPU can address. Process pressure
//! and system pressure both went unreported. A video game grabbing VRAM
//! never registered.
//!
//! This monitor distinguishes the four signals the policy actually needs:
//!
//! - `total_bytes` → Metal `MTLDevice.recommendedMaxWorkingSetSize` (still
//!   the right source for TOTAL — only wrong as a "free" proxy).
//! - `free_bytes` → Mach `host_statistics64(HOST_VM_INFO64)` summing
//!   free + speculative + inactive page counts × page size. System-wide
//!   free; the signal that catches "another app grabbed our headroom."
//! - `process_bytes` → Mach `task_info(mach_task_self(), TASK_VM_INFO)`
//!   → `phys_footprint`. This process's authoritative footprint, including
//!   unified-memory GPU buffers mapped into our address space.
//! - `utilization` / `temperature_c` / `power_watts` → IOReport.framework.
//!   No maintained Rust crate; requires our own Objective-C runtime shim.
//!   Phase 2.0a-IOReport ships separately. For now these return defaults
//!   (0.0 / None) so the policy can still rely on memory-pressure signals
//!   — the load-bearing signal — without blocking on the IOReport work.
//!
//! ## Unified vs discrete memory (task #163 fix)
//!
//! Apple Silicon (M1/M2/M3/...) uses **unified memory**: CPU and GPU
//! share one address space, so system VM stats ARE the right "GPU
//! free" signal. Intel Macs with a discrete GPU (AMD Radeon Pro Vega,
//! NVIDIA, etc.) use **discrete memory**: the GPU has its own VRAM
//! pool separate from system DRAM. Conflating the two pools causes
//! the `free <= total + 10%` invariant to fail catastrophically —
//! e.g., on a MacBookPro15,1 with a 4 GB Vega, system free pages
//! report 20 GB while VRAM total reports 4 GB.
//!
//! The fix: detect `MTLDevice.hasUnifiedMemory` at construction time
//! and branch the sampler. On unified, the existing Mach VM path is
//! correct. On discrete, use `MTLDevice.currentAllocatedSize` for
//! this-process GPU usage and derive `free = total - allocated`. The
//! discrete approximation OVER-reports free (it ignores other
//! processes' GPU use) but always satisfies `free <= total`, which is
//! the invariant the pressure-broker relies on. IOReport.framework
//! (Phase 2.0a) gives system-wide GPU usage and tightens this later.
//!
//! Module layout (Joel's modularize-to-simplify principle):
//!
//!   - `mod.rs` (this file) — `MetalMonitor` struct + `GpuMonitor` impl +
//!     tick spawn. The policy-facing surface.
//!   - `mach_ffi` — Mach VM FFI (structs, type aliases, raw read fns).
//!     Independently testable; separation caught the clashing-extern bug
//!     from the original mono-file version by making the FFI layer its
//!     own visible surface.

mod mach_ffi;

use crate::gpu::monitor::GpuMonitor;
use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Duration;

/// Memory accounting mode chosen at construction time based on the
/// Metal device's `hasUnifiedMemory` property.
///
/// This was a sampler-private detail on the stated assumption that the
/// distinction could stay internal. It could not: the ResourceGovernor
/// must know whether VRAM and RAM are one pool or two before it can hand
/// out bytes of either. The canonical enum now lives on the `GpuMonitor`
/// trait, and this is a re-export so the sampler's own `match` arms and
/// tests read unchanged.
/// On THIS platform the two arms mean:
///
/// - `Unified`: Apple Silicon — GPU and CPU share one address space. System VM
///   free pages ARE the GPU free signal.
/// - `Discrete`: Intel Mac with a discrete GPU (AMD / NVIDIA) — its own VRAM pool,
///   separate from system DRAM. System VM stats would conflate the pools and report
///   `free > total`, so this arm uses `MTLDevice.currentAllocatedSize()` for
///   this-process GPU usage and derives free from the device's working-set bound.
pub use crate::gpu::monitor::MemoryMode;

/// Tick cadence for the background sampler. 1Hz keeps Activity-Monitor
/// parity (its baseline cadence) and is essentially free per call —
/// each tick is two Mach syscalls + one Metal property read. Faster ticks
/// don't gain meaningful signal because the OS only updates `host_vm_info`
/// counters at ~1Hz internally.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

pub struct MetalMonitor {
    device_name: String,
    total_bytes: u64,
    free_bytes: AtomicU64,
    process_bytes: AtomicU64,
    /// The embedded publish channel — carries derived pressure (`1 - free/total`)
    /// as its snapshot. Ungated: this monitor reports a continuous signal; the
    /// pressure-broker decides what pressure level warrants backoff, so there is
    /// no daemon-side gate here. `GpuMonitor::pressure_rx` mints a receiver from it.
    channel: DaemonChannel<f32>,
    /// The Metal device handle, owned so the tick can sample it each cycle.
    /// `metal::Device` is `Send + Sync` (auto-impl via `foreign_obj_type!` in the
    /// metal crate), so it crosses to the daemon task safely. Needed every tick on
    /// Discrete mode for `current_allocated_size`; unused on Unified but kept for
    /// symmetry + future IOReport hooks.
    device: metal::Device,
    /// Sampling strategy fixed at construction time. Read-only after
    /// init; exposed via `memory_mode()` for telemetry / tests that
    /// branch on hardware shape.
    memory_mode: MemoryMode,
}

impl MetalMonitor {
    /// Construct a MetalMonitor and spawn it on the shared [`Daemon`] runner.
    /// Returns `None` if no Metal device is available (rare on a Mac;
    /// happens in headless build environments without `MTLCreateSystemDefaultDevice`).
    /// `None` is NOT a cue to substitute a CPU monitor — there is no CPU
    /// fallback (#980). A GPU host with no Metal device is a fail-loud
    /// condition the caller surfaces by name; it must never silently run
    /// "all CPU again" against fabricated numbers.
    ///
    /// Returns an `Arc<Self>` because [`spawn_daemon`] takes one (the runner's
    /// task captures it for the process lifetime) and callers store the monitor
    /// as `Arc<dyn GpuMonitor>` anyway.
    pub fn new() -> Option<Arc<Self>> {
        let device = metal::Device::system_default()?;
        let total_bytes = device.recommended_max_working_set_size();
        let device_name = device.name().to_string();
        if total_bytes == 0 {
            return None;
        }
        let memory_mode = if device.has_unified_memory() {
            MemoryMode::Unified
        } else {
            MemoryMode::Discrete
        };

        let monitor = Arc::new(Self {
            device_name,
            total_bytes,
            free_bytes: AtomicU64::new(total_bytes),
            process_bytes: AtomicU64::new(0),
            channel: DaemonChannel::ungated(0.0f32),
            device,
            memory_mode,
        });

        // The shared runner owns the interval + per-tick catch_unwind. The
        // previous hand-rolled sampler had NO isolation — a panic in a Mach FFI
        // read would have killed GPU monitoring for the whole process; now it
        // loses one tick and resumes against the last-good snapshot. The task
        // captures this Arc for the process lifetime (no "stop monitoring" case).
        let _ = spawn_daemon(monitor.clone());
        Some(monitor)
    }

    /// Sampling strategy chosen at construction. Exposed for
    /// telemetry + test branching. Production callers should use
    /// the trait methods, which abstract the mode away.
    pub fn memory_mode(&self) -> MemoryMode {
        self.memory_mode
    }
}

#[async_trait]
impl Daemon for MetalMonitor {
    type Snapshot = f32;

    fn name(&self) -> &'static str {
        "metal-gpu"
    }

    fn cadence(&self) -> Duration {
        TICK_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<f32> {
        &self.channel
    }

    /// Refresh free + process bytes and publish derived pressure. Pure atomic
    /// stores + one `sample_memory` call (two Mach syscalls + one Metal property
    /// read) — no lock, no await held across state, so it slots straight into the
    /// runner's per-tick `catch_unwind`.
    async fn tick(&self) {
        let (free, proc) = sample_memory(self.memory_mode, self.total_bytes, &self.device);
        self.free_bytes.store(free, Ordering::Relaxed);
        self.process_bytes.store(proc, Ordering::Relaxed);

        // Pressure: 1.0 - free/total. Clamped to [0,1] for sanity — on Unified,
        // free can briefly exceed total in some host_statistics64 reporting
        // windows due to inactive→free transitions racing with our read; on
        // Discrete, free is `saturating_sub(total, allocated)` which never
        // exceeds total, but the clamp keeps the shape uniform across modes.
        let pressure = if self.total_bytes > 0 {
            1.0 - (free as f32 / self.total_bytes as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.channel.publish(pressure);
    }
}

/// Read (free_bytes, process_bytes) for the current tick. Branches
/// on memory mode:
///
/// - **Unified**: `read_system_free_bytes()` for free (host VM
///   stats — accurate for shared-pool Apple Silicon) +
///   `read_process_phys_footprint()` for process (includes unified
///   GPU buffers mapped into our address space).
///
/// - **Discrete**: `device.current_allocated_size()` for this
///   process's GPU footprint; free = `total - allocated`. This
///   OVER-reports free because it ignores GPU memory consumed by
///   other processes on the same device — IOReport.framework
///   (Phase 2.0a) tightens this when wired. Bounded by `total` so
///   the broker's pressure invariants always hold.
fn sample_memory(mode: MemoryMode, total: u64, device: &metal::Device) -> (u64, u64) {
    match mode {
        MemoryMode::Unified => {
            let free = mach_ffi::read_system_free_bytes().unwrap_or(total);
            let proc = mach_ffi::read_process_phys_footprint().unwrap_or(0);
            (free, proc)
        }
        MemoryMode::Discrete => {
            let allocated = device.current_allocated_size() as u64;
            let free = total.saturating_sub(allocated);
            (free, allocated)
        }
    }
}

impl GpuMonitor for MetalMonitor {
    fn platform(&self) -> &'static str {
        "metal"
    }

    /// The real `hasUnifiedMemory` answer, detected at construction. This is the
    /// signal that tells the ResourceGovernor whether its VRAM and RAM ledgers
    /// describe one physical pool or two — never inferred from `target_os`, since
    /// an Intel Mac with a discrete AMD card also runs macOS and is NOT unified.
    fn memory_mode(&self) -> MemoryMode {
        self.memory_mode
    }
    fn device_name(&self) -> &str {
        &self.device_name
    }
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    fn free_bytes(&self) -> u64 {
        self.free_bytes.load(Ordering::Relaxed)
    }
    fn process_bytes(&self) -> u64 {
        self.process_bytes.load(Ordering::Relaxed)
    }
    fn utilization(&self) -> f32 {
        // TODO Phase 2.0a-IOReport: live GPU compute utilization via
        // IOReport.framework. Returns 0.0 until then — policy can still
        // make memory-pressure decisions without it.
        0.0
    }
    fn temperature_c(&self) -> Option<f32> {
        // TODO Phase 2.0a-IOReport: SMC / IOReport thermal sensors.
        None
    }
    fn power_watts(&self) -> Option<f32> {
        // TODO Phase 2.0a-IOReport: SMC / IOReport power channels.
        None
    }
    fn pressure_rx(&self) -> watch::Receiver<f32> {
        self.channel.handle().subscribe()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────
//
// FFI-layer tests live in `mach_ffi::tests` — struct-size arithmetic,
// field offsets, raw Mach call correctness. The tests below test the
// MONITOR integration: trait wiring, tick task, pressure derivation.

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: `MetalMonitor::new()` failing to detect a
    /// Metal device on a Mac (CI baseline check). If this returns None
    /// in CI on a Mac runner, MTLCreateSystemDefaultDevice is broken —
    /// almost certainly an environment issue (headless without GPU, or
    /// metal crate ABI mismatch).
    ///
    /// Validated 2026-04-21: returned None when MetalDevice initializer
    /// was patched to fail; test fails as expected; reverted.
    #[tokio::test(flavor = "multi_thread")]
    async fn new_returns_some_on_macos_with_metal_device() {
        let monitor = MetalMonitor::new();
        assert!(
            monitor.is_some(),
            "MetalMonitor::new() returned None on macOS — Metal device should be available"
        );
    }

    /// Force a Metal allocation that's guaranteed to show up in
    /// `currentAllocatedSize` (Discrete mode) AND inflate process
    /// phys_footprint (Unified mode). Returns the buffer; the caller
    /// holds it for the test's lifetime so Metal doesn't free it
    /// before the sampler reads.
    ///
    /// Why both tests below call this: on Discrete devices a freshly
    /// constructed `MetalMonitor` may observe `current_allocated_size
    /// == 0` if Metal's internal command-queue allocations haven't
    /// happened yet. A real allocation eliminates the timing race and
    /// pins the invariants regardless of when the sampler runs.
    fn force_metal_allocation() -> metal::Buffer {
        let device = metal::Device::system_default().expect("system_default device");
        device.new_buffer(
            16 * 1024 * 1024, // 16 MB — large enough to register clearly
            metal::MTLResourceOptions::StorageModePrivate,
        )
    }

    /// What this catches: total_bytes, free_bytes, process_bytes returning
    /// nonsensical values (zero, way larger than physical RAM, etc.).
    /// Sanity bounds: total > 1GB (any Mac), free <= total + 10% (slack
    /// for inactive→free races on Unified mode), process > 0 + < total.
    ///
    /// Validated 2026-04-21: multiplied read_system_free_bytes return
    /// by 100 (free → 26 GB × 100 = 2.6 TB), test fails on the
    /// `free <= total + 10%` assertion; reverted.
    ///
    /// Task #163 fix (2026-06-05): Mac Intel + AMD discrete now uses
    /// `MTLDevice.currentAllocatedSize()` instead of system VM stats,
    /// so the invariants hold uniformly across Unified + Discrete.
    /// Force-allocate a Metal buffer up front so the proc-bytes
    /// invariant holds even when the test process hasn't done any
    /// other GPU work.
    #[tokio::test(flavor = "multi_thread")]
    async fn memory_signals_are_within_sane_bounds() {
        let _hold = force_metal_allocation();
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        // Wait one tick so the background sampler has refreshed values.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let total = monitor.total_bytes();
        let free = monitor.free_bytes();
        let proc = monitor.process_bytes();
        eprintln!(
            "[metal-monitor] mode={:?} total={} ({} GB) free={} ({} GB) process={} ({} MB)",
            monitor.memory_mode(),
            total,
            total / 1_000_000_000,
            free,
            free / 1_000_000_000,
            proc,
            proc / 1_000_000
        );
        assert!(total > 1_000_000_000, "total < 1GB: {total}");
        assert!(
            free <= total + total / 10,
            "free ({free}) > total + 10% ({})",
            total + total / 10
        );
        assert!(
            proc > 0,
            "process bytes should be > 0 (we forced an allocation)"
        );
        assert!(proc < total, "process bytes ({proc}) >= total ({total})");
    }

    /// What this catches: pressure receiver staying at 0.0 forever (tick
    /// task never updated it) OR landing outside [0, 1]. After the first
    /// tick, pressure must reflect real (free, total) ratio.
    ///
    /// Validated 2026-04-21: commented out the pressure_tx.send() in the
    /// background tick (sampler stays stuck at initial 0.0), test fails
    /// on the `p > 0.0` assertion; reverted.
    ///
    /// Task #163 fix (2026-06-05): force a Metal allocation so
    /// pressure is non-zero on Discrete devices too (where pressure =
    /// allocated/total and a fresh device may report
    /// `currentAllocatedSize == 0` until something is actually
    /// allocated). Unified devices always have organic system
    /// pressure so the allocation is redundant-but-harmless there.
    #[tokio::test(flavor = "multi_thread")]
    async fn pressure_updates_after_first_tick() {
        let _hold = force_metal_allocation();
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let p = *monitor.pressure_rx().borrow();
        eprintln!(
            "[metal-monitor] mode={:?} pressure after first tick: {p:.6}",
            monitor.memory_mode()
        );
        assert!((0.0..=1.0).contains(&p), "pressure {p} outside [0,1]");
        assert!(
            p > 0.0,
            "pressure unchanged from initial 0.0 after first tick — \
             sampler may be stuck OR force_metal_allocation didn't register"
        );
    }

    /// What this catches: regression on the unified-vs-discrete
    /// branching. If a refactor accidentally hard-codes one mode (or
    /// the metal crate's `hasUnifiedMemory` accessor changes shape),
    /// this test fires.
    ///
    /// We can't assert the specific mode (depends on the runner —
    /// Apple Silicon CI returns Unified, Intel + discrete returns
    /// Discrete) but we can pin the invariants: the mode is one of
    /// the two variants, and it matches `device.has_unified_memory()`
    /// at this moment.
    #[tokio::test(flavor = "multi_thread")]
    async fn memory_mode_matches_device_unified_flag() {
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        let device = metal::Device::system_default().expect("device");
        let expected = if device.has_unified_memory() {
            MemoryMode::Unified
        } else {
            MemoryMode::Discrete
        };
        assert_eq!(
            monitor.memory_mode(),
            expected,
            "MetalMonitor memory_mode must agree with the device's hasUnifiedMemory at construction time"
        );
    }

    /// What this catches: `sample_memory` on Discrete mode returning
    /// `free > total` for ANY non-pathological allocated value.
    /// Tests the pure function directly with a constructed device so
    /// the invariant holds even when there's no Metal device (e.g.
    /// future cross-compile / mock contexts).
    ///
    /// Pure-function coverage of the discrete branch — closes the
    /// "free can exceed total" class of bug for good. Doctrine:
    /// [[test-fixtures-are-system-primitives]] — `sample_memory` is
    /// `pub(super)` exposed for this test (and future ones); not a
    /// `#[cfg(test)]` helper.
    #[test]
    fn sample_memory_discrete_never_exceeds_total() {
        let Some(device) = metal::Device::system_default() else {
            return; // No Metal device — test is moot, the new() path bails too
        };
        let total = device.recommended_max_working_set_size();
        let (free, proc) = sample_memory(MemoryMode::Discrete, total, &device);
        assert!(
            free <= total,
            "discrete free ({free}) must not exceed total ({total})"
        );
        assert!(
            proc <= total,
            "discrete proc ({proc}) must not exceed total ({total})"
        );
    }

    /// What this catches: the trait's snapshot() default impl producing
    /// inconsistent values vs the individual getters. snapshot is what
    /// the FootprintRegistry sanity check uses to compare; if it drifts
    /// from total_bytes/process_bytes the cross-check goes wrong.
    ///
    /// Validated 2026-04-21: changed `platform()` to return
    /// "wrong-platform", test fails on `assert_eq!(snap.platform, "metal")`;
    /// reverted.
    #[tokio::test(flavor = "multi_thread")]
    async fn snapshot_matches_individual_getters() {
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let snap = monitor.snapshot();
        assert_eq!(snap.platform, "metal");
        assert_eq!(snap.total_bytes, monitor.total_bytes());
        assert_eq!(snap.device_name, monitor.device_name());
        let dt = (snap.free_bytes as i64 - monitor.free_bytes() as i64).unsigned_abs();
        assert!(
            dt < 1_000_000_000,
            "snapshot.free vs getter drift > 1GB: {dt}"
        );
    }
}
