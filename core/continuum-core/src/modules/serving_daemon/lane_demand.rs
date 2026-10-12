//! LANE DEMAND: how many lanes the planner should serve. One base (the boot-wired persona
//! floor), a registry of measurement overrides recomputed on every change, and the pure
//! decisions over the measured demand (settling, the ready edge, sustained leased-in peaks).
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

/// The lane-demand authority: one BASE (the boot-wired persona floor) plus a registry of
/// measurement OVERRIDES, with the effective value recomputed on every change — never a
/// swap/restore pair. The naive swap/restore version had a real interleaving bug: with two
/// overlapping leases (eval's `quiesce_all` + a solve's `quiesce_others`), the first drop
/// restored the pre-lease value OVER the still-held second lease, and the second drop then
/// restored the FIRST lease's override — leaving the whole fleet's demand stuck at 1 with
/// nobody quiesced until the (boot-only) `set_lane_demand` ever ran again. Order-independent
/// recompute makes overlap correct by construction: effective = max(overrides) while any are
/// held (never starve a concurrent measurement below what it asked for), else base.
pub(super) struct LaneDemandState {
    /// The live cell the planner reads (`ServingDaemonModule::lane_demand`).
    pub(super) cell: Arc<std::sync::atomic::AtomicU32>,
    /// The boot-wired persona floor — restored whenever the last override releases.
    pub(super) base: std::sync::Mutex<u32>,
    /// Active measurement overrides as `(lease_id, active_minds)`.
    pub(super) overrides: std::sync::Mutex<Vec<(u64, u32)>>,
    pub(super) next_id: std::sync::atomic::AtomicU64,
}

impl LaneDemandState {
    /// Recompute the effective demand from base + overrides and publish it to the cell.
    pub(super) fn recompute(&self) {
        let overrides = self
            .overrides
            .lock()
            .expect("lane-demand overrides lock poisoned"); // expect: a poisoned overrides lock means a holder panicked mid-update; the demand cell cannot be trusted, so stop loudly
        let effective = overrides
            .iter()
            .map(|(_, active)| *active)
            .max()
            .unwrap_or_else(|| *self.base.lock().expect("lane-demand base lock poisoned")); // expect / unwrap_or_else: no override held means the base IS the demand; a poisoned base lock is a panicked writer, stop loudly
        drop(overrides);
        self.cell.store(effective.max(1), Ordering::Relaxed);
    }

    /// Add an override and return its lease id.
    pub(super) fn acquire(&self, active: u32) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned") // expect: a poisoned overrides lock means a holder panicked mid-update; the demand cell cannot be trusted, so stop loudly
            .push((id, active.max(1)));
        self.recompute();
        id
    }

    /// Remove the override with `id` (unknown id = no-op) and recompute.
    pub(super) fn release(&self, id: u64) {
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned") // expect: a poisoned overrides lock means a holder panicked mid-update; the demand cell cannot be trusted, so stop loudly
            .retain(|(oid, _)| *oid != id);
        self.recompute();
    }
}

/// Registered by the first [`ServingDaemonModule::set_lane_demand`] at boot. `None` before
/// boot (unit tests / tools that never stood a daemon) → every override is a no-op, so the
/// quiesce lease stays pure and daemon-free-testable.
/// [[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]
pub(super) static LANE_DEMAND: std::sync::OnceLock<LaneDemandState> = std::sync::OnceLock::new();
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
                .expect("lane-demand overrides lock poisoned") // expect: a poisoned overrides lock means a holder panicked mid-update; the demand cell cannot be trusted, so stop loudly
                .is_empty()
        })
        .unwrap_or(false) // JUSTIFIED unwrap_or: before boot registers the state there is no lease to honour
}

/// THE LANE DEMAND IS THE LIVE ROSTER (2026-09-14). `set_lane_demand` runs once at
/// boot with the persona floor and is a no-op afterwards, so a roster that grows to 16
/// after the decode-ready edge kept planning for the floor: measured 16 resident minds
/// on 3 lanes (2 + scratch) with 40 GB of KV budget idle, a lane every ~20 min per
/// mind, and the "let me take stock" loops that come from re-orienting every turn.
/// The roster is the demand; the boot floor is only its lower bound; a measurement
/// lease still wins while it is held.
/// The tick on which a launch SETTLES: its spawn cooldown reaches zero on this tick with
/// the lane ready and serving. Only a settled geometry is remembered for the next boot.
pub fn settles_this_tick(cooling_before: u32, ready: bool, lanes: u32) -> bool {
    cooling_before == 1 && ready && lanes > 0
}

