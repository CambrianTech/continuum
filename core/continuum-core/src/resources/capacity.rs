//! The scan-ingest seam — where a hardware monitor becomes a per-kind ceiling.
//!
//! The [governor](super::governor) is clock-free and hardware-free: it is *told*
//! the capacity of each [`ResourceKind`] and reconciles against it. This module
//! is the thin adapter that turns a live monitor (GPU / RAM / disk) into that
//! number. The [daemon](super::daemon) holds a `Vec<Arc<dyn CapacitySource>>`
//! and, every tick, reads each one into `governor.set_capacity`.
//!
//! # Non-blocking contract (load-bearing)
//!
//! [`CapacitySource::ceiling_bytes`] MUST be a **cached, non-blocking read** — it
//! is called on the daemon's hot tick and its value is then used while the
//! governor's accounting lock is held. It must NEVER probe hardware inline (no
//! `nvidia-smi` fork, no blocking ioctl). The concrete monitors already run
//! their own scan tasks on their own cadence and publish cached values
//! (`GpuMonitor`, `MemoryPressureMonitor`); a source reads the latest snapshot,
//! never triggers a fresh scan. The daemon additionally snapshots every ceiling
//! *before* taking its accounting lock, so even a momentarily costly read can
//! never stall a lease `acquire`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::gpu::monitor::GpuMonitor;

use super::lease::ResourceKind;

/// One resource axis's two live numbers, sourced from a hardware monitor: the
/// **fixed ceiling** (the hardware upper limit this node fundamentally has, less
/// a safety reserve) and the **physical usage** (what is bodily resident right
/// now — everyone's bytes, ours and external alike, `total − free`).
///
/// The un-inversion (task #79): capacity is a *stable* fact about the machine
/// ("this node is a 24 GB device"), not a moving remainder. What moves is
/// physical usage. The authority commits the global remainder
/// `capacity − max(granted, physical_used)` — honest whether the bytes are gone
/// to a lease we granted, to a model we hold resident but never leased (serving),
/// or to a game the OS handed VRAM. That single measured usage number is why the
/// board is no longer blind to `granted:0` residency, and why a grid peer sees
/// both what a node *is* (fixed capacity) and what it currently *bears*
/// (physical usage) as two separate, negotiable facts.
pub trait CapacitySource: Send + Sync {
    fn kind(&self) -> ResourceKind;

    /// The FIXED hardware ceiling in bytes — physical total minus a safety
    /// reserve. Near-constant (it only changes if the device itself changes).
    /// Cached, non-blocking — see the module contract.
    fn ceiling_bytes(&self) -> u64;

    /// Bytes of this kind physically resident RIGHT NOW across *everyone* —
    /// `total − free` as the hardware monitor sees it, counting our leases, our
    /// unleased residency, and every external process alike. This is the ground
    /// truth the authority nets against the fixed ceiling. Cached, non-blocking.
    ///
    /// Defaults to `0`: a source with no physical monitor behind it (the
    /// deterministic test driver, or a kind we scan only for its ceiling)
    /// contributes no usage, so `available` degrades cleanly to
    /// `capacity − granted` — the pre-un-inversion behavior.
    fn used_bytes(&self) -> u64 {
        0
    }
}

/// VRAM source from a [`GpuMonitor`]. Reports the device's fixed usable total
/// (less a driver/OS safety reserve) as the ceiling, and the live `total − free`
/// as physical usage. When a game grabs VRAM, `free` drops and `used_bytes`
/// rises — the ceiling does NOT move; the oversubscription surfaces as
/// `physical_used > capacity`, which is exactly what the daemon's reconcile
/// claws our leases back against. This is the number that fixes the
/// `host_budget()` OOM bug: serving no longer commits a fraction of *total* VRAM
/// blind to Bevy/LiveKit — it commits against `capacity − everything-resident`.
pub struct GpuCapacitySource {
    monitor: Arc<dyn GpuMonitor>,
    /// Bytes held back from the lease pool unconditionally (driver/OS headroom).
    reserve_bytes: u64,
}

impl GpuCapacitySource {
    pub fn new(monitor: Arc<dyn GpuMonitor>, reserve_bytes: u64) -> Self {
        Self {
            monitor,
            reserve_bytes,
        }
    }
}

impl CapacitySource for GpuCapacitySource {
    fn kind(&self) -> ResourceKind {
        ResourceKind::Vram
    }

    fn ceiling_bytes(&self) -> u64 {
        // The FIXED usable ceiling: device total minus the driver/OS reserve. It
        // does NOT move when a game or another process grabs VRAM — that shows up
        // in `used_bytes`, not here. `total_bytes` is a cached accessor on the
        // monitor (it scans on its own task) — non-blocking.
        self.monitor
            .total_bytes()
            .saturating_sub(self.reserve_bytes)
    }

