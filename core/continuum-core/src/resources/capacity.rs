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
        //
        // No reading yet → 0, the SAME degradation `HostRamCapacitySource`
        // already chose for host RAM and for the same reason: the governor
        // falls back to its own `capacity − granted` accounting, whereas
        // treating unknown as fully-consumed would refuse every VRAM consumer
        // for the first seconds of every boot. This is where the policy for an
        // unknown reading belongs — the monitor's job is to report that it
        // doesn't know, not to pick a number that hides it.
        match self.monitor.free_bytes() {
            Some(free) => self.monitor.total_bytes().saturating_sub(free),
            None => 0,
        }
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
    /// Last good free-bytes reading, from EITHER source. 0 = never read.
    ///
    /// # Why a fallback exists at all (regression, found live 2026-08-19)
    ///
    /// This first shipped reading ONLY `current_available_bytes()`, on the reasoning that
    /// an unset atomic means "the monitor has not polled yet" — a couple of seconds at
    /// boot, during which reporting `used = 0` is the sanctioned degradation.
    ///
    /// That reasoning was wrong in one specific and dangerous way: the atomic being 0
    /// does not only mean "not yet". It also means "that monitor is not publishing",
    /// which is unbounded. And because the unified pool routes BOTH axes through this one
    /// reader, a single silent monitor made the governor believe the whole machine was
    /// empty. Measured on this box: `system/resources` reported 54.2 GB used and 12.3 GB
    /// of swap in use, while the board reported 51.84 GB available. Before unified
    /// memory the VRAM axis read the GPU monitor independently and could not be blinded
    /// this way — collapsing the axes created the single point of failure.
    ///
    /// Erring toward "plenty free" is the worst direction to err: it over-grants into a
    /// machine that is already swapping.
    last_known_free: AtomicU64,
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
            last_known_free: AtomicU64::new(
                crate::system_resources::memory_pressure::available_from(&sys),
            ),
        }
    }

    /// A direct host sample — the SAME `host_statistics64` read `MemoryPressureMonitor`
    /// makes on its own 2s cadence, so it is not a new class of work, and it is what
    /// `system/resources` already serves from. Used only when the monitor's atomic is
    /// unset; the daemon's tick is ~1Hz, so this is at most one cheap syscall per tick
    /// on the degraded path and ZERO on the healthy one.
    fn direct_sample(&self) -> Option<u64> {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        match crate::system_resources::memory_pressure::available_from(&sys) {
            0 => None,
            n => Some(n),
        }
    }
}

impl HostMemoryReader for LiveHostMemory {
    fn total_bytes(&self) -> u64 {
        self.total
    }

    /// Free bytes, from the monitor if it is publishing, else a direct sample, else the
    /// last value either source gave. `None` ONLY if nothing has ever produced a reading
    /// — which after a successful `new()` means the host itself reported nothing.
    fn available_bytes(&self) -> Option<u64> {
        if let Some(n) = crate::system_resources::memory_pressure::current_available_bytes() {
            self.last_known_free.store(n, Ordering::Relaxed);
            return Some(n);
        }
        if let Some(n) = self.direct_sample() {
            self.last_known_free.store(n, Ordering::Relaxed);
            return Some(n);
        }
        // Stale is FAR better than fabricated-empty: a slightly old number keeps the
        // governor conservative, whereas 0-used invites it to over-grant a full machine.
        match self.last_known_free.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        }
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

/// ONE physical pool, presented as the two axes that draw on it (#56).
///
/// # The contract this implements, and who it was assigned to
///
/// [`ResourceKind::Vram`](super::lease::ResourceKind::Vram)'s own doc has always said
/// it: *"On UMA (Apple Silicon) this overlaps `Ram` physically; the authority's scan
/// layer is responsible for not double-counting."* This module IS the scan layer. The
/// contract was written, assigned here, and never implemented — so the governor kept
/// two independent ledgers over one pool and each could grant against bytes the other
/// had already spent.
///
/// Measured on an M5 (2026-08-19) before this existed: VRAM reported 16.9 GB available
/// while RAM reported 0, both describing the same ~6.4 GB of real free memory. Not a
/// rounding disagreement — two ledgers, one pool, no link.
///
/// # Why the axes stay separate rather than collapsing to one
///
/// A serving lane asking for VRAM and staging asking for RAM are making *semantically*
/// different requests even when the bytes are identical, and on a discrete box they are
/// physically different too. Collapsing the kinds would erase a distinction that is real
/// on CUDA. So the kinds survive; what changes is that on a unified host both views
/// report the SAME measured usage, so neither can grant into bytes the other's residency
/// has already consumed.
///
/// # The numbers
///
/// - **Pool ceiling** = host physical RAM − reserve. NOT `recommendedMaxWorkingSetSize`,
///   which is a Metal *hint* about GPU working sets and is smaller than physical memory.
/// - **VRAM view ceiling** = `min(pool ceiling, GPU working-set hint)`. The hint keeps its
///   meaning as a cap on GPU-side allocation without becoming a second pool.
/// - **Both views' used** = the one host measurement (`total − available`). On Apple
///   Silicon the host VM free count already includes GPU buffers — `metal_monitor`'s
///   Unified arm reads exactly this number for `free_bytes`, so using it here is agreeing
///   with the GPU monitor, not second-guessing it.
///
/// # Known residual, stated rather than hidden
///
/// Grants are still tracked per kind, so within a single tick two axes could each grant
/// against the same free bytes. The next tick closes it: a grant becomes residency, and
/// residency is the shared measurement both views report. That one-tick window is the
/// same race a single axis already has between `acquire` and the next scan — this does
/// not widen it, and `committed = max(granted, physical_used)` means the larger of the
/// two always binds. A cross-kind grant ledger would close it fully and is deferred.
pub struct UnifiedMemoryPool<R: HostMemoryReader> {
    host: R,
    /// Metal's `recommendedMaxWorkingSetSize` — a soft cap on GPU allocation, not a pool.
    gpu_working_set_hint: u64,
    reserve_bytes: u64,
}

impl<R: HostMemoryReader> UnifiedMemoryPool<R> {
    pub fn new(host: R, gpu_working_set_hint: u64, reserve_bytes: u64) -> Self {
        Self {
            host,
            gpu_working_set_hint,
            reserve_bytes,
        }
    }

