//! Each local mind's window on the engine's pool is her grant: her measured requirement
//! (`requirements_for`: the untrimmed demand her turns assemble, with headroom; the population's,
//! marked, until hers is measured), granted by
//! demand-following (`capacity::window_grant`). Never the pool divided by a count (Joel,
//! 2026-11-03: "System accommodates it doesn't do stupid division"). Published on every tick
//! that moves a grant; the re-home reconciler binds each mind to hers.
use super::*;
use crate::capacity::window_grant::{Demand, Excluded, GrantEvaluation, NoGrants, PoolGrants, Waiting};

/// What one evaluation grants from: the demands, the pool and one sequence's cap, and each
/// requirement that entered no demand.
struct GrantInputs {
    demands: Vec<Demand<uuid::Uuid>>,
    pool: u32,
    cap: u32,
    excluded: std::collections::BTreeMap<uuid::Uuid, Excluded>,
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
            Ok(GrantInputs { demands, pool, cap, excluded }) => {
                // One fair-state lock across the publish and the read of its queue, so the queue
                // positions are the ones this publish left.
                let mut fair = self.fair_grants.lock();
                let published = crate::capacity::window_grant::publish_fair_grants(
                    &mut fair, &demands, pool, cap, engine,
                    || self.grants_tx.borrow().clone(), |next| publish(Some(next)),
                );
                let granted = published.0.0.as_ref();
                let waiting = demands
                    .iter()
                    .filter(|d| granted.and_then(|g| g.grant_for(d.mind)).is_none())
                    .map(|d| (d.mind, waiting_reason(d, pool, fair.queued(d.mind))))
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
    fn grant_inputs(&self) -> Result<GrantInputs, NoGrants> {
        let live = self.serving_tx.borrow().clone();
        // The pool the engine itself reports (`context_geometry.allocated_tokens`), never a
        // published share multiplied back by a count (Cormac on #5733).
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
        let per_sequence_cap = match self.on_shared_pool().ok_or(NoGrants::PoolKindUnknown)? {
            true => pool_tokens.min(trained.get()),
            false => live.served_context_window,
        };
        // Only a measured mind has a demand: an unmeasured one has no number to grant (her first view
        // measures her), never the population's or anyone else's. The model's trained window is the one
        // hard limit on both her floor and her target (her context compresses to it).
        let mut excluded = std::collections::BTreeMap::new();
        let mut demands = Vec::new();
        for r in &self.serving_demand().requirements {
            match demand_of(r, trained, per_sequence_cap) {
                Ok(demand) => demands.push(demand),
                Err(why) => {
                    excluded.insert(r.persona, why);
                }
            }
        }
        Ok(GrantInputs { demands, pool: pool_tokens, cap: per_sequence_cap, excluded })
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

/// Why a demand the publisher received holds no grant after it. A need no sequence or pool can hold
/// is named before the queue.
fn waiting_reason(d: &Demand<uuid::Uuid>, pool: u32, queued: Option<(usize, usize)>) -> Waiting {
    let need = d.need.get();
    match queued {
        _ if need > d.ceiling => Waiting::NeedAboveCeiling { need, ceiling: d.ceiling },
        _ if need > pool => Waiting::NeedAbovePool { need, pool },
        Some((position, of)) => Waiting::Queued { position, of },
        None if crate::capacity::window_grant::is_grant_retiring(d.mind) => Waiting::Reclaiming,
        None => Waiting::Unplaced,
    }
}

/// Her requirement as a demand, or why it is none.
fn demand_of(
    r: &crate::inference::planning::window_allocator::LaneRequirement,
    trained: crate::inference::planning::window_allocator::Tokens,
    per_sequence_cap: u32,
) -> Result<Demand<uuid::Uuid>, Excluded> {
    // A bound is clamped to what one sequence holds (`Observed::within`); an exact need is not. A wish
    // no held turn has measured is no floor, only the most she might need: clamped to what one
    // sequence holds, so she is granted it and her first held turn measures her. Read as exact, a wish
    // above the ceiling was never granted, never held, never measured: every act refused for good
    // (BigMama, Sahar on 10523b95e: 'NeedAboveCeiling need127103 ceiling58112' while her fit was
    // 28,928). A declared size stays a hard floor (an authored requirement), and the wish never enters
    // `Observed`, so no ring reader can take it for a sample (Codex).
    let observed = r.measure.observed_need().ok_or(Excluded::NeedUnobserved)?.map(|need| need.min(trained));
    let need = match (r.measure, crate::inference::planning::window_allocator::Tokens::new(per_sequence_cap)) {
        (crate::inference::planning::window_allocator::Measure::Wished { .. }, Some(ceiling)) => observed.value().min(ceiling),
        _ => observed.within(per_sequence_cap),
    };
    // Her target is room to grow over what her turns hold (her held need with the sent
    // headroom), never her untrimmed demand: every held token slows every act (the M5,
    // 2026-11-07: targets of 150-557k grew grants to 197k, 71 acts vs 158; 7b7a696a).
    // Growing past it is a priced decision, never "fill because the pool has room".
    let headroom = crate::inference::planning::serving_plan::SENT_HEADROOM;
    let target = crate::inference::planning::window_allocator::Tokens::new((f64::from(need.get()) * headroom) as u32)
        .ok_or(Excluded::NoTarget)?
        .min(trained);
    Ok(Demand {
        mind: r.persona,
        need,
        target,
        wish: r.wish.ok_or(Excluded::NoWish)?.min(trained),
        in_flight: crate::cognition::turn_ledger::turn_in_flight(r.persona),
        // read off her receipts by the fair publisher, which owns them (`publish_fair_grants`)
        settled: false,
        ceiling: per_sequence_cap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::{observed::Observed, LaneRequirement, Tokens};

    fn t(n: u32) -> Tokens {
        Tokens::new(n).expect("test: a nonzero size") // expect: every size here is a positive literal
    }

    // what this catches (BigMama, Sahar on 10523b95e, 2026-11-11: 'NeedAboveCeiling need127103
    // ceiling58112', 0 acts while her rendered fit was 28,928): her wish, never measured by a held
    // turn, was read as an exact need, so above the ceiling she was never granted, never held a turn,
    // and never measured the floor that would have seated her. A wish is granted the ceiling; a floor
    // a held turn measured above it still waits for room (#5773), and a declared size is untouched.
    #[test]
    fn an_unheld_wish_above_the_ceiling_is_granted_the_ceiling_and_a_held_floor_still_waits() {
        let (ceiling, pool, trained) = (58_112, 116_224, t(262_144));
        let her = uuid::Uuid::from_u128(1);
        let wished = demand_of(&LaneRequirement::wished(her, t(127_103), 1.0), trained, ceiling).expect("test: a wish is a demand"); // expect: a wished requirement always has a need, target and wish
        assert_eq!(wished.need.get(), ceiling, "her wish is granted all one sequence holds");
        assert!(!matches!(waiting_reason(&wished, pool, None), Waiting::NeedAboveCeiling { .. }), "never parked above the ceiling");
        let held = LaneRequirement::wished(her, t(127_103), 1.0).held(Observed::Exact(t(100_000)));
        let held = demand_of(&held, trained, ceiling).expect("test: a held need is a demand"); // expect: a held requirement always has a need
        assert_eq!(held.need.get(), 100_000, "a floor her turns measured is her need");
        assert!(matches!(waiting_reason(&held, pool, None), Waiting::NeedAboveCeiling { .. }), "a measured floor past the ceiling still waits, as before");
    }
}