    fn used_bytes(&self) -> u64 {
        // Everything physically resident right now, ours and external alike:
        // total − free. `free_bytes`/`total_bytes` are cached monitor accessors —
        // non-blocking. Saturating so a momentary free > total glitch reads 0.
        self.monitor
            .total_bytes()
            .saturating_sub(self.monitor.free_bytes())
    }
}

/// The two live host-RAM numbers, behind a trait so a test can drive them without a
/// running [`MemoryPressureMonitor`] — same injection shape as `RenderSurface` and
/// `StagingResidency`.
pub trait HostMemoryReader: Send + Sync {
    /// Total physical RAM. A CONSTANT for the machine, so implementations read it once.
    fn total_bytes(&self) -> u64;
    /// Free physical RAM as of the monitor's last poll, or `None` before the first one.
    /// `None` is "unknown", NEVER "zero" — see [`HostRamCapacitySource::used_bytes`].
    fn available_bytes(&self) -> Option<u64>;
}

/// Reads the live host through the [`MemoryPressureMonitor`]'s already-published numbers.
///
/// Total is probed ONCE at construction (`sysinfo::total_memory()`) because physical RAM is a
/// stable fact about the machine, and available comes from
/// [`current_available_bytes`](crate::system_resources::memory_pressure::current_available_bytes)
/// — a lock-free atomic the monitor loop refreshes every 2s. Both reads satisfy the module's
/// non-blocking contract; neither touches the reporter list (`budget_snapshot()` DOES — it locks
/// and calls `budget()`/`report()` on every reporter, which must never happen on the daemon's
/// hot tick with the accounting lock held).
pub struct LiveHostMemory {
    total: u64,
}

impl Default for LiveHostMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveHostMemory {
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        Self {
            total: sys.total_memory(),
        }
    }
}

impl HostMemoryReader for LiveHostMemory {
    fn total_bytes(&self) -> u64 {
        self.total
    }
    fn available_bytes(&self) -> Option<u64> {
        crate::system_resources::memory_pressure::current_available_bytes()
    }
}

/// HOST RAM as a governed axis (#56).
///
/// # Why this did not exist until 2026-08-19, and what it cost
///
/// The governor has carried [`ResourceKind::Ram`] through leases, footprints, reclaim, and
/// `available_for` since #56 — but the daemon's `capacity_sources` vec held ONLY the GPU source.
/// So `capacity(Ram)` was 0, and therefore `available_for(_, Ram)` returned 0 for EVERY consumer,
/// permanently. Serving, Bevy and Voice never noticed because they lease VRAM. The first
/// consumer to plan against RAM — benchmark staging — was refused instantly, on a box with tens
/// of gigabytes free.
///
/// The machinery was complete and one wire was missing, which is the failure shape this
/// codebase keeps producing: a capability that is structurally unreachable while looking
/// finished. It surfaced only because a new consumer actually tried to use it.
///
/// # The cold-boot trap, and why the ceiling is probed once
///
/// `total_bytes` from the monitor's snapshot is 0 until its first poll. A ceiling of 0 is not
/// "no RAM" — it is "not measured yet" — and reporting it would reproduce the exact defect
/// above for the first seconds of every boot. Physical RAM does not change, so it is read ONCE
/// at construction and the ceiling is correct immediately.
pub struct HostRamCapacitySource<R: HostMemoryReader> {
    reader: R,
    /// Bytes held back unconditionally so we never lease the host into the OOM killer's reach.
    reserve_bytes: u64,
}

/// Bytes held back from the RAM ceiling so leases can never reach the OOM killer.
///
/// Proportional with bounds, because neither end works as a constant: a flat 4 GiB is
/// noise on a 128 GB M5 and HALF the machine on an 8 GB box. An eighth, clamped to
/// [1 GiB, 8 GiB], keeps the OS and the untracked slack solvent at every tier this
/// substrate targets. ONE place, so a tier change is one edit.
pub fn default_ram_reserve_for(total_bytes: u64) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    (total_bytes / 8).clamp(GIB, 8 * GIB)
}

impl<R: HostMemoryReader> HostRamCapacitySource<R> {
    pub fn new(reader: R, reserve_bytes: u64) -> Self {
        Self {
            reader,
            reserve_bytes,
        }
    }

    /// The production constructor — reserve derived from the machine it is running on.
    pub fn with_default_reserve(reader: R) -> Self {
        let reserve = default_ram_reserve_for(reader.total_bytes());
        Self::new(reader, reserve)
    }
}

