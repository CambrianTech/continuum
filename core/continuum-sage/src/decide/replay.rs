//! Replay: another policy on recorded history, offline (never on the hot path), handed the choices
//! and outcomes in the order the live decider saw them.

use super::policy::Policy;
use super::record::{DecisionId, Record};
use super::use_case::{Gate, UseCase};

/// One recorded decision, replayed against another policy.
#[derive(Debug, Clone, Copy)]
pub struct Replayed<U: UseCase> {
    pub id: DecisionId,
    pub recorded: Option<U::Action>,
    pub chosen: Option<U::Action>,
    /// The recorded outcome, only where the replayed policy chose what was done: an action not taken
    /// has no observed outcome, and replay never pretends it does.
    pub outcome: Option<U::Outcome>,
}

/// Replay `records` (in the order they were decided) against `policy`. Choices and outcomes are
/// handed to it in the order the live decider saw them: a decision when it was issued, an outcome when
/// it arrived, and only for an action the replayed policy also chose, so a policy that learns sees the
/// same sequence it would have seen live. The producer and the executor are not involved.
pub fn replay<'r, U: UseCase>(records: impl IntoIterator<Item = &'r Record<U>>, policy: &mut dyn Policy<U>) -> Vec<Replayed<U>> {
    let records: Vec<&Record<U>> = records.into_iter().collect();
    let mut events: Vec<(u64, bool, usize)> = Vec::with_capacity(records.len() * 2);
    for (index, record) in records.iter().enumerate() {
        events.push((record.issued, false, index));
        if let Some(closed) = record.closed {
            events.push((closed, true, index));
        }
    }
    events.sort_unstable();
    let mut admissible = Vec::new();
    let mut replayed: Vec<Replayed<U>> = records.iter().map(|record| Replayed { id: record.id, recorded: record.action(), chosen: None, outcome: None }).collect();
    for (_, is_outcome, index) in events {
        let record = records[index];
        if is_outcome {
            if replayed[index].chosen == record.action() {
                replayed[index].outcome = record.outcome;
                policy.learn(record);
            }
            continue;
        }
        admissible.clear();
        replayed[index].chosen = match U::envelope(&record.situation, &mut admissible) {
            Gate::Forced(action) => Some(action),
            Gate::Defer(_) => None,
            Gate::Choose => admissible.get(policy.choose(&record.situation, &admissible).index).or(admissible.first()).copied(),
        };
    }
    replayed
}
