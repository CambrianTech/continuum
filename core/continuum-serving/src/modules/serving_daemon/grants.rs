//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: the untrimmed demand her turns assemble, with headroom; the population's,
//! marked, until hers is measured), granted by
//! demand-following (`capacity::window_grant`). Never the pool divided by a count (Joel,
//! 2026-11-03: "System accommodates it doesn't do stupid division"). Published on every tick
//! that moves a grant; the re-home reconciler binds each mind to hers.
use super::*;
use crate::capacity::window_grant::{Demand, DemandFollowing, PoolGrants, WindowPolicy};

impl ServingDaemonModule {
    /// The minds' grants on the running engine, published when one moves.
    pub(super) fn publish_grants(&self) {
        let next = self.grants_now();
        let changed = self.grants_tx.send_if_modified(|current| {
            if *current == next {
                return false;
            }
            current.clone_from(&next);
            true
        });
        if let (true, Some(grants)) = (changed, next.as_ref()) {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.grants",
                pool_tokens = grants.pool_tokens,
                per_sequence_cap = grants.per_sequence_cap,
                minds = grants.by_mind.len() as u64,
                granted = grants.by_mind.values().map(|g| u64::from(*g)).sum::<u64>(),
                grants = ?grants.by_mind,
                "each measured local mind's window on the engine's pool: her need, her target when every target fits"
            );
        }
    }

    /// Subscribe to the minds' grants.
    pub fn subscribe_grants(&self) -> watch::Receiver<Option<PoolGrants>> {
        self.grants_tx.subscribe()
    }

    /// `None` until the engine serves and has reported its pool: there is nothing to grant from.
    fn grants_now(&self) -> Option<PoolGrants> {
        let live = self.serving_tx.borrow().clone();
        // The pool the engine itself reports (`context_geometry.allocated_tokens`), never a
        // published share multiplied back by a count (Cormac on #5733).
        let pool_tokens = self.served_pool.load(Ordering::Relaxed);
        if !live.ready || live.served_context_window == 0 || pool_tokens == 0 {
            return None;
        }
        let trained = live
            .active_model
            .as_deref()
            .and_then(|id| (self.model_resolver)(id))
            .and_then(|model| crate::inference::planning::window_allocator::Tokens::new(model.context_window))?;
        let per_sequence_cap = match self.on_shared_pool()? {
            true => pool_tokens.min(trained.get()),
            false => live.served_context_window,
        };
        // Only a measured mind has a demand: an unmeasured one has no number to grant (her first view
        // measures her), never the population's or anyone else's. The model's trained window is the one
        // hard limit on both her floor and her target (her context compresses to it).
        let demands: Vec<Demand<uuid::Uuid>> = self
            .serving_demand()
            .requirements
            .iter()
            .filter_map(|r| {
                Some(Demand {
                    mind: r.persona,
                    // A bound is clamped to what one sequence holds (`Observed::within`); an exact need is not.
                    need: r.measure.observed_need()?.map(|need| need.min(trained)).within(per_sequence_cap),
                    target: r.measure.target()?.min(trained),
                    wish: r.wish?.min(trained),
                    in_flight: crate::cognition::turn_ledger::turn_in_flight(r.persona),
                    ceiling: per_sequence_cap,
                })
            })
            .collect();
        let held = self.grants_tx.borrow().as_ref().map(|g| g.by_mind.clone()).unwrap_or_default(); // unwrap_or_default: nothing granted yet holds nothing
        let by_mind = DemandFollowing.grant(&demands, &held, pool_tokens);
        Some(PoolGrants { pool_tokens, per_sequence_cap, by_mind })
    }

    /// Whether the running engine is on the shared pool, read off its argv once per process.
    fn on_shared_pool(&self) -> Option<bool> {
        let pid = (self.engine_pid)()?;
        if let Some((known, shared)) = *self.shared_pool_of.lock() {
            if known == pid {
                return Some(shared);
            }
        }
        let shared = self.server.pool_launch()?.kv_shared();
        *self.shared_pool_of.lock() = Some((pid, shared));
        Some(shared)
    }
}
