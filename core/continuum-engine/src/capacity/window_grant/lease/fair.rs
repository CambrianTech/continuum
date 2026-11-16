//! Dispatch at the act (CONTINUUM-ENGINE 4.9 P3, I3 and I4): grants follow the minds that are acting.
//!
//! - A mind with a request in custody (an act hold, a reservation, an unresolved request) is ACTING:
//!   her grant is fixed and never reclaimed.
//! - A ready mind short of her need is OWED. The oldest owed is served first (aging, never a queue:
//!   no position, no head-of-line hold, no idle-for-two-evaluations).
//! - She takes free cells first. Short, every NON-acting holder she needs is reclaimed in ONE
//!   evaluation, coolest first: a seat whose holder never served on this engine holds no KV and is
//!   withdrawn at once; a seat with KV leaves through a [`GrantHandoff`] and its cells stay charged
//!   until the engine's reclaim receipt (Codex's #6040).
//! - When every holder is acting she waits behind their acts; an act's end moves demand and wakes
//!   the next evaluation.
//!
//! Physical claims stay charged until a reclaim proof is consumed in the same transaction that
//! publishes replacement grants.
use super::*;
use crate::capacity::window_grant::{stamp_revisions, DemandFollowing, PoolGrants};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;
mod diagnosis;

/// Why a ready mind holds less than her need after an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchWait {
    /// Cells are being reclaimed for her: she is granted after the engine's receipt.
    Reclaiming,
    /// Every holder that could make room is mid-act: she is served when an act ends.
    BehindActs { acting: usize },
}

/// The dispatch owner's memory across evaluations on one engine incarnation.
#[derive(Default)]
pub struct DispatchState {
    engine: Option<EngineIncarnation>,
    publication: u64,
    /// The publication each ready mind became owed at.
    owed: BTreeMap<uuid::Uuid, u64>,
    /// For each mind seated since: the publication her wait began and her receipt revision when
    /// seated. Until she completes an act on it, her seat is reclaimed only for a mind that has waited
    /// at least as long (aging both ways, so no fresh seat thrashes).
    seated: BTreeMap<uuid::Uuid, (u64, Option<u64>)>,
    /// A holder whose reclaim the engine refused, at her receipt revision then: she is not asked again
    /// until she completes another act.
    refused_at: BTreeMap<uuid::Uuid, Option<u64>>,
    /// Each mind's receipt revision when her grant last changed: she is settled (her grant may shrink
    /// toward her share) once a receipt past it says she served on it.
    changed: BTreeMap<uuid::Uuid, u64>,
    /// Each seat being reclaimed (its holder retiring) and the ready mind it is reclaimed for, kept
    /// across evaluations until the engine's receipt: a later evaluation counts those cells toward
    /// her need before it reclaims more, and cells reclaimed for a mind who left go to the pool.
    reclaiming: BTreeMap<uuid::Uuid, uuid::Uuid>,
    waits: BTreeMap<uuid::Uuid, DispatchWait>,
    diagnosis: Option<diagnosis::DispatchDiagnosis>,
}

impl DispatchState {
    /// What `mind` waits on after the last evaluation, if she is short.
    pub fn waiting(&self, mind: uuid::Uuid) -> Option<DispatchWait> {
        self.waits.get(&mind).copied()
    }
}

/// The callback publishes while the reservation lock is held. Tickets leave only after the old
/// holder is absent from the publication and its physical claim is fixed separately.
pub fn publish_at_the_act<R>(
    dispatch: &mut DispatchState,
    demands: &[Demand<uuid::Uuid>],
    pool: u32,
    ceiling: u32,
    engine: Option<EngineIncarnation>,
    current: impl FnOnce() -> Option<PoolGrants>,
    publish: impl FnOnce(PoolGrants) -> R,
) -> (R, Vec<GrantHandoff>) {
    RESERVATIONS.dispatch_publish(dispatch, Publication { demands, pool, ceiling, engine }, current, publish)
}

