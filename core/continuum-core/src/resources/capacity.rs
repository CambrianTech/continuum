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
}
