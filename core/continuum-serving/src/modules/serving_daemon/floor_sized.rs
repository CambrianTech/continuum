//! A running engine whose pool holds fewer of the seated minds' needs than a launch now would: the
//! one pool-size change that retires an engine (card 8569b03d).
//!
//! ONE LAUNCH keeps a ready engine's pool for life, and the fork has no live pool resize, so a pool
//! launched from a transient reading stays at it. The M5, 2026-11-13: at 19:36Z an engine launched
//! at a 2,048-token pool while the plan, 15 s later, read 2 x 134,690; at 22:16Z one launched at
//! 49,408 for 5 sequences and did not grow in 10 minutes. Every mind needed 68-110k, more than the
//! whole pool, so every mind was dark until the engine was killed by hand.
//!
//! The rule counts minds, never tokens of slack: the engine is retired once when a launch at the
//! plan's live budget would hold the needs of MORE seated minds than the running pool holds. After
//! that launch the running pool holds them, so the rule cannot fire again until another mind's
//! need outgrows the pool and the host has room for it: at most once per seated mind, and once per
//! engine process. An engine that hosts live work is never reconciled (the occupancy guard runs
//! first), and the relaunch goes through the settle gate like any launch.

use super::*;

/// Why the running engine is retired: what it holds, and what a launch now would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FloorSized {
    pub(super) running_pool: u32,
    pub(super) launch_pool: u64,
    /// Seated minds whose need the running pool holds.
    pub(super) held_now: usize,
    /// Seated minds whose need the launch would hold.
    pub(super) held_then: usize,
}

/// Pure: the running pool against a launch now, over the seated minds' needs (tokens each).
pub(super) fn floor_sized(running_pool: u32, needs: &[u64], launch_pool: u64) -> Option<FloorSized> {
    let holds = |pool: u64| needs.iter().filter(|&&need| need <= pool).count();
    let (held_now, held_then) = (holds(u64::from(running_pool)), holds(launch_pool));
    (held_then > held_now).then_some(FloorSized { running_pool, launch_pool, held_now, held_then })
}

impl ServingDaemonModule {
    /// The ready engine serving `desired` is floor-sized against the plan's launch now (`None`
    /// otherwise, or when this engine process was already judged once).
    pub(super) fn floor_sized_engine(&self, desired: &str, served_ctx: u32, lanes: u32) -> Option<FloorSized> {
        {
            let live = self.serving_tx.borrow();
            if !live.ready || live.active_model.as_deref() != Some(desired) {
                return None;
            }
        }
        let running_pool = self.served_pool.load(Ordering::Relaxed);
        if running_pool == 0 {
            return None;
        }
        let pid = u64::from(self.server.served_incarnation()?.pid);
        if self.floor_sized_tried.load(Ordering::Relaxed) == pid {
            return None;
        }
        let model = (self.model_resolver)(desired)?;
        let needs = self.seated_needs(&model);
        if needs.iter().all(|&need| need <= u64::from(running_pool)) {
            return None;
        }
        let (per_sequence, sequences) = self.engine_pool(&model, served_ctx, lanes);
        let verdict = floor_sized(running_pool, &needs, u64::from(per_sequence) * u64::from(sequences))?;
        self.floor_sized_tried.store(pid, Ordering::Relaxed);
        Some(verdict)
    }

    /// Retire a floor-sized engine; the next reconcile launches at the plan's live budget.
    pub(super) fn retire_floor_sized_engine(&self, operation: ServingOperation, revision: u64, floor: FloorSized) -> JoinHandle<()> {
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.engine.floor_sized",
            running_pool = floor.running_pool as u64,
            launch_pool = floor.launch_pool,
            held_now = floor.held_now as u64,
            held_then = floor.held_then as u64,
            "the running pool holds fewer seated minds' needs than a launch at the live budget would: retiring it once, the next reconcile launches the grown pool"
        );
        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let verified_target = self.verified_target.clone();
        let intent_owner = self.intent.clone();
        let bus = self.bus.get().cloned();
        let served_pool = Arc::clone(&self.served_pool);
        tokio::spawn(async move {
            let operation = operation;
            let admit = || {
                let intent = intent_owner.state.borrow();
                if intent.revision != revision {
                    return false;
                }
                drop(intent);
                Self::publish_lifecycle_admission(&serving_tx, &verified_target, bus.as_ref(), None);
                true
            };
            operation.step(ReconcileStep::Retire);
            match server.idle_if_current(&admit).await {
                Ok(()) => {
                    served_pool.store(0, Ordering::Relaxed);
                    verified_target.send_replace(None);
                    let snapshot = ServingSnapshot::degraded(format!(
                        "the engine's {}-token pool held {} of the seated minds' needs; relaunching at {} to hold {}",
                        floor.running_pool, floor.held_now, floor.launch_pool, floor.held_then
                    ));
                    Self::emit_serving(bus.as_ref(), &snapshot);
                    serving_tx.send_replace(snapshot);
                }
                Err(crate::inference::llama_server::LlamaServerError::Superseded) => {}
                Err(e) => {
                    tracing::warn!(target: "continuum_core::modules::serving_daemon", error = %e, "floor-sized engine retirement failed")
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the M5, 2026-11-13, card 8569b03d): a pool launched from a transient
    // reading kept for life. A 2,048 pool beside needs of 68-110k holds none of them; the plan's
    // launch at the settled budget holds all four, so the engine is retired once. After the grown
    // launch the running pool holds them and the rule is quiet: a budget that wobbles never
    // relaunches, and a pool that already holds every need is never retired for slack.
    #[test]
    fn boot_at_the_floor_then_the_grown_plan_is_applied_once() {
        let needs = [68_000, 82_000, 95_000, 110_000];
        let grown = 2 * 134_690;
        let verdict = floor_sized(2_048, &needs, grown).expect("a 2,048 pool beside 68-110k needs is floor-sized");
        assert_eq!((verdict.held_now, verdict.held_then), (0, 4));
        assert_eq!(floor_sized(49_408, &needs, grown).map(|v| v.held_now), Some(0), "the 22:16Z engine: 49,408 for five");
        assert_eq!(floor_sized(grown as u32, &needs, grown + 50_000), None, "the grown pool holds every need: no relaunch for slack");
        assert_eq!(floor_sized(grown as u32, &needs, grown - 50_000), None, "a smaller launch never retires a working engine");
        assert_eq!(floor_sized(100_000, &needs, 104_000), None, "no more minds held: a wobble never relaunches");
        assert_eq!(floor_sized(100_000, &needs, 110_000).map(|v| v.held_then), Some(4), "one more mind held is worth the one relaunch");
    }
}