impl<R: HostMemoryReader> CapacitySource for HostRamCapacitySource<R> {
    fn kind(&self) -> ResourceKind {
        ResourceKind::Ram
    }

    fn ceiling_bytes(&self) -> u64 {
        self.reader
            .total_bytes()
            .saturating_sub(self.reserve_bytes)
    }

    fn used_bytes(&self) -> u64 {
        // Before the monitor's first poll there is NO reading. Report 0 — the trait's documented
        // degradation for a source with no physical monitor behind it, which leaves the governor
        // at `capacity − granted`. The alternative (treating unknown as total-consumed) would
        // refuse every RAM consumer for the first seconds of every boot, which is the very bug
        // this type exists to end. The window is one 2s poll.
        match self.reader.available_bytes() {
            Some(avail) => self.reader.total_bytes().saturating_sub(avail),
            None => 0,
        }
    }
}

/// Deterministic ceiling driver for rung-1/2 tests — the daemon's capacity input
/// with no hardware. Set the ceiling and the daemon reacts on its next tick,
/// exactly as a real scan would. `set_ceiling` is a lock-free atomic store, so a
/// test can shrink VRAM under live grants between ticks the way a launching game
/// would.
pub struct MockCapacitySource {
    kind: ResourceKind,
    ceiling: AtomicU64,
    /// Physical usage the source reports (`total − free` in the real world).
    /// Defaults to 0 so a test that only scripts the ceiling keeps the
    /// pre-un-inversion `available = capacity − granted` behavior; set it to
    /// emulate external pressure / resident-but-unleased bytes.
    used: AtomicU64,
}

impl MockCapacitySource {
    pub fn new(kind: ResourceKind, ceiling_bytes: u64) -> Self {
        Self {
            kind,
            ceiling: AtomicU64::new(ceiling_bytes),
            used: AtomicU64::new(0),
        }
    }

    pub fn set_ceiling(&self, bytes: u64) {
        self.ceiling.store(bytes, Ordering::SeqCst);
    }

    /// Script physical usage — the deterministic stand-in for a game grabbing
    /// VRAM (usage rises toward/over the ceiling) or a resident-but-unleased
    /// model. A lock-free atomic store so a test can shift it under live grants
    /// between ticks.
    pub fn set_used(&self, bytes: u64) {
        self.used.store(bytes, Ordering::SeqCst);
    }
}

impl CapacitySource for MockCapacitySource {
    fn kind(&self) -> ResourceKind {
        self.kind
    }

    fn ceiling_bytes(&self) -> u64 {
        self.ceiling.load(Ordering::SeqCst)
    }

