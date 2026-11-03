//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: the untrimmed demand her turns assemble, with headroom), granted by
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
                "each local mind's window on the engine's pool: her measured requirement, granted by demand-following"
            );
        }
    }

    /// Subscribe to the minds' grants.
    pub fn subscribe_grants(&self) -> watch::Receiver<Option<PoolGrants>> {
        self.grants_tx.subscribe()
    }

    /// `None` until the engine serves: there is no pool to grant from.
    fn grants_now(&self) -> Option<PoolGrants> {
        let live = self.serving_tx.borrow().clone();
        if !live.ready || live.served_context_window == 0 || live.lanes == 0 {
            return None;
        }
        // The engine's pool as it published it: its window times its sequences, on fixed slots
        // and on the shared pool alike.
        let pool_tokens = live.served_context_window.saturating_mul(live.lanes);
        let trained = live
            .active_model
            .as_deref()
            .and_then(|id| (self.model_resolver)(id))
            .map(|model| model.context_window)?;
        let per_sequence_cap = match self.on_shared_pool()? {
            true => pool_tokens.min(trained),
            false => live.served_context_window,
        };
        let requirements = self.serving_demand().requirements;
        let largest = crate::inference::planning::window_allocator::largest_known(&requirements);
        let demands: Vec<Demand<uuid::Uuid>> = requirements
            .iter()
            .map(|r| Demand {
                mind: r.persona,
                // A mind not yet measured asks for the largest requirement on the seat, else the
                // most one sequence may hold: what she may grow into, never a picked number.
                required_tokens: r.window.or(largest).unwrap_or(per_sequence_cap).min(per_sequence_cap), // unwrap_or: nothing measured on the seat = the most one sequence may hold
                in_flight: crate::cognition::turn_ledger::turn_in_flight(r.persona),
                ceiling: per_sequence_cap,
            })
            .collect();
        let held = self.grants_tx.borrow().as_ref().map(|g| g.by_mind.clone()).unwrap_or_default(); // unwrap_or_default: nothing granted yet holds nothing
        let by_mind = DemandFollowing.grant(&demands, &held, pool_tokens);
        Some(PoolGrants { pool_tokens, per_sequence_cap, by_mind })
    }

    /// Whether the running engine is on the shared pool, read off its argv once per process.
    fn on_shared_pool(&self) -> Option<bool> {
        let pid = crate::inference::lane_pidfile::read()?;
        if let Some((known, shared)) = *self.shared_pool_of.lock() {
            if known == pid {
                return Some(shared);
            }
        }
        let shared = self.server.launched_on_shared_pool()?;
        *self.shared_pool_of.lock() = Some((pid, shared));
        Some(shared)
    }
}