/// One evaluation's inputs.
#[derive(Clone, Copy)]
pub(super) struct Publication<'a> {
    pub demands: &'a [Demand<uuid::Uuid>],
    pub pool: u32,
    pub ceiling: u32,
    pub engine: Option<EngineIncarnation>,
}

/// Whether `holder`'s seat may be reclaimed for a mind owed since `waited_since`: not after the engine
/// refused it until she acts again, and not before she has used it unless the waiter has waited at
/// least as long as she did.
fn reclaimable_for(dispatch: &DispatchState, state: &ReservationState, holder: uuid::Uuid, waited_since: u64) -> bool {
    // her newest receipt revision; none (nothing served here) orders before any
    let served = state.receipts.get(&holder).map(|(_, revision)| *revision);
    if dispatch.refused_at.get(&holder).is_some_and(|at| served <= *at) {
        return false;
    }
    match dispatch.seated.get(&holder) {
        Some((seat_waited, seat_served)) => served > *seat_served || waited_since <= *seat_waited,
        None => true,
    }
}

fn seatable(demand: &Demand<uuid::Uuid>, pool: u32) -> bool {
    demand.need.get() <= demand.ceiling && demand.need.get() <= pool
}

impl Reservations {
    pub(super) fn dispatch_publish<R>(
        &self,
        dispatch: &mut DispatchState,
        publication: Publication<'_>,
        current: impl FnOnce() -> Option<PoolGrants>,
        publish: impl FnOnce(PoolGrants) -> R,
    ) -> (R, Vec<GrantHandoff>) {
        let Publication { demands, pool, ceiling, engine } = publication;
        let mut state = self.lock();
        if dispatch.engine != engine {
            *dispatch = DispatchState { engine, ..Default::default() };
        }
        for entries in state.entries.values_mut() {
            entries.retain(|entry| entry.engine == engine);
        }
        state.entries.retain(|_, entries| !entries.is_empty());
        state.receipts.retain(|_, (incarnation, _)| *incarnation == engine);
        state.retiring.retain(|_, lease| lease.engine == engine);
        state.reclaimed.retain(|_, lease| lease.engine == engine);
        state.refused.retain(|_, lease| lease.engine == engine);
        dispatch.publication += 1;
        let now = dispatch.publication;
        let current = current();
        let held = current.as_ref().filter(|g| g.engine == engine).map(|g| g.by_mind.clone()).unwrap_or_default(); // no publication on this engine holds no grants
        let live: BTreeSet<_> = demands.iter().map(|d| d.mind).collect();
        dispatch.owed.retain(|mind, _| live.contains(mind));
        dispatch.seated.retain(|mind, _| live.contains(mind));
        dispatch.refused_at.retain(|mind, _| live.contains(mind));
        dispatch.changed.retain(|mind, _| live.contains(mind));
        // a reclaim stays counted for its beneficiary while its seat is still retiring and she is still ready
        dispatch.reclaiming.retain(|holder, beneficiary| state.retiring.contains_key(holder) && live.contains(beneficiary));
        // Reclaimed capacity is consumed only here, under the publish/reserve transaction.
        let reclaimed: BTreeSet<_> = state
            .reclaimed
            .iter()
            .filter_map(|(mind, lease)| state.retiring.get(mind).filter(|retired| retired.same_identity(lease)).map(|_| *mind))
            .collect();
        for mind in &reclaimed {
            state.retiring.remove(mind);
            state.reclaimed.remove(mind);
        }
        let mut held_for_policy = held.clone();
        let restored: BTreeMap<_, _> = state
            .refused
            .iter()
            .filter_map(|(mind, lease)| state.retiring.get(mind).filter(|retired| retired.same_identity(lease)).map(|_| (*mind, lease.tokens)))
            .collect();
        for (mind, tokens) in &restored {
            state.retiring.remove(mind);
            state.refused.remove(mind);
            held_for_policy.insert(*mind, *tokens);
            dispatch.refused_at.insert(*mind, state.receipts.get(mind).map(|(_, revision)| *revision));
        }
        for mind in reclaimed.iter().chain(state.retiring.keys()) {
            held_for_policy.remove(mind);
        }
        for demand in demands.iter().filter(|d| seatable(d, pool)) {
            if !held_for_policy.get(&demand.mind).is_some_and(|tokens| *tokens >= demand.need.get()) {
                dispatch.owed.entry(demand.mind).or_insert(now);
            }
        }
        // the oldest owed first, then everyone else in the order demand arrived
        let mut ordered = demands.to_vec();
        ordered.sort_by_key(|d| dispatch.owed.get(&d.mind).copied().unwrap_or(u64::MAX)); // unwrap_or: a mind not owed sorts after every owed one
        let compute = |state: &ReservationState, held: &BTreeMap<uuid::Uuid, u32>| {
            let mut fixed: BTreeMap<_, _> = state
                .entries
                .iter()
                .filter_map(|(mind, entries)| entries.iter().map(|entry| entry.tokens).max().map(|tokens| (*mind, tokens)))
                .collect();
            for (mind, lease) in &state.retiring {
                fixed.insert(*mind, lease.tokens);
            }
            fixed.extend(restored.iter().map(|(mind, tokens)| (*mind, *tokens)));
            let open: Vec<_> = ordered
                .iter()
                .filter(|d| !state.retiring.contains_key(&d.mind))
                .map(|d| Demand {
                    in_flight: false,
                    settled: dispatch.changed.get(&d.mind).is_none_or(|at| state.receipts.get(&d.mind).is_some_and(|(_, served)| served > at)),
                    ..*d
                })
                .collect();
            let mut grants = grant_with_reservations(&mut DemandFollowing, &open, held, pool, &fixed);
            grants.retain(|mind, _| !state.retiring.contains_key(mind));
            grants
        };
        let mut grants = compute(&state, &held_for_policy);
        let mut tickets = Vec::new();
        let mut unseated = Vec::new();
        let mut owed: Vec<(uuid::Uuid, u64)> = dispatch.owed.iter().map(|(mind, at)| (*mind, *at)).collect();
        owed.sort_by_key(|(_, at)| *at);
        for (waiter, waited_since) in owed {
            let Some(need) = demands.iter().find(|d| d.mind == waiter).map(|d| d.need.get()) else { continue };
            // her grant plus the cells already being reclaimed for her covers her need
            let covered = |grants: &BTreeMap<uuid::Uuid, u32>, pending: u32| grants.get(&waiter).map_or(pending, |held| held.saturating_add(pending)) >= need;
            // cells an earlier evaluation began reclaiming for her, still retiring
            let mut pending = dispatch
                .reclaiming
                .iter()
                .filter(|(_, beneficiary)| **beneficiary == waiter)
                .filter_map(|(holder, _)| state.retiring.get(holder))
                .fold(0u32, |sum, lease| sum.saturating_add(lease.tokens));
            if covered(&grants, pending) || engine.is_none() {
                continue;
            }
            // non-acting holders she may take (aging, a refused reclaim not retried before her next
            // act), coolest first: a holder that never served here sorts first
            let mut candidates: Vec<(uuid::Uuid, Option<u64>)> = held_for_policy
                .keys()
                .filter(|mind| **mind != waiter && !state.entries.contains_key(mind) && !restored.contains_key(mind))
                .filter(|mind| reclaimable_for(dispatch, &state, **mind, waited_since))
                .map(|mind| (*mind, state.receipts.get(mind).map(|(_, revision)| *revision)))
                .collect();
            candidates.sort_by_key(|(_, served)| *served);
            // reclaim nothing that cannot seat her: free cells plus every candidate's seat must cover her need
            let claimed: u32 = grants.iter().filter(|(mind, _)| **mind != waiter).fold(0u32, |sum, (_, tokens)| sum.saturating_add(*tokens));
            let charged: u32 = state.retiring.values().fold(0u32, |sum, lease| sum.saturating_add(lease.tokens));
            let free = pool.saturating_sub(claimed.saturating_add(charged));
            let reclaimable = candidates.iter().filter_map(|(mind, _)| held_for_policy.get(mind)).fold(0u32, |sum, tokens| sum.saturating_add(*tokens));
            if free.saturating_add(pending).saturating_add(reclaimable) < need {
                continue;
            }
            for (holder, receipt) in candidates {
                if covered(&grants, pending) {
                    break;
                }
                // every seat leaves through the engine's save and erase, even one this owner never saw
                // serve: after an adoption her KV may be resident with no receipt here (Codex on #6060)
                let Some(lease) = current.as_ref().and_then(|c| c.lease_for(holder)) else { continue };
                state.retiring.insert(holder, lease);
                held_for_policy.remove(&holder);
                pending = pending.saturating_add(lease.tokens);
                tickets.push(GrantHandoff { persona: holder, lease, receipt });
                dispatch.reclaiming.insert(holder, waiter);
                unseated.push(holder);
                grants = compute(&state, &held_for_policy);
            }
        }
        // a holder who gave up her seat waits from now, like any mind that becomes ready
        for holder in unseated {
            dispatch.seated.remove(&holder);
            dispatch.owed.entry(holder).or_insert(now);
        }
        let seated_now: Vec<(uuid::Uuid, u64)> = dispatch
            .owed
            .iter()
            .filter(|(mind, _)| grants.get(mind).is_some_and(|tokens| demands.iter().any(|d| d.mind == **mind && *tokens >= d.need.get())))
            .map(|(mind, at)| (*mind, *at))
            .collect();
        for (mind, waited_since) in seated_now {
            dispatch.owed.remove(&mind);
            dispatch.seated.insert(mind, (waited_since, state.receipts.get(&mind).map(|(_, revision)| *revision)));
        }
        let acting = state.entries.len();
        dispatch.waits = dispatch
            .owed
            .keys()
            .map(|mind| (*mind, if dispatch.reclaiming.values().any(|beneficiary| beneficiary == mind) || !state.retiring.is_empty() { DispatchWait::Reclaiming } else { DispatchWait::BehindActs { acting } }))
            .collect();
        for d in demands {
            if grants.get(&d.mind) != held.get(&d.mind) {
                let served = state.receipts.get(&d.mind).map_or(0, |(_, revision)| *revision); // no receipt yet: she has served nothing on any grant
                dispatch.changed.insert(d.mind, served);
            }
        }
        let mut next = PoolGrants { pool_tokens: pool, per_sequence_cap: ceiling, by_mind: grants, engine, revisions: BTreeMap::new() };
        stamp_revisions(current.as_ref(), &mut next);
        // An owed mind is owed each seat she may take when its holder's act ends: every such holder
        // is armed, so her next reserve after a completed act is refused at the receipt boundary and
        // the next evaluation reclaims the seat (a busy holder never chains acts past a waiter).
        state.yield_after.clear();
        if engine.is_some() {
            for (waiter, waited_since) in &dispatch.owed {
                for holder in next.by_mind.keys().filter(|holder| *holder != waiter) {
                    // mid-act, she will have used her seat when the act completes
                    let acting = state.entries.contains_key(holder);
                    let refused = dispatch.refused_at.get(holder).is_some_and(|at| state.receipts.get(holder).map(|(_, served)| *served) <= *at);
                    if refused || !(acting || reclaimable_for(dispatch, &state, *holder, *waited_since)) {
                        continue;
                    }
                    if let Some(lease) = next.lease_for(*holder) {
                        let baseline = state.receipts.get(holder).map_or(0, |(_, revision)| *revision); // no receipt: no completed act yet
                        state.yield_after.insert(*holder, (lease, baseline));
                    }
                }
            }
        }
        diagnosis::observe(dispatch, &state, &next);
        (publish(next), tickets)
    }
}
