//! The per-slot window a launch takes from the plan's window: the #175 sticky floor
//! (keep the incumbent window on a same-lane relaunch), the #363 wedge-heal floor (the
//! last healthy lane's window through a teardown-transient plan), then the physical-fit
//! bound. One derivation, read by the reconcile that sizes a launch and by the tick that
//! seeds the page dir it sizes from (card 37eb1925).

use super::*;

/// Map a reconcile [`EnsureOutcome`] (+ the real per-slot window read from
/// `/props`) to the published [`ServingSnapshot`]. Pure (no IO) so the mapping
/// #175 sticky per-slot window on relaunch. The startup LoRA-load cascade relaunches
/// the living lane once per persona (each genome-set change is a relaunch), and each
/// relaunch recomputes `-c` against now-lower free memory, ratcheting the per-slot
/// window DOWN — which strands the personas pinned to the earlier, larger slot (they
/// then overflow it → the poisoned-slot "Compute error"). Keep the incumbent window
/// when we relaunch at the SAME lane count: its KV is already resident and a same-lane
/// genome reload adds only a tiny LoRA delta, so preserving it is memory-safe. A
/// LANE-count change legitimately resizes per-slot KV (2 slots need half the window
/// each) — keeping the old window across a lane INCREASE would multiply resident KV
/// and risk OOM, so only stick when `live.lanes == plan_lanes`. A LARGER plan window
/// (memory freed) is never held down here — the "starved lane" grow check above owns
/// growing. Pure so the invariant is unit-tested without a live gateway.
pub(super) fn sticky_served_window(plan_window: u32, plan_lanes: u32, live: &ServingSnapshot) -> u32 {
    if live.ready && live.lanes == plan_lanes && live.served_context_window > plan_window {
        live.served_context_window
    } else {
        plan_window
    }
}

/// The #363 wedge-heal floor — [`sticky_served_window`]'s sibling for the case sticky
/// cannot see: the live snapshot was EMPTIED by a wedge declaration (or a cold boot after
/// a crash), so `live.ready` is false and sticky passes the plan through untouched. If a
/// healthy lane with the SAME lane count was observed before, the successor spawns at
/// least that big — the plan's momentary dip is the dying predecessor's memory not yet
/// freed, and that memory is precisely what the successor inherits. Applies ONLY when
/// live is not ready (when it is, sticky already owns the decision) and only same-lanes
/// (a lane-count change legitimately resizes KV).
pub(super) fn wedge_heal_floor(
    plan_window: u32,
    plan_lanes: u32,
    live_ready: bool,
    last_healthy_window: u32,
    last_healthy_lanes: u32,
) -> u32 {
    if !live_ready && last_healthy_lanes == plan_lanes && last_healthy_window > plan_window {
        last_healthy_window
    } else {
        plan_window
    }
}

/// The window a launch takes ([`ServingDaemonModule::launch_window`]), with the steps the
/// reconcile reports: after the sticky floor, after the wedge-heal floor, and the window served.
pub(super) struct LaunchWindow {
    pub(super) sticky: u32,
    pub(super) floored: u32,
    pub(super) served: u32,
}

impl ServingDaemonModule {
    /// The per-slot window a launch of `desired` at `lanes` takes from the plan's window: the
    /// #175 sticky floor (keep the incumbent window on a same-lane relaunch), the #363 wedge-heal
    /// floor (the last healthy lane's window, through a teardown-transient plan), then the
    /// physical-fit bound (a kept window must still fit the card). ONE derivation, so the
    /// reconcile that sizes a launch and the tick that seeds the page dir it sizes from read the
    /// same geometry (card 37eb1925). Pure of side effects: the reconcile probes each step.
    pub(super) fn launch_window(&self, desired: &str, plan_window: u32, lanes: u32) -> LaunchWindow {
        // #175: a relaunch WILL happen past the no-relaunch guard (a genome page-in / model
        // change). Don't let the startup LoRA-load cascade ratchet the per-slot window DOWN on a
        // same-lane relaunch and strand the personas pinned to the earlier, larger slot: keep
        // the incumbent window when lanes are unchanged (a lane change legitimately resizes KV).
        let sticky = sticky_served_window(plan_window, lanes, &self.serving_tx.borrow());
        // #363: the sticky floor reads the live snapshot, but declaring a wedge EMPTIES that
        // snapshot first, so the relaunch the detector triggers used to spawn at whatever the
        // teardown-transient plan said (the dying predecessor still holds its memory when the
        // successor's budget samples free RAM). Floor the successor to the last HEALTHY lane's
        // window when the lane count is unchanged.
        let floored = wedge_heal_floor(
            sticky,
            lanes,
            self.serving_tx.borrow().ready,
            self.last_healthy_window.load(Ordering::Relaxed),
            self.last_healthy_lanes.load(Ordering::Relaxed),
        );
        // A KEPT WINDOW MUST STILL FIT. Both floors keep the BIGGER window on the assumption it
        // was legitimately sized. On 2026-09-17 it was not: a 144,640 window sized against a
        // fictional 54 GB budget (the ledger's uncapped add-back) was kept by sticky, then by
        // wedge-heal, across every relaunch onto a 26 GB budget (31.9 of 32.6 GB VRAM, 44 s
        // prefills). Bound it by what the PHYSICAL budget fits at this lane count (physical, not
        // live: the live budget is exactly the transient the wedge-heal law sees through). The
        // plan's own window is always within it, so this touches nothing but a kept fiction.
        let served = match self
            .catalog
            .snapshot()
            .get(desired)
            .and_then(|live| footprint_for(&live.model))
            .map(|fp| fp.window_within(self.physical_budget().usable_bytes, lanes))
        {
            Some(fits) if floored > fits.max(crate::inference::planning::MIN_SERVE_CTX) => {
                fits.max(crate::inference::planning::MIN_SERVE_CTX)
            }
            _ => floored,
        };
        LaunchWindow { sticky, floored, served }
    }

    /// Before a reconcile sizes anything, measure the saved states in the exact page dirs it will
    /// read for the plan's model: the dir a launch would size its prompt cache from, and the dir
    /// at the running lane's window (the cache-holds-no-state check). Bounded and off the worker
    /// (card 37eb1925).
    pub(super) async fn seed_reconcile_pages(&self) {
        let Some((model, plan_window, lanes)) = self.plan_tx.borrow().plan.as_ref().map(|plan| {
            (plan.base_model.model_id.clone(), plan.served_context_window, plan.lanes)
        }) else {
            return;
        };
        let launch = self.launch_window(&model, plan_window, lanes).served;
        crate::inference::slots::seed_state_samples(&crate::inference::llama_server::kv_page_dir(&model, launch)).await;
        let live = self.serving_tx.borrow().served_context_window;
        if live > 0 && live != launch {
            crate::inference::slots::seed_state_samples(&crate::inference::llama_server::kv_page_dir(&model, live)).await;
        }
    }
}
