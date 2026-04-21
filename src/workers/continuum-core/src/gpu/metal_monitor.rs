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
//! Tick: a single tokio task ticks once per second, refreshes the four
//! cheap-to-read values, and pushes the derived pressure (1.0 - free/total)
//! into the `watch` channel. The policy reads from `pressure_rx()` on its
//! rebalance loop.

use crate::gpu::monitor::GpuMonitor;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;
use tokio::time::Duration;

/// Tick cadence for the background sampler. 1Hz keeps Activity-Monitor
/// parity (its baseline cadence) and is essentially free per call —
/// each tick is two Mach syscalls + one Metal property read. Faster ticks
/// don't gain meaningful signal because the OS only updates `host_vm_info`
/// counters at ~1Hz internally.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

pub struct MetalMonitor {
    device_name: String,
    total_bytes: u64,
    free_bytes: Arc<AtomicU64>,
    process_bytes: Arc<AtomicU64>,
    pressure_rx: watch::Receiver<f32>,
}

impl MetalMonitor {
    /// Construct a MetalMonitor and spawn its background tick task.
    /// Returns `None` if no Metal device is available (rare on a Mac;
    /// happens in headless build environments without `MTLCreateSystemDefaultDevice`).
    /// Caller falls back to `CpuMonitor` in that case — same trait, no
    /// branch in policy code.
    pub fn new() -> Option<Self> {
        let device = metal::Device::system_default()?;
        let total_bytes = device.recommended_max_working_set_size();
        let device_name = device.name().to_string();
        if total_bytes == 0 {
            return None;
        }

        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);
        let monitor = Self {
            device_name,
            total_bytes,
            free_bytes: Arc::new(AtomicU64::new(total_bytes)),
            process_bytes: Arc::new(AtomicU64::new(0)),
            pressure_rx,
        };

        // Spawn the background sampler. Lives for the process lifetime —
        // when the last Arc drop happens the channel closes and the task
        // exits naturally. We don't store a JoinHandle because there's no
        // "stop monitoring" use case; if the process is alive, we want
        // signals.
        let free_bytes = monitor.free_bytes.clone();
        let process_bytes = monitor.process_bytes.clone();
        let total = total_bytes;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(TICK_INTERVAL);
            // First tick fires immediately; subsequent ticks at TICK_INTERVAL.
            loop {
                tick.tick().await;
                if pressure_tx.is_closed() {
                    break;
                }
                let free = read_system_free_bytes().unwrap_or(total);
                let proc = read_process_phys_footprint().unwrap_or(0);
                free_bytes.store(free, Ordering::Relaxed);
                process_bytes.store(proc, Ordering::Relaxed);

                // Pressure: 1.0 - free/total. Clamped to [0,1] for sanity
                // (free can briefly exceed total in some host_statistics64
                // reporting windows due to inactive→free transitions
                // racing with our read).
                let pressure = if total > 0 {
                    1.0 - (free as f32 / total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let _ = pressure_tx.send(pressure);
            }
        });

        Some(monitor)
    }
}

impl GpuMonitor for MetalMonitor {
    fn platform(&self) -> &'static str {
        "metal"
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
        self.pressure_rx.clone()
    }
}

// ─── Mach FFI shims ──────────────────────────────────────────────────
//
// libc declares `task_info` with its own (typed) signature; declaring it
// again here would cause a "clashing extern declarations" warning AND
// a real ABI mismatch at link time. We tunnel through libc's call-site
// where possible and only declare what libc doesn't expose: host_statistics64
// (libc has it but with a different flavor type) and the task_vm_info
// struct shape.
//
// All `unsafe` surfaces are confined to read_system_free_bytes /
// read_process_phys_footprint — the GpuMonitor impl above is safe.

#[allow(non_camel_case_types)]
type kern_return_t = libc::c_int;
#[allow(non_camel_case_types)]
type natural_t = libc::c_uint;
#[allow(non_camel_case_types)]
type integer_t = libc::c_int;
#[allow(non_camel_case_types)]
type mach_msg_type_number_t = natural_t;

// `host_flavor_t` and `task_flavor_t` are both `natural_t` (u32), not
// `integer_t` (i32). The Mach headers use natural_t even though most
// flavor constants fit in i32 — passing i32 risks ABI mismatch on
// platforms where the calling convention sign-extends differently.
const HOST_VM_INFO64: natural_t = 4;
const TASK_VM_INFO: natural_t = 22;

