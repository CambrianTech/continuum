//! Evidence from the same locked transaction that publishes grants. No admission authority.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DispatchDiagnosis {
    /// Owed minds and the publication each became owed at, oldest first.
    owed: Vec<(uuid::Uuid, u64)>,
    holders: BTreeMap<uuid::Uuid, Holder>,
}

#[derive(Debug, PartialEq, Eq)]
struct Holder {
    live: usize,
    unresolved: usize,
    rendered: usize,
    terminal_revision: Option<u64>,
    retiring: bool,
    reclaimed: bool,
    refused: bool,
    /// Armed to yield her seat when her act completes (an owed mind waits behind it).
    armed: bool,
    published_tokens: Option<u32>,
}

impl DispatchDiagnosis {
    fn capture(dispatch: &DispatchState, state: &ReservationState, next: &PoolGrants) -> Self {
        // Include claims removed from the publication while a physical handoff is pending.
        let minds: BTreeSet<_> = next
            .by_mind
            .keys()
            .chain(state.entries.keys())
            .chain(state.retiring.keys())
            .chain(state.reclaimed.keys())
            .chain(state.refused.keys())
            .chain(state.yield_after.keys())
            .copied()
            .collect();
        let holders = minds
            .into_iter()
            .map(|mind| {
                let (mut live, mut unresolved, mut rendered) = (0, 0, 0);
                for entry in state.entries.get(&mind).into_iter().flatten() {
                    match entry.custody {
                        Custody::Live => live += 1,
                        Custody::Unresolved => unresolved += 1,
                        Custody::Rendered => rendered += 1,
                    }
                }
                let holder = Holder {
                    live,
                    unresolved,
                    rendered,
                    terminal_revision: state.receipts.get(&mind).map(|(_, revision)| *revision),
                    retiring: state.retiring.contains_key(&mind),
                    reclaimed: state.reclaimed.contains_key(&mind),
                    refused: state.refused.contains_key(&mind),
                    armed: state.yield_after.contains_key(&mind),
                    published_tokens: next.by_mind.get(&mind).copied(),
                };
                (mind, holder)
            })
            .collect();
        let mut owed: Vec<_> = dispatch.owed.iter().map(|(mind, at)| (*mind, *at)).collect();
        owed.sort_by_key(|(_, at)| *at);
        Self { owed, holders }
    }
}

pub(super) fn observe(dispatch: &mut DispatchState, state: &ReservationState, next: &PoolGrants) {
    if dispatch.owed.is_empty() {
        if dispatch.diagnosis.take().is_some() {
            continuum_base::probe!(class = "serving.grants.owed_cleared", engine = ?next.engine,
                "every ready mind holds her need");
        }
        return;
    }
    let diagnosis = DispatchDiagnosis::capture(dispatch, state, next);
    if dispatch.diagnosis.as_ref() != Some(&diagnosis) {
        continuum_base::probe!(class = "serving.grants.owed_custody", engine = ?next.engine,
            pool_tokens = next.pool_tokens, diagnosis = ?diagnosis,
            "owed minds and physical claims from the grant publication transaction; no custody is released by this observation");
        dispatch.diagnosis = Some(diagnosis);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a newer terminal receipt must not disguise an older unresolved reservation
    // as free, and reading the diagnostic must neither consume that receipt nor release the claim.
    #[test]
    fn older_unresolved_custody_remains_visible_beside_newer_service() {
        let owner = Reservations::new();
        let holder = uuid::Uuid::from_u128(1);
        let waiter = uuid::Uuid::from_u128(2);
        let mut dispatch = DispatchState::default();
        dispatch.owed.insert(waiter, 3);
        let mut state = owner.lock();
        state.entries.insert(
            holder,
            vec![
                Entry { id: 1, tokens: 60, engine: None, custody: Custody::Unresolved },
                Entry { id: 2, tokens: 60, engine: None, custody: Custody::Live },
            ],
        );
        state.receipts.insert(holder, (None, 8));
        let next = PoolGrants { pool_tokens: 100, per_sequence_cap: 100, by_mind: BTreeMap::from([(holder, 60)]), engine: None, revisions: BTreeMap::new(), owed: BTreeMap::new() };
        let snapshot = DispatchDiagnosis::capture(&dispatch, &state, &next);
        assert_eq!(snapshot.owed, vec![(waiter, 3)]);
        let held = &snapshot.holders[&holder];
        assert_eq!((held.live, held.unresolved, held.terminal_revision), (1, 1, Some(8)));
        assert_eq!(state.entries[&holder].len(), 2, "observation releases no custody");
        assert_eq!(state.receipts[&holder], (None, 8));
    }
}
