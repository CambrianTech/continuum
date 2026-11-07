//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: the untrimmed demand her turns assemble, with headroom; the population's,
//! marked, until hers is measured), granted by
//! demand-following (`capacity::window_grant`). Never the pool divided by a count (Joel,
//! 2026-11-03: "System accommodates it doesn't do stupid division"). Published on every tick
//! that moves a grant; the re-home reconciler binds each mind to hers.
use super::*;
use crate::capacity::window_grant::{Demand, PoolGrants};

impl ServingDaemonModule {
    /// The minds' grants on the running engine, published when one moves.
    /// Computed and published under the requests' reservations (`window_grant::honoring_reservations`): a
    /// mind whose request holds a reserved lease is never regranted below it until that request's
    /// terminal receipt releases it; the shrink applies at the first publish after.
    pub(super) fn publish_grants(&self) {
        let engine = self.server.served_incarnation();
        let publish = |next: Option<PoolGrants>| {
            let changed = self.grants_tx.send_if_modified(|current| {
                if *current == next {
                    return false;
                }
                current.clone_from(&next);
                true
            });
            (next, changed)
        };
        let ((next, changed), tickets) = match self.grant_inputs() {
            Some((demands, pool, cap)) => crate::capacity::window_grant::publish_fair_grants(
                &mut self.fair_grants.lock(), &demands, pool, cap, engine,
                || self.grants_tx.borrow().clone(), |next| publish(Some(next)),
            ),
            None => (crate::capacity::window_grant::honoring_reservations(engine, |_| publish(None)), Vec::new()),
        };
        for ticket in tickets {
            // The endpoint owns preparation and IO; dropping this observer cannot cancel them.
            let _receipt = crate::inference::slots::reclaim_grant(
                reqwest::Client::new(), crate::inference::llama_server::serving_root(), ticket,
            );
        }
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
    fn grant_inputs(&self) -> Option<(Vec<Demand<uuid::Uuid>>, u32, u32)> {
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
                // A bound is clamped to what one sequence holds (`Observed::within`); an exact need is not.
                let need = r.measure.observed_need()?.map(|need| need.min(trained)).within(per_sequence_cap);
                // Her target is room to grow over what her turns hold (her held need with the sent
                // headroom), never her untrimmed demand: every held token slows every act (the M5,
                // 2026-11-07: targets of 150-557k grew grants to 197k, 71 acts vs 158; 7b7a696a).
                // Growing past it is a priced decision, never "fill because the pool has room".
                let headroom = crate::inference::planning::serving_plan::SENT_HEADROOM;
                let target = crate::inference::planning::window_allocator::Tokens::new((f64::from(need.get()) * headroom) as u32)?.min(trained);
                Some(Demand {
                    mind: r.persona,
                    need,
                    target,
                    wish: r.wish?.min(trained),
                    in_flight: crate::cognition::turn_ledger::turn_in_flight(r.persona),
                    // read off her receipts by the fair publisher, which owns them (`publish_fair_grants`)
                    settled: false,
                    ceiling: per_sequence_cap,
                })
            })
            .collect();
        Some((demands, pool_tokens, per_sequence_cap))
    }

    /// Whether the running engine is on the shared pool, read off its argv once per process.
    fn on_shared_pool(&self) -> Option<bool> {
        let pid = crate::inference::lane_pidfile::read()?;
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
