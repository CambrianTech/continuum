//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: what her view holds at full detail plus her act's tail, with headroom),
//! granted by
//! demand-following (`capacity::window_grant`). Never the pool divided by a count (Joel,
//! 2026-11-03: "System accommodates it doesn't do stupid division"). Published on every tick
//! that moves a grant; the re-home reconciler binds each mind to hers.
use super::*;
use crate::capacity::window_grant::{Demand, DispatchWait, Excluded, GrantEvaluation, NoGrants, PoolGrants, Waiting};

/// What one evaluation grants from: the demands, the pool and one sequence's cap, each
/// requirement that entered no demand, and the least the grants hold (`live_pool::need_floor`).
pub(super) struct GrantInputs {
    demands: Vec<Demand<uuid::Uuid>>,
    pool: u32,
    cap: u32,
    excluded: std::collections::BTreeMap<uuid::Uuid, Excluded>,
    pub(super) floor: u32,
}

impl ServingDaemonModule {
    /// The minds' grants on the running engine, published when one moves.
    /// Computed and published under the requests' reservations (`window_grant::honoring_reservations`): a
    /// mind whose request holds a reserved lease is never regranted below it until that request's
    /// terminal receipt releases it; the shrink applies at the first publish after.
    /// Every run also publishes its evaluation (`evaluations_tx`), changed or not, so a mind waiting
    /// on a grant learns at this cadence why she has none (card 5e79e1c1).
    pub(super) fn publish_grants(&self) {
        let started = std::time::Instant::now();
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
        let (((next, changed), tickets), nobody, excluded, waiting) = match self.grant_inputs() {
            Ok(GrantInputs { demands, pool, cap, excluded, .. }) => {
                // One fair-state lock across the publish and the read of its queue, so the queue
                // positions are the ones this publish left.
                let mut fair = self.fair_grants.lock();
                let published = crate::capacity::window_grant::publish_at_the_act(
                    &mut fair, &demands, pool, cap, engine,
                    || self.grants_tx.borrow().clone(), |next| publish(Some(next)),
                );
                let granted = published.0.0.as_ref();
                let waiting = demands
                    .iter()
                    .filter(|d| granted.and_then(|g| g.grant_for(d.mind)).is_none())
                    .map(|d| (d.mind, waiting_reason(d, pool, fair.waiting(d.mind))))
                    .collect();
                (published, None, excluded, waiting)
            }
            Err(nobody) => (
                (crate::capacity::window_grant::honoring_reservations(engine, |_| publish(None)), Vec::new()),
                Some(nobody),
                Default::default(),
                Default::default(),
            ),
        };
        // One value carrying the grants it published with and the reasons, so a mind never reads
        // her grant from one evaluation and its reason from another (Codex on the C2 design).
        self.evaluations_tx.send_modify(|evaluation| {
            let seq = evaluation.as_ref().map_or(1, |previous| previous.seq + 1);
            *evaluation = Some(GrantEvaluation { seq, started, cadence: TICK, engine, grants: next.clone(), nobody, excluded, waiting });
        });
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

    /// Subscribe to the publisher's evaluations.
    pub fn subscribe_evaluations(&self) -> watch::Receiver<Option<GrantEvaluation>> {
        self.evaluations_tx.subscribe()
    }

    /// Refused, typed, until the engine serves and has reported its pool: there is nothing to grant from.
    pub(super) fn grant_inputs(&self) -> Result<GrantInputs, NoGrants> {
        let live = self.serving_tx.borrow().clone();
        // The pool the engine itself reports (`context_geometry.allocated_tokens`), never a
        // published share multiplied back by a count (Cormac on #5733); the room moves it below.
        let pool_tokens = self.served_pool.load(Ordering::Relaxed);
        if !live.ready {
            return Err(NoGrants::NotReady);
        }
        if live.served_context_window == 0 {
            return Err(NoGrants::ServedWindowZero);
        }
        if pool_tokens == 0 {
            return Err(NoGrants::PoolZero);
        }
        let trained = live
            .active_model
            .as_deref()
            .and_then(|id| (self.model_resolver)(id))
            .and_then(|model| crate::inference::planning::window_allocator::Tokens::new(model.context_window))
            .ok_or(NoGrants::ModelUnresolved)?;
        // The most one sequence may hold, as the engine reports it (one slot under fixed slots, the
        // pool under a shared one), never a division read off how it was launched (CONTINUUM-ENGINE.md,
        // serving engine: admission (4.8); BigMama 2026-11-11: Sahar parked at 58,112, half the pool, every act refused).
        let per_sequence_cap = match self.served_sequence.load(Ordering::Relaxed) {
            0 => return Err(NoGrants::PoolZero),
            reported => reported.min(trained.get()),
        };
        // Only a measured mind has a demand: an unmeasured one has no number to grant (her first view
        // measures her), never the population's or anyone else's. The model's trained window is the one
        // hard limit on both her floor and her target (her context compresses to it).
        let mut excluded = std::collections::BTreeMap::new();
        let mut demands = Vec::new();
        for r in &self.serving_demand().requirements {
            match Demand::of(r, trained, per_sequence_cap, crate::cognition::turn_ledger::turn_in_flight(r.persona)) {
                Ok(demand) => demands.push(demand),
                Err(why) => {
                    excluded.insert(r.persona, why);
                }
            }
        }
        // The pool the grants share is the engine's as the host's room has moved it: the value the
        // pool step publishes (`live_pool`), never below what the minds need, never above what the
        // engine holds. This publisher is its one reader; the step never publishes grants itself.
        let floor = super::live_pool::need_floor(&demands, self.grants_tx.borrow().as_ref());
        let engine = self.server.served_incarnation();
        let pool = self.live_pool.borrow().pool(pool_tokens, floor, engine);
        Ok(GrantInputs { demands, pool, cap: per_sequence_cap, excluded, floor })
    }
}

/// Why a demand the publisher received holds no grant after it. A need no sequence or pool can hold
/// is named before what she waits on.
fn waiting_reason(d: &Demand<uuid::Uuid>, pool: u32, waits: Option<DispatchWait>) -> Waiting {
    let need = d.need.get();
    match waits {

        _ if crate::capacity::window_grant::is_grant_retiring(d.mind) => Waiting::Reclaiming,
        Some(DispatchWait::Reclaiming) => Waiting::ForReclaim,
        Some(DispatchWait::BehindActs { acting }) => Waiting::BehindActs { acting },
        None => Waiting::Unplaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::{observed::Observed, LaneRequirement, Tokens};

    fn t(n: u32) -> Tokens {
        Tokens::new(n).expect("test: a nonzero size") // expect: every size here is a positive literal
    }

}