// Sized to match `mach/vm_statistics.h`'s `vm_statistics64_data_t`.
// Stable on macOS 10.7+. We read free + speculative + inactive as
// "available to take" — same definition Activity Monitor's "Memory
// Free" column uses.
#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
struct vm_statistics64 {
    free_count: natural_t,
    active_count: natural_t,
    inactive_count: natural_t,
    wire_count: natural_t,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: natural_t,
    speculative_count: natural_t,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: natural_t,
    throttled_count: natural_t,
    external_page_count: natural_t,
    internal_page_count: natural_t,
    total_uncompressed_pages_in_compressor: u64,
}

// HOST_VM_INFO64_COUNT = sizeof(vm_statistics64) / sizeof(integer_t)
const HOST_VM_INFO64_COUNT: mach_msg_type_number_t = (std::mem::size_of::<vm_statistics64>()
    / std::mem::size_of::<integer_t>())
    as mach_msg_type_number_t;

// task_vm_info — only `phys_footprint` is load-bearing for us, but we
// must declare the full struct so task_info copies the right number of
// bytes into our pointer. Layout from `mach/task_info.h`. Stable on
// macOS 10.10+ (when phys_footprint was introduced).
#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
struct task_vm_info {
    virtual_size: u64,
    region_count: integer_t,
    page_size: integer_t,
    resident_size: u64,
    resident_size_peak: u64,
    device: u64,
    device_peak: u64,
    internal: u64,
    internal_peak: u64,
    external: u64,
    external_peak: u64,
    reusable: u64,
    reusable_peak: u64,
    purgeable_volatile_pmap: u64,
    purgeable_volatile_resident: u64,
    purgeable_volatile_virtual: u64,
    compressed: u64,
    compressed_peak: u64,
    compressed_lifetime: u64,
    phys_footprint: u64,
    min_address: u64,
    max_address: u64,
    // Newer fields (10.15+) — declared so we get the full extent of the
    // struct kernel may write. Using TASK_VM_INFO_COUNT (older flavor)
    // instead of TASK_VM_INFO_REV1_COUNT keeps us compatible with the
    // 10.10 baseline; kernel writes only the fields the count says.
    ledger_phys_footprint_peak: u64,
    ledger_purgeable_nonvolatile: u64,
    ledger_purgeable_novolatile_compressed: u64,
    ledger_purgeable_volatile: u64,
    ledger_purgeable_volatile_compressed: u64,
    ledger_tag_network_nonvolatile: u64,
    ledger_tag_network_nonvolatile_compressed: u64,
    ledger_tag_network_volatile: u64,
    ledger_tag_network_volatile_compressed: u64,
    ledger_tag_media_footprint: u64,
    ledger_tag_media_footprint_compressed: u64,
    ledger_tag_media_nofootprint: u64,
    ledger_tag_media_nofootprint_compressed: u64,
    ledger_tag_graphics_footprint: u64,
    ledger_tag_graphics_footprint_compressed: u64,
    ledger_tag_graphics_nofootprint: u64,
    ledger_tag_graphics_nofootprint_compressed: u64,
    ledger_tag_neural_footprint: u64,
    ledger_tag_neural_footprint_compressed: u64,
    ledger_tag_neural_nofootprint: u64,
    ledger_tag_neural_nofootprint_compressed: u64,
}

const TASK_VM_INFO_COUNT: mach_msg_type_number_t = (std::mem::size_of::<task_vm_info>()
    / std::mem::size_of::<integer_t>())
    as mach_msg_type_number_t;

