//! Evidence from the same locked transaction that publishes grants. No admission authority.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct QueueDiagnosis {
    waiters: Vec<uuid::Uuid>,
    holders: BTreeMap<uuid::Uuid, Holder>,
}

#[derive(Debug, PartialEq, Eq)]
struct Holder {
    live: usize,
    unresolved: usize,
    terminal_revision: Option<u64>,
    baseline: Option<u64>,
    retiring: bool,
    reclaimed: bool,
    refused: bool,
    published_tokens: Option<u32>,
}

impl QueueDiagnosis {
    fn capture(fair: &FairGrantState, state: &ReservationState, next: &PoolGrants) -> Self {
        // Include claims removed from the publication while a physical handoff is pending.
        let minds: BTreeSet<_> = next
            .by_mind
            .keys()
            .chain(state.entries.keys())
            .chain(state.retiring.keys())
            .chain(state.reclaimed.keys())
            .chain(state.refused.keys())
            .copied()
            .collect();
        let holders = minds
            .into_iter()
            .map(|mind| {
                let entries = state.entries.get(&mind).into_iter().flatten();
                let (mut live, mut unresolved) = (0, 0);
                for entry in entries {
                    match entry.custody {
                        Custody::Live => live += 1,
                        Custody::Unresolved => unresolved += 1,
                    }
                }
                (
                    mind,
                    Holder {
                        live,
                        unresolved,
                        terminal_revision: state.receipts.get(&mind).map(|(_, revision)| *revision),
                        baseline: fair.baseline.get(&mind).copied(),
                        retiring: state.retiring.contains_key(&mind),
                        reclaimed: state.reclaimed.contains_key(&mind),
                        refused: state.refused.contains_key(&mind),
                        published_tokens: next.by_mind.get(&mind).copied(),
                    },
                )
            })
            .collect();
        Self {
            waiters: fair.waiters.iter().copied().collect(),
            holders,
        }
    }
}

pub(super) fn observe(fair: &mut FairGrantState, state: &ReservationState, next: &PoolGrants) {
    if fair.waiters.is_empty() {
        if fair.diagnosis.take().is_some() {
            continuum_base::probe!(class = "serving.grants.queue_cleared", engine = ?next.engine,
                "the grant owner's waiting queue cleared");
        }
        return;
    }
    let diagnosis = QueueDiagnosis::capture(fair, state, next);
    if fair.diagnosis.as_ref() != Some(&diagnosis) {
        continuum_base::probe!(class = "serving.grants.queue_custody", engine = ?next.engine,
            pool_tokens = next.pool_tokens, diagnosis = ?diagnosis,
            "queued minds and physical claims from the grant publication transaction; no custody is released by this observation");
        fair.diagnosis = Some(diagnosis);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A newer terminal receipt must not disguise an older unresolved reservation as free.
    // Reading diagnostic evidence must neither consume that receipt nor release the claim.
    #[test]
    fn older_unresolved_custody_remains_visible_beside_newer_service() {
        let owner = Reservations::new();
        let holder = uuid::Uuid::from_u128(1);
        let waiter = uuid::Uuid::from_u128(2);
        let mut fair = FairGrantState::default();
        fair.waiters.push_back(waiter);
        fair.baseline.insert(holder, 7);
        let mut state = owner.lock();
        state.entries.insert(
            holder,
            vec![
                Entry {
                    id: 1,
                    tokens: 60,
                    engine: None,
                    custody: Custody::Unresolved,
                },
                Entry {
                    id: 2,
                    tokens: 60,
                    engine: None,
                    custody: Custody::Live,
                },
            ],
        );
        state.receipts.insert(holder, (None, 8));
        let next = PoolGrants {
            pool_tokens: 100,
            per_sequence_cap: 100,
            by_mind: BTreeMap::from([(holder, 60)]),
            engine: None,
            revisions: BTreeMap::new(),
        };
        let snapshot = QueueDiagnosis::capture(&fair, &state, &next);
        assert_eq!(snapshot.waiters, vec![waiter]);
        let held = &snapshot.holders[&holder];
        assert_eq!((held.live, held.unresolved), (1, 1));
        assert_eq!((held.terminal_revision, held.baseline), (Some(8), Some(7)));
        assert_eq!(
            state.entries[&holder].len(),
            2,
            "observation releases no custody"
        );
        assert_eq!(state.receipts[&holder], (None, 8));
    }
}