    fn used_bytes(&self) -> u64 {
        self.used.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::monitor::MockMonitor;

    // what this catches: the un-inversion — the VRAM ceiling is the FIXED device
    // total minus the reserve and does NOT move when a game grabs memory; the grab
    // surfaces on the separate `used_bytes` axis (total − free) instead. If the
    // ceiling still tracked free (the old `free + ours` math), capacity would be a
    // moving remainder and a grid peer could never read a stable "what is this
    // node" number. The daemon reclaims against `used > ceiling`, not a shrinking
    // ceiling.
    #[test]
    fn gpu_ceiling_is_fixed_total_minus_reserve_while_used_tracks_grabs() {
        let mon = Arc::new(MockMonitor::new(24_000));
        mon.set_process_bytes(8_000); // our resident
        mon.set_free_bytes(15_000); // 24_000 total − 9_000 used, of which 8_000 ours
        let src = GpuCapacitySource::new(mon.clone(), 1_000);
        // ceiling is fixed: total(24_000) − reserve(1_000) = 23_000
        assert_eq!(src.ceiling_bytes(), 23_000);
        // used is everything resident: total(24_000) − free(15_000) = 9_000
        assert_eq!(src.used_bytes(), 9_000);

        // a game grabs 6_000: free drops to 9_000 → used rises to 15_000, ceiling
        // UNCHANGED. The oversubscription lives on the used axis now.
        mon.set_free_bytes(9_000);
        assert_eq!(src.ceiling_bytes(), 23_000, "fixed ceiling does not move");
        assert_eq!(
            src.used_bytes(),
            15_000,
            "the grab shows up as physical usage"
        );
    }

    // what this catches: the mock is a faithful deterministic stand-in for BOTH
    // axes — set the ceiling and the physical usage, read each back atomically,
    // with the kind it was constructed for. `used` defaults to 0 so a ceiling-only
    // test keeps the `available = capacity − granted` degrade. These are the knobs
    // every daemon scenario test turns to emulate a scan and external pressure.
    #[test]
    fn mock_source_reports_both_axes_for_its_kind() {
        let src = MockCapacitySource::new(ResourceKind::Vram, 10_000);
        assert_eq!(src.kind(), ResourceKind::Vram);
        assert_eq!(src.ceiling_bytes(), 10_000);
        assert_eq!(
            src.used_bytes(),
            0,
            "usage defaults to 0 (ceiling-only degrade)"
        );
        src.set_ceiling(4_000);
        assert_eq!(src.ceiling_bytes(), 4_000);
        src.set_used(3_500);
        assert_eq!(src.used_bytes(), 3_500);
    }

    mod host_ram {
        use super::*;

        /// A reader whose available reading can be switched off, to model the window
        /// before the memory monitor's first poll.
        struct FakeHost {
            total: u64,
            available: Option<u64>,
        }

        impl HostMemoryReader for FakeHost {
            fn total_bytes(&self) -> u64 {
                self.total
            }
            fn available_bytes(&self) -> Option<u64> {
                self.available
            }
        }

        const GIB: u64 = 1024 * 1024 * 1024;

        // what this catches: the RAM axis reporting a ceiling of 0, which is what made
        // `available_for(_, Ram)` return 0 for every consumer for as long as this source
        // did not exist (#56). A ceiling must be the machine's real RAM less the reserve.
        #[test]
        fn ceiling_is_physical_ram_less_the_reserve() {
            let src = HostRamCapacitySource::new(
                FakeHost {
                    total: 64 * GIB,
                    available: Some(40 * GIB),
                },
                4 * GIB,
            );
            assert_eq!(src.kind(), ResourceKind::Ram);
            assert_eq!(src.ceiling_bytes(), 60 * GIB);
            // used is EVERYONE's bytes, ours and the OS's alike — total − available.
            assert_eq!(src.used_bytes(), 24 * GIB);
        }

        // what this catches: THE COLD-BOOT TRAP, and it is the same defect shape as the
        // bug this type was written to end. Before the monitor's first poll there is no
        // available reading. If "unknown" were rendered as "zero free", used would equal
        // total, the remainder would be 0, and every RAM consumer would be refused for the
        // first seconds of every boot — governed-but-useless, exactly the state the RAM
        // axis was in before. Unknown must degrade to `capacity − granted`, never to zero
        // capacity. The ceiling must ALSO survive, which is why total is probed once at
        // construction rather than read from the (still-empty) monitor snapshot.
        #[test]
        fn an_unpolled_monitor_does_not_report_the_machine_as_full() {
            let src = HostRamCapacitySource::new(
                FakeHost {
                    total: 64 * GIB,
                    available: None,
                },
                4 * GIB,
            );
            assert_eq!(
                src.ceiling_bytes(),
                60 * GIB,
                "the ceiling is a hardware constant and must not wait for a poll"
            );
            assert_eq!(
                src.used_bytes(),
                0,
                "no reading means UNKNOWN, never 'all of it is spoken for'"
            );
        }

        // what this catches: a reserve that is right at one hardware tier and absurd at
        // another. A flat constant is either noise on a 128 GB box or half of an 8 GB one,
        // and this substrate is claimed to run the same code on both.
        #[test]
        fn the_reserve_stays_sane_across_the_whole_hardware_ladder() {
            // Small box: the proportional eighth would be 1 GiB — the floor holds it there,
            // and crucially it does NOT eat half the machine.
            assert_eq!(default_ram_reserve_for(8 * GIB), GIB);
            let small = HostRamCapacitySource::with_default_reserve(FakeHost {
                total: 8 * GIB,
                available: Some(4 * GIB),
            });
            assert_eq!(small.ceiling_bytes(), 7 * GIB);

            // Mid box: purely proportional.
            assert_eq!(default_ram_reserve_for(32 * GIB), 4 * GIB);

            // Big box: the ceiling clamp stops the reserve from growing without bound —
            // an eighth of 128 GB would idle 16 GB the substrate is meant to be using.
            assert_eq!(default_ram_reserve_for(128 * GIB), 8 * GIB);
            let big = HostRamCapacitySource::with_default_reserve(FakeHost {
                total: 128 * GIB,
                available: Some(100 * GIB),
            });
            assert_eq!(big.ceiling_bytes(), 120 * GIB);
        }

        // what this catches: a reserve larger than the machine underflowing into a
        // near-u64::MAX ceiling, which would report a laptop as having exabytes free.
        #[test]
        fn an_oversized_reserve_floors_at_zero_rather_than_wrapping() {
            let src = HostRamCapacitySource::new(
                FakeHost {
                    total: 2 * GIB,
                    available: Some(GIB),
                },
                8 * GIB,
            );
            assert_eq!(src.ceiling_bytes(), 0);
        }
    }
}