/// PURE: has a launch just come READY that the reconcile has not settled on? `settled` is
/// the ready stamp last settled on (0 = none); `now` is the live snapshot's stamp (`None` =
/// not ready). A stamp not seen before is a new launch and re-arms the settle cooldown;
/// the same stamp again, or no stamp, is not.
pub fn ready_edge(settled: u64, now: Option<u64>) -> bool {
    matches!(now, Some(t) if t != settled)
}

/// The model whose decode knee bounds the plan: the one a lane is serving, else the one
/// this box served last (card 29e4ab34 — the boot window before a lane reports its model
/// is not "no knee", it is the remembered model's knee). `remembered` is read lazily: it
/// is a file, and it is only consulted in the boot window, never on every plan tick.
pub fn knee_model(active: Option<String>, remembered: impl FnOnce() -> Option<String>) -> Option<String> {
    active.or_else(remembered)
}

pub fn resident_lane_demand(boot_floor: u32, live_residents: usize, overridden: bool) -> u32 {
    if overridden {
        return boot_floor.max(1);
    }
    boot_floor.max(live_residents as u32).max(1)
}
/// The leased-in lane demand the PLANNER may act on: the MINIMUM of the last
/// [`DOWNSHIFT_SUSTAINED_TICKS`] per-tick peaks — a burst has to be a LOAD for that many
/// consecutive ticks before it moves a slot. Fewer samples than the window = 0: a fresh
/// gauge never relaunches anything.
///
/// Why (Cormac's review of #4197, 2026-09-18): `--parallel` is a LAUNCH arg, so a lane-count
/// change is a new llama-server and every in-flight generation dies with the old one. The
/// planner debounces DOWNshifts by this same constant and adopts UPshifts immediately — so a
/// raw per-tick peak of bursty grid overflow would go: three leased calls arrive → +3 lanes →
/// relaunch NOW → the relaunch kills the three that raised the count → next tick reads 0 →
/// downshift → relaunch back → next burst → again. The 718-replan flap through a new input,
/// on the one seat everyone queues on, each swing destroying the remote turns it was sized
/// for. One constant for both directions, on purpose: the guard that stops a squeeze
/// evicting the best activity is the guard that stops a burst evicting the grid's coders.
pub fn sustained_leased_in(tick_peaks: &[usize]) -> usize {
    let window = DOWNSHIFT_SUSTAINED_TICKS as usize;
    if tick_peaks.len() < window {
        return 0;
    }
    tick_peaks[tick_peaks.len() - window..]
        .iter()
        .copied()
        .min()
        .unwrap_or(0) // unwrap_or: unreachable (window >= 1 and len >= window) — an empty slice reads as no demand, never a lane
}
/// The last [`DOWNSHIFT_SUSTAINED_TICKS`] per-tick leased-in peaks, oldest first — the
/// input to [`sustained_leased_in`]. Kept beside the tick that fills it. The most recent
/// raw peak is also kept so the plan receipt can carry both numbers.
pub(super) static LEASED_IN_PEAKS: std::sync::Mutex<std::collections::VecDeque<usize>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
pub(super) static LAST_LEASED_IN_PEAK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Fold this tick's leased-in peak into the ring and return the sustained demand.
pub(super) fn note_leased_in_peak(peak: usize) -> usize {
    LAST_LEASED_IN_PEAK.store(peak.min(u32::MAX as usize) as u32, Ordering::Relaxed);
    let mut ring = LEASED_IN_PEAKS.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned ring reads its last state, same policy as every ledger lock here
    ring.push_back(peak);
    while ring.len() > DOWNSHIFT_SUSTAINED_TICKS as usize {
        ring.pop_front();
    }
    sustained_leased_in(ring.make_contiguous())
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