// Mach symbols not declared by libc. Use renamed Rust binding
// (`host_statistics64_raw`) so we don't clash with anything libc may
// declare under the same name. The `link_name` attribute resolves to
// the actual Mach symbol at link time.
unsafe extern "C" {
    #[link_name = "host_statistics64"]
    fn host_statistics64_raw(
        host_priv: libc::host_t,
        flavor: natural_t,
        host_info_out: *mut integer_t,
        host_info_outCnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;
    #[link_name = "task_info"]
    fn task_info_raw(
        target_task: libc::task_t,
        flavor: natural_t,
        task_info_out: *mut integer_t,
        task_info_outCnt: *mut mach_msg_type_number_t,
    ) -> kern_return_t;
}

const KERN_SUCCESS: kern_return_t = 0;

/// System-wide free bytes — what Activity Monitor reports as "Memory Free."
/// Sum of (free + speculative + inactive) page counts × page size.
/// Returns None on Mach error so the caller can fall back to "assume total"
/// without baking in a wrong number.
fn read_system_free_bytes() -> Option<u64> {
    let mut info = vm_statistics64::default();
    let mut count = HOST_VM_INFO64_COUNT;
    // libc::mach_host_self is deprecated in favor of the mach2 crate.
    // We don't yet have mach2 in deps and adding it for one symbol is
    // its own commit — silence here, switch in a follow-up if mach2
    // earns its dep weight elsewhere.
    #[allow(deprecated)]
    let kr = unsafe {
        host_statistics64_raw(
            libc::mach_host_self(),
            HOST_VM_INFO64,
            &mut info as *mut vm_statistics64 as *mut integer_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }
    // Page size: sysconf(_SC_PAGESIZE) is the userspace-stable accessor.
    // vm_kernel_page_size is a kernel-only symbol — calling it from
    // userspace SIGBUSes (caught 2026-04-21). Apple Silicon Macs use
    // 16384, x86_64 Macs use 4096; both via sysconf so we don't bake in.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    let pages = info.free_count as u64 + info.speculative_count as u64 + info.inactive_count as u64;
    Some(pages.saturating_mul(page_size))
}

/// This process's `phys_footprint` — the same number macOS uses for its
/// memory-pressure computations and what `top`/`Activity Monitor` show
/// in the "Memory" column. Includes unified-memory Metal buffers mapped
/// into our address space.
fn read_process_phys_footprint() -> Option<u64> {
    let mut info = task_vm_info::default();
    let mut count = TASK_VM_INFO_COUNT;
    // Same deprecated-libc reason as read_system_free_bytes above.
    #[allow(deprecated)]
    let kr = unsafe {
        task_info_raw(
            libc::mach_task_self(),
            TASK_VM_INFO,
            &mut info as *mut task_vm_info as *mut integer_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }
    Some(info.phys_footprint)
}

// ─── Tests ────────────────────────────────────────────────────────────

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

    /// What this catches: total_bytes, free_bytes, process_bytes returning
    /// nonsensical values (zero, way larger than physical RAM, etc.).
    /// Sanity bounds: total > 1GB (any Mac), free <= total + 10% (slack
    /// for inactive→free races), process > 0 + < total.
    #[tokio::test(flavor = "multi_thread")]
    async fn memory_signals_are_within_sane_bounds() {
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        // Wait one tick so the background sampler has refreshed values.
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let total = monitor.total_bytes();
        let free = monitor.free_bytes();
        let proc = monitor.process_bytes();
        eprintln!(
            "[metal-monitor] total={} ({} GB) free={} ({} GB) process={} ({} MB)",
            total,
            total / 1_000_000_000,
            free,
            free / 1_000_000_000,
            proc,
            proc / 1_000_000
        );
        assert!(total > 1_000_000_000, "total < 1GB: {total}");
        // Free can briefly exceed total during inactive→free transitions
        // (Mach reports them in different counters that race). Allow 10%
        // headroom on the upper bound.
        assert!(
            free <= total + total / 10,
            "free ({free}) > total + 10% ({})",
            total + total / 10
        );
        assert!(proc > 0, "process bytes should be > 0 (we're running)");
        assert!(proc < total, "process bytes ({proc}) >= total ({total})");
    }

    /// What this catches: pressure receiver staying at 0.0 forever (tick
    /// task never updated it) OR landing outside [0, 1]. After the first
    /// tick, pressure must reflect real (free, total) ratio.
    #[tokio::test(flavor = "multi_thread")]
    async fn pressure_updates_after_first_tick() {
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        // The background sampler runs immediately on first tick. Wait
        // ~1.2s to give it room.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let p = *monitor.pressure_rx().borrow();
        eprintln!("[metal-monitor] pressure after first tick: {p:.3}");
        assert!((0.0..=1.0).contains(&p), "pressure {p} outside [0,1]");
        // We're a real process running real tests; pressure must be > 0.
        // If it's exactly 0 either the tick didn't fire or free == total.
        assert!(
            p > 0.0,
            "pressure unchanged from initial 0.0 after first tick — sampler may be stuck"
        );
    }

    /// What this catches: the trait's snapshot() default impl producing
    /// inconsistent values vs the individual getters. snapshot is what
    /// the FootprintRegistry sanity check uses to compare; if it drifts
    /// from total_bytes/process_bytes the cross-check goes wrong.
    #[tokio::test(flavor = "multi_thread")]
    async fn snapshot_matches_individual_getters() {
        let monitor = MetalMonitor::new().expect("MetalMonitor on macOS");
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let snap = monitor.snapshot();
        assert_eq!(snap.platform, "metal");
        assert_eq!(snap.total_bytes, monitor.total_bytes());
        assert_eq!(snap.device_name, monitor.device_name());
        // free + process come from atomic stores that may race with the
        // tick — allow a small window where a tick fired between snapshot
        // and individual reads. They should be within ONE tick's drift.
        let dt = (snap.free_bytes as i64 - monitor.free_bytes() as i64).unsigned_abs();
        assert!(
            dt < 1_000_000_000,
            "snapshot.free vs getter drift > 1GB: {dt}"
        );
    }
}
