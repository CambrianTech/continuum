//! The LANE DEMAND authority: how many minds need a warm serving lane now. The serving
//! daemon registers its live cell at boot and plans from it; a measurement lease (the
//! persona registry's quiesce) overrides it for its duration. An engine fact, so a mind
//! takes its override here and never reaches into the module layer.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// The lane-demand authority: one BASE (the boot-wired persona floor) plus a registry of
/// measurement OVERRIDES, with the effective value recomputed on every change — never a
/// swap/restore pair. The naive swap/restore version had a real interleaving bug: with two
/// overlapping leases (eval's `quiesce_all` + a solve's `lease_measured_work`), the first drop
/// restored the pre-lease value OVER the still-held second lease, and the second drop then
/// restored the FIRST lease's override — leaving the whole fleet's demand stuck at 1 with
/// nobody quiesced until the (boot-only) `set_lane_demand` ever ran again. Order-independent
/// recompute makes overlap correct by construction: effective = max(overrides) while any are
/// held (never starve a concurrent measurement below what it asked for), else base.
pub(crate) struct LaneDemandState {
    /// The live cell the planner reads (the serving daemon's `lane_demand`).
    pub(crate) cell: Arc<std::sync::atomic::AtomicU32>,
    /// The boot-wired persona floor — restored whenever the last override releases.
    pub(crate) base: std::sync::Mutex<u32>,
    /// Active measurement overrides as `(lease_id, active_minds)`.
    pub(crate) overrides: std::sync::Mutex<Vec<(u64, u32)>>,
    pub(crate) next_id: std::sync::atomic::AtomicU64,
}

impl LaneDemandState {
    /// Recompute the effective demand from base + overrides and publish it to the cell.
    pub(crate) fn recompute(&self) {
        let overrides = self
            .overrides
            .lock()
            .expect("lane-demand overrides lock poisoned");
        let effective = overrides
            .iter()
            .map(|(_, active)| *active)
            .max()
            .unwrap_or_else(|| *self.base.lock().expect("lane-demand base lock poisoned"));
        drop(overrides);
        self.cell.store(effective.max(1), Ordering::Relaxed);
    }

    /// Add an override and return its lease id.
    pub(crate) fn acquire(&self, active: u32) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned")
            .push((id, active.max(1)));
        self.recompute();
        id
    }

    /// Remove the override with `id` (unknown id = no-op) and recompute.
    pub(crate) fn release(&self, id: u64) {
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned")
            .retain(|(oid, _)| *oid != id);
        self.recompute();
    }
}

/// Registered by the first `ServingDaemonModule::set_lane_demand` at boot. `None` before
/// boot (unit tests / tools that never stood a daemon) → every override is a no-op, so the
/// quiesce lease stays pure and daemon-free-testable.
/// [[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]
pub(crate) static LANE_DEMAND: std::sync::OnceLock<LaneDemandState> = std::sync::OnceLock::new();
/// Whether a measurement lease (`quiesce_*`) currently holds the lane demand down.
/// While one does, the demand is the lease's ACTIVE count and the live roster must
/// not raise it back — that is the lease's whole purpose.
pub fn lane_demand_overridden() -> bool {
    LANE_DEMAND
        .get()
        .map(|state| {
            !state
                .overrides
                .lock()
                .expect("lane-demand overrides lock poisoned")
                .is_empty()
        })
        .unwrap_or(false) // JUSTIFIED unwrap_or: before boot registers the state there is no lease to honour
}

/// Register the daemon's live demand cell the first time boot sets the floor, then set the
/// floor and recompute. Idempotent registration: a later call only moves the floor.
pub fn register(cell: Arc<AtomicU32>, demand: u32) {
    let state = LANE_DEMAND.get_or_init(|| LaneDemandState {
        cell,
        base: std::sync::Mutex::new(demand.max(1)),
        overrides: std::sync::Mutex::new(Vec::new()),
        next_id: std::sync::atomic::AtomicU64::new(1),
    });
    *state.base.lock().expect("lane-demand base lock poisoned") = demand.max(1);
    state.recompute();
}

/// Add a measurement override: the fleet's warm-slot demand becomes `active` (minds that
/// need a warm slot now; floored at 1 — a measurement still needs one lane) until released.
/// Returns the lease id to pass to [`release_lane_demand`]; `None` before the daemon booted.
pub fn quiesce_lane_demand(active: u32) -> Option<u64> {
    LANE_DEMAND.get().map(|state| state.acquire(active))
}

/// Release the override acquired by [`quiesce_lane_demand`]. Idempotent (an unknown id is
/// a no-op) and order-independent — the effective demand is recomputed from what remains.
pub fn release_lane_demand(id: u64) {
    if let Some(state) = LANE_DEMAND.get() {
        state.release(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_demand_overrides_release_in_any_order() {
        let cell = Arc::new(AtomicU32::new(0));
        let state = LaneDemandState {
            cell: cell.clone(),
            base: std::sync::Mutex::new(4),
            overrides: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        };
        state.recompute();
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "base floor before any lease"
        );

        // Overlap: eval quiesce_all (active 0 → floored 1), then a solve's
        // lease_measured_work (active 1) while the first is still held.
        let eval = state.acquire(0);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        let solve = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 1);

        // The killer interleaving: release in ACQUISITION order. The solve still
        // holds its lease, so demand must STAY at the measurement value...
        state.release(eval);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            1,
            "first release must not restore the base over a still-held lease"
        );
        // ...and only the LAST release restores the base floor.
        state.release(solve);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "last release restores the base — never a stale override"
        );

        // Idempotent: releasing an unknown/stale id changes nothing.
        state.release(solve);
        assert_eq!(cell.load(Ordering::Relaxed), 4);

        // Concurrent measurements with different needs: never starve the larger one.
        let a = state.acquire(2);
        let b = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 2, "max of held overrides");
        state.release(a);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        state.release(b);
        assert_eq!(cell.load(Ordering::Relaxed), 4);
    }

    // what this catches (Cormac's review of #4197, 2026-09-18): a burst of leased-in calls
    // on ONE tick must not move the lane count. `--parallel` is a launch arg — a lane-count
    // change is a new llama-server and every in-flight generation dies with the old one —
    // so a raw per-tick peak would relaunch the seat and kill the very calls that raised
    // the count, then downshift, then repeat: the 718 flap through a new input. The
    // planner may act only on a load sustained for DOWNSHIFT_SUSTAINED_TICKS.
}