    /// Production constructor — reserve derived from the machine, same policy as the
    /// discrete-host RAM source so a unified box and a discrete box hold back alike.
    pub fn with_default_reserve(host: R, gpu_working_set_hint: u64) -> Self {
        let reserve = default_ram_reserve_for(host.total_bytes());
        Self::new(host, gpu_working_set_hint, reserve)
    }

    fn pool_ceiling(&self) -> u64 {
        self.host.total_bytes().saturating_sub(self.reserve_bytes)
    }

    /// The ONE usage measurement, shared by both views. Unknown (pre-first-poll)
    /// degrades to 0 for the same reason as [`HostRamCapacitySource::used_bytes`]:
    /// "not measured yet" must never be rendered as "all of it is spoken for".
    fn pool_used(&self) -> u64 {
        match self.host.available_bytes() {
            Some(avail) => self.host.total_bytes().saturating_sub(avail),
            None => 0,
        }
    }

    /// The two capacity sources to register with the daemon. Both read this pool, so
    /// the governor sees one physical truth through two named axes.
    pub fn views(self: Arc<Self>) -> Vec<Arc<dyn CapacitySource>>
    where
        R: 'static,
    {
        vec![
            Arc::new(UnifiedPoolView {
                pool: self.clone(),
                kind: ResourceKind::Ram,
            }),
            Arc::new(UnifiedPoolView {
                pool: self,
                kind: ResourceKind::Vram,
            }),
        ]
    }
}

/// One axis's view of a [`UnifiedMemoryPool`].
struct UnifiedPoolView<R: HostMemoryReader> {
    pool: Arc<UnifiedMemoryPool<R>>,
    kind: ResourceKind,
}

impl<R: HostMemoryReader> CapacitySource for UnifiedPoolView<R> {
    fn kind(&self) -> ResourceKind {
        self.kind
    }

    fn ceiling_bytes(&self) -> u64 {
        let pool = self.pool.pool_ceiling();
        match self.kind {
            // The GPU hint caps GPU-side allocation without minting a second pool.
            ResourceKind::Vram => pool.min(self.pool.gpu_working_set_hint),
            _ => pool,
        }
    }

