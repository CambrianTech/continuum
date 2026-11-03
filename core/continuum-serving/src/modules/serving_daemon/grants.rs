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
                from_population = ?grants.from_population,
                "each local mind's window on the engine's pool: her measured requirement, granted by demand-following"
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
            .map(|model| model.context_window)?;
        let per_sequence_cap = match self.on_shared_pool()? {
            true => pool_tokens.min(trained),
            false => live.served_context_window,
        };
        let requirements = self.serving_demand().requirements;
        // A mind not measured yet claims the population's measured requirement on this seat,
        // marked as such (ONE-BUDGET: an unmeasured claim never asks for the most). With no one
        // measured she is granted nothing yet and keeps her binding until her first turn is.
        let population = population_requirement(requirements.iter().map(|r| r.window));
        let mut from_population = std::collections::BTreeSet::new();
        let demands: Vec<Demand<uuid::Uuid>> = requirements
            .iter()
            .filter_map(|r| {
                let required = match r.window {
                    Some(own) => own,
                    None => {
                        from_population.insert(r.persona);
                        population?
                    }
                };
                Some(Demand {
                    mind: r.persona,
                    // The model's trained window is the one hard limit (her context compresses to it);
                    // a need above what the pool holds now is not trimmed here, it waits.
                    required_tokens: required.min(trained),
                    in_flight: crate::cognition::turn_ledger::turn_in_flight(r.persona),
                    ceiling: per_sequence_cap,
                })
            })
            .collect();
        from_population.retain(|mind| demands.iter().any(|d| d.mind == *mind));
        let held = self.grants_tx.borrow().as_ref().map(|g| g.by_mind.clone()).unwrap_or_default(); // unwrap_or_default: nothing granted yet holds nothing
        let by_mind = DemandFollowing.grant(&demands, &held, pool_tokens);
        Some(PoolGrants { pool_tokens, per_sequence_cap, by_mind, from_population })
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

/// The population's measured requirement on this seat: the median of the minds measured so far.
/// `None` when no one is.
fn population_requirement(windows: impl Iterator<Item = Option<u32>>) -> Option<u32> {
    let mut measured: Vec<u32> = windows.flatten().collect();
    measured.sort_unstable();
    measured.get(measured.len() / 2).copied()
}

#[cfg(test)]
mod tests {
    use super::population_requirement;

    // what this catches: the least-known mind claiming the biggest share (Cormac on #5733: an
    // unmeasured mind asked for the largest requirement on the seat, or the whole cap). She claims
    // the population's median, and nothing when no one is measured.
    #[test]
    fn an_unmeasured_mind_claims_the_median_never_the_most() {
        assert_eq!(population_requirement([Some(70_000), None, Some(10_000), Some(20_000)].into_iter()), Some(20_000));
        assert_eq!(population_requirement([None, None].into_iter()), None);
    }
}
