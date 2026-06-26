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

/// One resource axis's live ceiling, sourced from a hardware monitor. The
/// ceiling is *the bytes the lease pool may grow to* — physical total minus the
/// bytes held by genuinely-external (non-lease) consumers minus a safety
/// reserve. When something outside a lease grabs memory, the ceiling drops; if
/// it drops below what the authority has already granted, the daemon's
/// reconcile claws the difference back. This is the precise number that fixes
/// the `host_budget()` OOM bug: serving no longer claims a fraction of *total*
/// VRAM blind to Bevy/LiveKit — it leases against this net-of-everyone ceiling.
pub trait CapacitySource: Send + Sync {
    fn kind(&self) -> ResourceKind;

    /// Current ceiling in bytes. Cached, non-blocking — see the module contract.
    fn ceiling_bytes(&self) -> u64;
}

/// VRAM ceiling from a [`GpuMonitor`]. The pool may hold what is currently free
/// PLUS what our own process already holds resident (which the lease pool could
/// itself move around) — i.e. `total − external`, where external = everything
/// resident that isn't ours. A safety reserve is held off the top so the
/// authority never leases the last sliver the driver/OS needs.
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
        // free + ours = total − external. `free_bytes`/`process_bytes` are cached
        // accessors on the monitor (it scans on its own task) — non-blocking.
        let pool = self
            .monitor
            .free_bytes()
            .saturating_add(self.monitor.process_bytes());
        pool.saturating_sub(self.reserve_bytes)
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
}

impl MockCapacitySource {
    pub fn new(kind: ResourceKind, ceiling_bytes: u64) -> Self {
        Self {
            kind,
            ceiling: AtomicU64::new(ceiling_bytes),
        }
    }

    pub fn set_ceiling(&self, bytes: u64) {
        self.ceiling.store(bytes, Ordering::SeqCst);
    }
}

impl CapacitySource for MockCapacitySource {
    fn kind(&self) -> ResourceKind {
        self.kind
    }

    fn ceiling_bytes(&self) -> u64 {
        self.ceiling.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::monitor::MockMonitor;

    // what this catches: the VRAM ceiling nets out everything non-lease and the
    // reserve. If a game grabs VRAM (free drops) while our resident stays, the
    // ceiling must drop by exactly the game's grab — that drop below `granted` is
    // the signal the daemon's reconcile reclaims against. If this math inverts,
    // the authority would either over-grant into a game's memory (the OOM bug) or
    // needlessly evict when there is headroom.
    #[test]
    fn gpu_ceiling_is_free_plus_ours_minus_reserve_and_tracks_external_grabs() {
        let mon = Arc::new(MockMonitor::new(24_000));
        mon.set_process_bytes(8_000); // our leases resident
        mon.set_free_bytes(15_000); // 1_000 held by OS/driver, none external yet
        let src = GpuCapacitySource::new(mon.clone(), 1_000);
        // free(15_000) + ours(8_000) − reserve(1_000) = 22_000
        assert_eq!(src.ceiling_bytes(), 22_000);

        // a game grabs 6_000: free drops, our resident unchanged → ceiling drops 6_000
        mon.set_free_bytes(9_000);
        assert_eq!(src.ceiling_bytes(), 16_000);
    }

    // what this catches: the mock is a faithful deterministic stand-in — set the
    // ceiling, read it back atomically, with the kind it was constructed for.
    // This is the knob every daemon scenario test turns to emulate a scan.
    #[test]
    fn mock_source_reports_what_was_set_for_its_kind() {
        let src = MockCapacitySource::new(ResourceKind::Vram, 10_000);
        assert_eq!(src.kind(), ResourceKind::Vram);
        assert_eq!(src.ceiling_bytes(), 10_000);
        src.set_ceiling(4_000);
        assert_eq!(src.ceiling_bytes(), 4_000);
    }
}
