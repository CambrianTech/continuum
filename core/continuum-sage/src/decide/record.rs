//! The decision's record, the decider that writes it, and the replay that compares policies on it.

use std::collections::VecDeque;

use crate::dataflow::InputRevision;

use super::policy::{Policy, PolicyId};
use super::use_case::{Envelope, UseCase, Violation};

/// One decision, in the order a decider made it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DecisionId(pub u64);

/// What was decided, from what, by which policy, and what followed. It names the owner-published
/// revisions the decision read and keeps the compact situation the policy saw (the use case's
/// features, a few numbers per subject), never the owners' payloads (KV, histories, files).
#[derive(Debug, Clone)]
pub struct Record<U: UseCase> {
    pub id: DecisionId,
    pub read: Vec<InputRevision>,
    pub situation: U::Situation,
    pub policy: PolicyId,
    pub action: U::Action,
    /// The envelope forced the action; the policy was not asked.
    pub forced: bool,
    /// The policy chose outside the envelope: what it chose, refused; `action` is the envelope's first.
    pub refused: Option<U::Action>,
    pub expected: Option<f64>,
    /// Reported by the executor; `None` while the decision is open.
    pub outcome: Option<U::Outcome>,
    pub violation: Option<Violation>,
}

/// Envelope, then policy, then record. The executor carries out the action and reports its outcome
/// by id; the closed record is what the policy learns from and what replay reads.
pub struct Decider<U: UseCase> {
    policy: Box<dyn Policy<U>>,
    next: u64,
    open: VecDeque<Record<U>>,
    closed: VecDeque<Record<U>>,
    retain: usize,
}

impl<U: UseCase> Decider<U> {
    /// `retain`: how many closed records it keeps, declared (a view's window, charged like any memory).
    pub fn new(policy: Box<dyn Policy<U>>, retain: usize) -> Self {
        Self { policy, next: 0, open: VecDeque::new(), closed: VecDeque::new(), retain: retain.max(1) }
    }

    pub fn policy(&self) -> PolicyId {
        self.policy.id()
    }

    /// Decide in `situation`, which was built from the revisions in `read`.
    pub fn decide(&mut self, situation: U::Situation, read: &[InputRevision]) -> (DecisionId, U::Action) {
        let read = read.to_vec();
        let id = DecisionId(self.next);
        self.next += 1;
        let policy = self.policy.id();
        let (action, forced, refused, expected) = match U::envelope(&situation) {
            Envelope::Forced(action) => (action, true, None, None),
            Envelope::Admissible(actions) => {
                let choice = self.policy.choose(&situation, &actions);
                if actions.contains(&choice.action) {
                    (choice.action, false, None, choice.expected)
                } else {
                    continuum_base::probe!(class = "sage.decide.refused", use_case = U::NAME, policy = policy.name, "a policy chose outside its envelope; the envelope's first action was taken");
                    (actions.first().clone(), false, Some(choice.action), None)
                }
            }
        };
        let record = Record { id, read, situation, policy, action: action.clone(), forced, refused, expected, outcome: None, violation: None };
        self.open.push_back(record);
        (id, action)
    }

    /// The executor's measured outcome for `id`: the record closes, its constraint is checked, and
    /// the policy learns from it. `None` when no open decision has that id.
    pub fn outcome(&mut self, id: DecisionId, outcome: U::Outcome) -> Option<&Record<U>> {
        let at = self.open.iter().position(|record| record.id == id)?;
        let mut record = self.open.remove(at)?;
        record.violation = U::constraint(&outcome);
        if let Some(violation) = &record.violation {
            continuum_base::probe!(class = "sage.decide.violated", use_case = U::NAME, constraint = violation.constraint, measured = violation.measured, limit = violation.limit, "an outcome broke its use case's constraint");
        }
        record.outcome = Some(outcome);
        self.policy.learn(&record);
        if self.closed.len() == self.retain {
            self.closed.pop_front();
        }
        self.closed.push_back(record);
        self.closed.back()
    }

    /// The closed records it retains, oldest first.
    pub fn closed(&self) -> impl Iterator<Item = &Record<U>> {
        self.closed.iter()
    }
}

/// One recorded decision, replayed against another policy.
#[derive(Debug, Clone)]
pub struct Replayed<U: UseCase> {
    pub id: DecisionId,
    pub recorded: U::Action,
    pub chosen: U::Action,
    /// The recorded outcome, only where the replayed policy chose what was done: an action not taken
    /// has no observed outcome, and replay never pretends it does.
    pub outcome: Option<U::Outcome>,
}

/// Replay `records` against `policy`: each situation as it was recorded, through the same envelope,
/// with nothing later visible. The producer and the executor are not involved.
pub fn replay<'r, U: UseCase>(records: impl IntoIterator<Item = &'r Record<U>>, policy: &mut dyn Policy<U>) -> Vec<Replayed<U>> {
    records
        .into_iter()
        .map(|record| {
            let chosen = match U::envelope(&record.situation) {
                Envelope::Forced(action) => action,
                Envelope::Admissible(actions) => {
                    let choice = policy.choose(&record.situation, &actions).action;
                    if actions.contains(&choice) { choice } else { actions.first().clone() }
                }
            };
            let outcome = if chosen == record.action { record.outcome.clone() } else { None };
            Replayed { id: record.id, recorded: record.action.clone(), chosen, outcome }
        })
        .collect()
}
