//! Receipt-driven grant handoff. Physical claims stay charged until a reclaim proof is
//! consumed in the same transaction that publishes replacement grants.
use super::*;
use crate::capacity::window_grant::{stamp_revisions, DemandFollowing, PoolGrants};
use std::collections::{BTreeSet, VecDeque};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct FairGrantState {
    engine: Option<EngineIncarnation>,
    baseline: BTreeMap<uuid::Uuid, u64>,
    waiters: VecDeque<uuid::Uuid>,
}

/// The callback publishes while the reservation lock is held. Tickets leave only after the
/// old holder is absent from the publication and its physical claim is fixed separately.
pub fn publish_fair_grants<R>(
    fair: &mut FairGrantState,
    demands: &[Demand<uuid::Uuid>],
    pool: u32,
    ceiling: u32,
    engine: Option<EngineIncarnation>,
    current: impl FnOnce() -> Option<PoolGrants>,
    publish: impl FnOnce(PoolGrants) -> R,
) -> (R, Vec<GrantHandoff>) {
    RESERVATIONS.fair_publish(fair, demands, pool, ceiling, engine, current, publish)
}

impl Reservations {
    fn fair_publish<R>(
        &self,
        fair: &mut FairGrantState,
        demands: &[Demand<uuid::Uuid>],
        pool: u32,
        ceiling: u32,
        engine: Option<EngineIncarnation>,
        current: impl FnOnce() -> Option<PoolGrants>,
        publish: impl FnOnce(PoolGrants) -> R,
    ) -> (R, Vec<GrantHandoff>) {
        let mut state = self.lock();
        if fair.engine != engine {
            *fair = FairGrantState {
                engine,
                ..Default::default()
            };
        }
        for entries in state.entries.values_mut() {
            entries.retain(|entry| entry.engine == engine);
        }
        state.entries.retain(|_, entries| !entries.is_empty());
        state
            .receipts
            .retain(|_, (incarnation, _)| *incarnation == engine);
        state.retiring.retain(|_, lease| lease.engine == engine);
        state.reclaimed.retain(|_, lease| lease.engine == engine);
        let current = current();
        let held = current
            .as_ref()
            .filter(|g| g.engine == engine)
            .map(|g| g.by_mind.clone())
            .unwrap_or_default(); // no publication on this engine holds no grants
        let live: BTreeSet<_> = demands.iter().map(|d| d.mind).collect();
        fair.baseline.retain(|mind, _| live.contains(mind));
        fair.waiters.retain(|mind| live.contains(mind));
        for demand in demands {
            if demand.need.get() <= demand.ceiling
                && demand.need.get() <= pool
                && !held
                    .get(&demand.mind)
                    .is_some_and(|tokens| *tokens >= demand.need.get())
                && !fair.waiters.contains(&demand.mind)
            {
                fair.waiters.push_back(demand.mind);
            }
        }
        // Reclaimed capacity is consumed only here, under the publish/reserve transaction.
        let reclaimed: BTreeSet<_> = state
            .reclaimed
            .iter()
            .filter_map(|(mind, lease)| {
                state
                    .retiring
                    .get(mind)
                    .filter(|retired| retired.same_identity(lease))
                    .map(|_| *mind)
            })
            .collect();
        for mind in &reclaimed {
            state.retiring.remove(mind);
            state.reclaimed.remove(mind);
            if live.contains(mind) && !fair.waiters.contains(mind) {
                fair.waiters.push_back(*mind);
            }
        }
        let mut held_for_policy = held.clone();
        for mind in reclaimed.iter().chain(state.retiring.keys()) {
            held_for_policy.remove(mind);
        }
        let mut ordered = demands.to_vec();
        ordered.sort_by_key(|d| fair.waiters.iter().position(|mind| *mind == d.mind));
        // Option sorts None first; explicitly put queued waiters ahead of non-waiters.
        ordered.sort_by_key(|d| !fair.waiters.contains(&d.mind));
        let compute = |state: &ReservationState, held: &BTreeMap<uuid::Uuid, u32>| {
            let mut fixed: BTreeMap<_, _> = state
                .entries
                .iter()
                .filter_map(|(mind, entries)| {
                    entries
                        .iter()
                        .map(|entry| entry.tokens)
                        .max()
                        .map(|tokens| (*mind, tokens))
                })
                .collect();
            for (mind, lease) in &state.retiring {
                fixed.insert(*mind, lease.tokens);
            }
            // Actual dispatch reservations above own in-flight protection. A service-loop
            // boolean can outlive terminal/erase receipts and must not outrank queued minds.
            let open: Vec<_> = ordered
                .iter()
                .filter(|d| !state.retiring.contains_key(&d.mind))
                .map(|d| Demand {
                    in_flight: false,
                    ..*d
                })
                .collect();
            let mut grants =
                grant_with_reservations(&mut DemandFollowing, &open, held, pool, &fixed);
            grants.retain(|mind, _| !state.retiring.contains_key(mind));
            grants
        };
        let mut grants = compute(&state, &held_for_policy);
        let waiter = fair
            .waiters
            .iter()
            .find(|mind| {
                demands.iter().any(|d| {
                    d.mind == **mind && !grants.get(mind).is_some_and(|n| *n >= d.need.get())
                })
            })
            .copied();
        let mut tickets = Vec::new();
        if let (Some(waiter), Some(_)) = (waiter, engine) {
            if state.retiring.is_empty() {
                let candidate = held.iter().find_map(|(mind, _)| {
                    if *mind == waiter
                        || state.entries.contains_key(mind)
                        || reclaimed.contains(mind)
                    {
                        return None;
                    }
                    let (_, revision) = state.receipts.get(mind)?;
                    if *revision <= fair.baseline.get(mind).copied().unwrap_or(0) {
                        return None;
                    } // no service since grant
                    let lease = current.as_ref()?.lease_for(*mind)?;
                    Some((*mind, lease, *revision))
                });
                if let Some((mind, lease, receipt)) = candidate {
                    state.retiring.insert(mind, lease);
                    fair.baseline.insert(mind, receipt);
                    held_for_policy.remove(&mind);
                    tickets.push(GrantHandoff {
                        persona: mind,
                        lease,
                        receipt,
                    });
                    grants = compute(&state, &held_for_policy);
                }
            }
        }
        for mind in grants.keys() {
            if !held.contains_key(mind) || reclaimed.contains(mind) {
                let revision = state
                    .receipts
                    .get(mind)
                    .map_or(0, |(_, revision)| *revision);
                fair.baseline.insert(*mind, revision);
            }
        }
        fair.waiters.retain(|mind| {
            !grants.get(mind).is_some_and(|tokens| {
                demands
                    .iter()
                    .any(|d| d.mind == *mind && *tokens >= d.need.get())
            })
        });
        let mut next = PoolGrants {
            pool_tokens: pool,
            per_sequence_cap: ceiling,
            by_mind: grants,
            engine,
            revisions: BTreeMap::new(),
        };
        stamp_revisions(current.as_ref(), &mut next);
        (publish(next), tickets)
    }
}
