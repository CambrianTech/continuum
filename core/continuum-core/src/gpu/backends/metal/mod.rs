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

use crate::gpu::device_probe::{GpuDeviceProbe, GpuSample, MonitoredGpu};
use async_trait::async_trait;
use std::sync::Arc;

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

// Cadence is the base's (`device_probe::DEFAULT_TICK`, 1Hz) and this backend
// accepts it: 1Hz keeps Activity-Monitor parity, costs two Mach syscalls plus one
// Metal property read, and the OS only refreshes `host_vm_info` at ~1Hz internally
// so faster ticks buy no signal. Nothing Metal-specific to override.

/// The Metal-SPECIFIC half of GPU monitoring, and nothing else.
///
/// Holds only what is genuinely Apple's: the device handle, the constants probed
/// from it once, and how to ask Mach for a reading. Retention of the last good
/// sample, the unknown-until-first-sample state, the tick and the pressure signal
/// are [`MonitoredGpu`]'s — shared with CUDA and with the Vulkan/MLX adapters still
/// to come, so none of them can express those differently. See
/// [`crate::gpu::device_probe`] for why that split is load-bearing (this backend is
/// the one that got it wrong).
pub struct MetalProbe {
    device_name: String,
    total_bytes: u64,
    /// The Metal device handle, owned so each sample can read it.
    /// `metal::Device` is `Send + Sync` (auto-impl via `foreign_obj_type!` in the
    /// metal crate), so it crosses to the daemon task safely. Needed every tick on
    /// Discrete mode for `current_allocated_size`; unused on Unified but kept for
    /// symmetry + future IOReport hooks.
    device: metal::Device,
    /// Sampling strategy fixed at construction time.
    memory_mode: MemoryMode,
}

/// A Metal device under the shared monitoring machinery. The name is unchanged
/// because every caller's contract is unchanged — only the ownership of the common
/// half moved.
pub type MetalMonitor = MonitoredGpu<MetalProbe>;

impl MetalProbe {
    /// Probe for a Metal device. `None` if there is none (rare on a Mac; happens in
    /// headless build environments without `MTLCreateSystemDefaultDevice`).
    /// `None` is NOT a cue to substitute a CPU monitor — there is no CPU fallback
    /// (#980). A GPU host with no Metal device is a fail-loud condition the caller
    /// surfaces by name; it must never silently run "all CPU again" against
    /// fabricated numbers.
    pub fn detect() -> Option<Self> {
        let device = metal::Device::system_default()?;
        let total_bytes = device.recommended_max_working_set_size();
        if total_bytes == 0 {
            return None;
        }
        Some(Self {
            device_name: device.name().to_string(),
            total_bytes,
            memory_mode: if device.has_unified_memory() {
                MemoryMode::Unified
            } else {
                MemoryMode::Discrete
            },
            device,
        })
    }
}

impl MetalMonitor {
    /// Detect a Metal device and put it under the shared monitor. Returns an
    /// `Arc<Self>` because the daemon runner takes one (its task captures it for the
    /// process lifetime) and callers store it as `Arc<dyn GpuMonitor>` anyway.
    pub fn new() -> Option<Arc<Self>> {
        MetalProbe::detect().map(MonitoredGpu::spawn)
    }
}

#[async_trait]
impl GpuDeviceProbe for MetalProbe {
    fn platform(&self) -> &'static str {
        "metal"
    }

    fn daemon_name(&self) -> &'static str {
        "metal-gpu"
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// The real `hasUnifiedMemory` answer, detected at construction. This is the
    /// signal that tells the ResourceGovernor whether its VRAM and RAM ledgers
    /// describe one physical pool or two — never inferred from `target_os`, since
    /// an Intel Mac with a discrete AMD card also runs macOS and is NOT unified.
    fn memory_mode(&self) -> MemoryMode {
        self.memory_mode
    }

    /// Two Mach syscalls (Unified) or one Metal property read (Discrete). In-process
    /// and non-blocking, so this returns without ever yielding.
    ///
    /// Utilization / temperature / power stay `None` pending
    /// Phase 2.0a-IOReport (IOReport.framework for compute utilization, SMC channels
    /// for thermals + power). `None` is the correct answer for a sensor we do not
    /// read — the base maps it to "no reading", not to 0.
    async fn sample(&self) -> GpuSample {
        let (free_bytes, process_bytes) =
            sample_memory(self.memory_mode, self.total_bytes, &self.device);
        GpuSample {
            free_bytes,
            process_bytes,
            utilization: None,
            temperature_c: None,
            power_watts: None,
        }
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
///
/// Returns `None` for a term the platform could not report this tick. It does
/// NOT substitute: a failed `host_statistics64` used to read as `total` — "the
/// syscall broke, so assume the whole pool is free" — which is the single most
/// dangerous number this file can hand the governor. The caller keeps its last
/// MEASURED value instead and the reading simply does not refresh.
fn sample_memory(
    mode: MemoryMode,
    total: u64,
    device: &metal::Device,
) -> (Option<u64>, Option<u64>) {
    match mode {
        MemoryMode::Unified => (
            mach_ffi::read_system_free_bytes(),
            mach_ffi::read_process_phys_footprint(),
        ),
        MemoryMode::Discrete => {
            // Metal's own property — infallible, so this arm always measures.
            let allocated = device.current_allocated_size() as u64;
            (Some(total.saturating_sub(allocated)), Some(allocated))
        }
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
    // The monitor surface now arrives through the shared base, so the trait has to
    // be in scope explicitly — it is no longer implemented in this file.
    use crate::gpu::monitor::GpuMonitor;
    use tokio::time::Duration;

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
        // `expect` is the point, not a convenience: after a full tick on a live Mac
        // a reading MUST exist. This assertion also covers the state that used to be
        // unrepresentable — a monitor that never sampled and answered with `total`.
        let free = monitor
            .free_bytes()
            .expect("a live Mac must have a real free-bytes reading one tick after spawn");
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
        // Discrete reads a Metal property that cannot fail, so this arm must always
        // MEASURE — `None` here would mean the arm went fallible without saying so.
        let free = free.expect("discrete free is derived from an infallible Metal property");
        let proc = proc.expect("discrete allocated is derived from an infallible Metal property");
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
        // Both sides must be readings — a snapshot taken after a live tick that
        // carried no free value would mean the snapshot path lost it.
        let snap_free = snap.free_bytes.expect("snapshot carries the live free reading") as i64;
        let live_free = monitor.free_bytes().expect("monitor has a live free reading") as i64;
        let dt = (snap_free - live_free).unsigned_abs();
        assert!(
            dt < 1_000_000_000,
            "snapshot.free vs getter drift > 1GB: {dt}"
        );
    }
}