    fn used_bytes(&self) -> u64 {
        // THE fix: one measurement, both axes. Reporting these independently is what
        // let VRAM claim headroom RAM knew was gone.
        self.pool.pool_used()
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

        mod unified {
            use super::*;

            /// The M5 this was found on, as `resources/board` actually reported it at
            /// 2026-08-19 10:2x. Real numbers, not illustrative ones — this test IS the
            /// incident.
            const M5_TOTAL: u64 = 68_719_476_736; // 64 GiB physical
            const M5_AVAILABLE: u64 = 6_856_146_944; // 6.39 GiB genuinely free
            const M5_USED: u64 = M5_TOTAL - M5_AVAILABLE; // 61,863,329,792 — board's physicalUsed
            /// Board's observed VRAM ceiling (Metal's working-set hint, less the GPU reserve).
            const M5_GPU_HINT: u64 = 55_125_917_696;

            fn m5_pool() -> Arc<UnifiedMemoryPool<FakeHost>> {
                Arc::new(UnifiedMemoryPool::with_default_reserve(
                    FakeHost {
                        total: M5_TOTAL,
                        available: Some(M5_AVAILABLE),
                    },
                    M5_GPU_HINT,
                ))
            }

            fn view_of(
                views: &[Arc<dyn CapacitySource>],
                kind: ResourceKind,
            ) -> &Arc<dyn CapacitySource> {
                views.iter().find(|v| v.kind() == kind).expect("view exists")
            }

            // what this catches: THE DEFECT, with the numbers it was found with. Before this
            // type existed the board reported VRAM available 16,954,408,960 (16.9 GB) while
            // RAM reported 0 — two ledgers describing the same 6.39 GiB of real free memory,
            // so a serving lane could lease ~17 GB that RAM already knew was spent. Both axes
            // must now agree, because both read ONE measurement.
            #[test]
            fn vram_can_no_longer_advertise_headroom_that_ram_knows_is_gone() {
                let views = m5_pool().views();
                let ram = view_of(&views, ResourceKind::Ram);
                let vram = view_of(&views, ResourceKind::Vram);

                // The one shared measurement — this is the whole fix.
                assert_eq!(ram.used_bytes(), M5_USED);
                assert_eq!(vram.used_bytes(), M5_USED);
                assert_eq!(ram.used_bytes(), vram.used_bytes());

                // What the ledger will compute: capacity − max(granted, physical_used), and
                // with nothing granted that is ceiling − used.
                let ram_avail = ram.ceiling_bytes().saturating_sub(ram.used_bytes());
                let vram_avail = vram.ceiling_bytes().saturating_sub(vram.used_bytes());
                assert_eq!(ram_avail, 0, "RAM was already honest and must stay honest");
                assert_eq!(
                    vram_avail, 0,
                    "the 16.9 GB phantom: VRAM must not offer bytes the pool does not have"
                );
            }

            // what this catches: the pool ceiling silently becoming the GPU's working-set
            // hint. That hint is smaller than physical memory, so adopting it pool-wide would
            // shrink RAM for no reason; ignoring it entirely would let a GPU allocation blow
            // past what Metal recommends. It caps VRAM only.
            #[test]
            fn the_gpu_hint_caps_vram_without_shrinking_the_pool() {
                let views = m5_pool().views();
                // 64 GiB − 8 GiB reserve. Matches the board's reported ram capacity exactly.
                assert_eq!(
                    view_of(&views, ResourceKind::Ram).ceiling_bytes(),
                    60_129_542_144
                );
                // min(pool, hint) — and on this box the hint binds, matching the board's vram.
                assert_eq!(
                    view_of(&views, ResourceKind::Vram).ceiling_bytes(),
                    M5_GPU_HINT
                );
            }

            // what this catches: a GPU whose hint EXCEEDS the pool handing out bytes the host
            // does not have. The pool always binds; the hint can only ever lower the ceiling.
            #[test]
            fn a_hint_larger_than_the_pool_never_raises_the_vram_ceiling() {
                let pool = Arc::new(UnifiedMemoryPool::new(
                    FakeHost {
                        total: 16 * GIB,
                        available: Some(8 * GIB),
                    },
                    1024 * GIB, // absurd hint
                    2 * GIB,
                ));
                let views = pool.views();
                let pool_ceiling = 14 * GIB;
                assert_eq!(
                    view_of(&views, ResourceKind::Vram).ceiling_bytes(),
                    pool_ceiling
                );
                assert_eq!(
                    view_of(&views, ResourceKind::Ram).ceiling_bytes(),
                    pool_ceiling
                );
            }

            // what this catches: the cold-boot trap reaching the unified path. Same reasoning
            // as the discrete source — before the monitor's first poll, "unknown" must not be
            // rendered as "fully consumed" on EITHER axis, or a UMA box refuses every request
            // for the first seconds of every boot.
            #[test]
            fn an_unpolled_monitor_leaves_both_views_usable() {
                let pool = Arc::new(UnifiedMemoryPool::with_default_reserve(
                    FakeHost {
                        total: 64 * GIB,
                        available: None,
                    },
                    32 * GIB,
                ));
                for view in pool.views() {
                    assert_eq!(view.used_bytes(), 0, "unknown is not 'all spoken for'");
                    assert!(view.ceiling_bytes() > 0, "the ceiling must not wait on a poll");
                }
            }
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
