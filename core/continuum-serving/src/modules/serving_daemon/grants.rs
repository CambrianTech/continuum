//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: the untrimmed demand her turns assemble, with headroom; the population's,
//! marked, until hers is measured), granted by
//! demand-following (`capacity::window_grant`). Never the pool divided by a count (Joel,
//! 2026-11-03: "System accommodates it doesn't do stupid division"). Published on every tick
//! that moves a grant; the re-home reconciler binds each mind to hers.
use super::*;
use crate::capacity::window_grant::{Demand, DemandFollowing, PoolGrants, TurnsEnded, WindowPolicy};
use crate::inference::planning::window_allocator::{LaneRequirement, Tokens};

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
        let held = self.grants_tx.borrow().as_ref().map(|g| g.by_mind.clone()).unwrap_or_default(); // unwrap_or_default: nothing granted yet holds nothing
        let requirements = self.serving_demand().requirements;
        // Her turn state from the turn ledger: in a turn now, and how many she has ended (counted as
        // each ends, so a turn that ends and restarts between two ticks is still seen to end).
        let turns = |persona| {
            (crate::cognition::turn_ledger::turn_in_flight(persona), crate::cognition::turn_ledger::turns_ended(persona))
        };
        // One policy across ticks: it keeps each mind's rotation tenure (`DemandFollowing`).
        Some(grant_tick(&mut self.grant_policy.lock(), &requirements, turns, trained, per_sequence_cap, pool_tokens, &held))
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

/// One grant tick at the daemon's seam: each measured mind's requirement and live turn state
/// (`live`: in a turn, and her ended-turn count) into her demand, granted by `policy`, published
/// with the lapses its service guarantee sets. Only a measured mind has a demand: an unmeasured one
/// has no number to grant (her first view measures her), never the population's or anyone else's.
/// The model's trained window is the one hard limit on her need, floor and target (her context
/// compresses to it).
pub(super) fn grant_tick(
    policy: &mut DemandFollowing<uuid::Uuid>,
    requirements: &[LaneRequirement],
    live: impl Fn(uuid::Uuid) -> (bool, TurnsEnded),
    trained: Tokens,
    per_sequence_cap: u32,
    pool_tokens: u32,
    held: &std::collections::BTreeMap<uuid::Uuid, u32>,
) -> PoolGrants {
    let demands: Vec<Demand<uuid::Uuid>> = requirements
        .iter()
        .filter_map(|r| {
            let (in_flight, turns_ended) = live(r.persona);
            Some(Demand {
                mind: r.persona,
                // Typed exact or a bound: the grant reads it against what one sequence holds and
                // what the pool has left (`Observed::within`); an exact need is never trimmed.
                need: r.measure.observed_need()?.map(|need| need.min(trained)),
                floor: r.floor.map(|floor| floor.min(trained)),
                target: r.measure.target()?.min(trained),
                wish: r.wish?.min(trained),
                in_flight,
                turns_ended,
                ceiling: per_sequence_cap,
            })
        })
        .collect();
    let by_mind = policy.grant(&demands, held, pool_tokens);
    PoolGrants { pool_tokens, per_sequence_cap, by_mind, lapses: policy.lapses().clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::window_grant::schedule::{play, Busy, Restart};
    use crate::inference::planning::window_allocator::test_tokens as t;

    /// Busy minds with exact needs and unmeasured floors, played through the daemon's grant tick on
    /// a shared pool of `pool`: what the schedule served.
    fn serve(needs: &[(u128, u32, u32)], pool: u32, restart: Restart) -> crate::capacity::window_grant::schedule::Served {
        let requirements: Vec<LaneRequirement> =
            needs.iter().map(|&(id, need, _)| LaneRequirement::declared(uuid::Uuid::from_u128(id), t(need))).collect();
        let minds: Vec<Busy> = needs.iter().map(|&(id, _, turn_ticks)| Busy { mind: uuid::Uuid::from_u128(id), turn_ticks }).collect();
        let mut policy = DemandFollowing::default();
        let mut held = std::collections::BTreeMap::new();
        play(&minds, restart, 300, |seen| {
            let live = |persona| {
                seen.iter().find(|s| s.mind == persona).map_or((false, TurnsEnded::default()), |s| (s.in_flight, s.turns_ended)) // map_or: every requirement is a played mind
            };
            let grants = grant_tick(&mut policy, &requirements, live, t(262_144), pool, pool, &held);
            held.clone_from(&grants.by_mind);
            grants
        })
    }

    // what this catches (Codex CHANGES at a95afe3d1, counterexample 1, and the M5 live grant at
    // 21:28Z on a95afe3d1: a 35,584 pool seated Aiko 16,150 and Atlas 14,394, and Aris and Benchy
    // nothing): unmeasured floors outside the service guarantee. Two 60,000 exact needs on a 100,000
    // pool: the seats fit, so rotation never ran and B waited forever. Through the daemon's grant
    // tick, both are served, each waiting at most (M − S)·S = 1 turn ending; and the 21:28Z shape
    // (four unmeasured minds, two fit at once) serves all four within (4 − 2)·2 = 4.
    #[test]
    fn unmeasured_floors_are_served_within_the_bound_at_the_daemon_seam() {
        for restart in [Restart::AfterATick, Restart::BetweenTicks] {
            let two = serve(&[(1, 60_000, 2), (2, 60_000, 3)], 100_000, restart);
            assert!(two.longest_wait <= 1, "{restart:?}: waited {} turn endings", two.longest_wait);
            assert!(two.turns.values().all(|&n| n >= 20) && two.turns.len() == 2, "{restart:?}: both served: {:?}", two.turns);
            assert_eq!((two.over_pool, two.shrunk), (0, 0), "{restart:?}: within the pool, no running turn shrunk");
            // Needs are this test's inputs: the two granted at 21:28Z, and two that did not fit beside them.
            let m5 = serve(&[(1, 16_150, 2), (2, 14_394, 3), (3, 17_500, 2), (4, 15_800, 4)], 35_584, restart);
            assert!(m5.longest_wait <= 4, "{restart:?}: waited {} turn endings", m5.longest_wait);
            assert!(m5.turns.values().all(|&n| n >= 10) && m5.turns.len() == 4, "{restart:?}: all four served: {:?}", m5.turns);
            assert_eq!((m5.over_pool, m5.shrunk), (0, 0), "{restart:?}: within the pool, no running turn shrunk");
        }
    }
}
