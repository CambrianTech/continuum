//! The serving daemon's side of the pool's ONE UPWARD TEST (S2 DEBT, card e9724a7d; the engine's
//! side and the full story are `llama_server::pool_growth`). The launch reads a transient and one
//! launch keeps its pool (three rollbacks on the M5, 2026-11-03), so the first STEADY plan, whose
//! budget was sampled after the engine is up with its own footprint credited back once, re-sizes
//! the pool by the same `launch_pool` the launch used. When that proves a strictly larger pool than
//! the engine reports holding, the daemon asks for one relaunch into it; never twice per engine
//! process, never downward. Goes when the engine resizes live (S2).
use super::*;

impl ServingDaemonModule {
    /// Whether serving has reported its own footprint to the board: the plan's budget is then the
    /// ledger's replace-myself budget, sampled with this engine up and credited back exactly once.
    /// The ONE reading of "steady" (planning's `at_rest` arm reads it too).
    pub(super) fn serving_footprint_reported(&self) -> bool {
        self.resource_daemon
            .board()
            .attributions
            .iter()
            .any(|a| a.consumer_id == SERVING_CONSUMER_ID && a.kind == serving_pool_kind() && a.bytes > 0)
    }

    /// Whether the running engine is due its one upward pool relaunch now. Read on the ready tick,
    /// beside the shared-pool move.
    ///
    /// The relaunch makes its own idle edge (card 2540194e): when the first steady plan proves a
    /// larger pool, a task quiesces admission (every new inference request held, every entered one
    /// run to its end) and only then asks the engine's owner for the relaunch, holding admission
    /// until the new engine answers. #5742 instead waited for a `/slots` read with nothing
    /// processing; on the M5 on 2026-11-03 five residents on 409-token windows acted without pause,
    /// the read never came, and the 2,048-token launch pool was kept on two builds in a row.
    pub(super) fn pool_growth_due(&self, desired: &str, served_ctx: u32, lanes: u32) -> bool {
        let Some(engine) = self.server.served_incarnation() else { return false };
        // A hold taken for an engine that is no longer the one serving ends here.
        crate::inference::llama_server::end_stale_hold(engine.pid);
        if crate::inference::llama_server::growth_requested(engine.pid) {
            return true;
        }
        if self.pool_growth_tried.load(Ordering::Relaxed) == u64::from(engine.pid) {
            return false;
        }
        let Some(model) = (self.model_resolver)(desired) else { return false };
        let running = self.served_pool.load(Ordering::Relaxed);
        let (per_slot, sequences) = self.engine_pool(&model, served_ctx, lanes);
        let wanted = u64::from(per_slot) * u64::from(sequences);
        let budget = self.plan_budget_bytes.load(Ordering::Relaxed);
        if !grows(self.serving_footprint_reported(), budget, running, wanted) {
            return false;
        }
        self.pool_growth_tried.store(u64::from(engine.pid), Ordering::Relaxed);
        let pid = engine.pid;
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.pool.grow_due",
            pid = u64::from(pid),
            running_tokens = u64::from(running),
            wanted_tokens = wanted,
            budget_gb = budget / 1_000_000_000,
            "the first steady plan proves a larger pool than the engine launched with: quiescing admission for one relaunch into it (S2 debt)"
        );
        tokio::spawn(async move {
            let Some(held) = crate::inference::resource_admission::quiesce_admission().await else {
                continuum_base::probe!(
                    target: "continuum_core::modules::serving_daemon", class = "serving.pool.grow_quiesce_busy",
                    pid = u64::from(pid),
                    "admission is already quiescing: this engine's growth is not requested"
                );
                return;
            };
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.pool.grow_quiesced",
                pid = u64::from(pid),
                drained_ms = held.held_ms(),
                "admission quiesced: nothing is generating; the next ensure relaunches into the larger pool"
            );
            crate::inference::llama_server::request_pool_growth(pid, held);
        });
        false
    }
}

/// PURE: whether a steady plan proves the pool should grow. Steady (serving's footprint reported,
/// so the budget was sampled with the engine up), a sampled budget, a known running pool, and a
/// wanted pool strictly larger. Equal or smaller never relaunches.
pub(super) fn grows(steady: bool, budget_bytes: u64, running_tokens: u32, wanted_tokens: u64) -> bool {
    steady && budget_bytes > 0 && running_tokens > 0 && wanted_tokens > u64::from(running_tokens)
}

#[cfg(test)]
mod tests {
    use super::grows;

    // what this catches (the M5 2026-11-03, 81aef1844: a 16,640 pool launched from a boot-time
    // transient and kept): a pool that never grows from a bad launch, or a bridge that relaunches
    // when it should not. Only a steady plan with a sampled budget grows it, only upward.
    #[test]
    fn only_a_steady_plan_grows_the_pool_and_only_upward() {
        assert!(grows(true, 23_000_000_000, 16_640, 257_536), "the 81aef1844 case: steady, larger -> grow");
        assert!(!grows(false, 23_000_000_000, 16_640, 257_536), "not steady (the boot ticks): never");
        assert!(!grows(true, 0, 16_640, 257_536), "no sampled budget: never");
        assert!(!grows(true, 23_000_000_000, 0, 257_536), "the running pool unknown: never");
        assert!(!grows(true, 23_000_000_000, 257_536, 257_536), "equal: no relaunch");
        assert!(!grows(true, 23_000_000_000, 1_310_720, 257_536), "downward never relaunches (590df4bc3's pool stays)");
    }

}
