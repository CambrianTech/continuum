//! The decision's record, the decider that writes it, and the replay that compares policies on it.
//!
//! The decider allocates once: a ring of record slots, found by `id % capacity`, and one buffer of
//! admissible actions, both reused at every decision. One counter stamps every decision and every
//! outcome, so replay can hand another policy the same sequence of choices and learned outcomes the
//! live policy saw, in the order it saw them.

use std::num::NonZeroUsize;

use crate::dataflow::InputRevision;

use super::policy::{Policy, PolicyId};
use super::use_case::{Deferred, Gate, UseCase, Violation};

/// One decision, in the order a decider made it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DecisionId(pub u64);

/// What the decider answered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decided<A> {
    /// Carry this out and report its outcome under this id.
    Act(DecisionId, A),
    /// Nothing may be done now; ask again when the observations change.
    Defer(DecisionId, Deferred),
}

/// What was decided, from what, by which policy, and what followed. It names the owner-published
/// revisions the decision read and keeps the compact situation the policy saw (the use case's
/// features), never the owners' payloads (KV, histories, files).
#[derive(Debug)]
pub struct Record<U: UseCase> {
    pub id: DecisionId,
    /// The counter's value when it was decided, and when its outcome arrived.
    pub issued: u64,
    pub closed: Option<u64>,
    pub read: Vec<InputRevision>,
    pub situation: U::Situation,
    pub policy: PolicyId,
    pub verdict: Verdict<U::Action>,
    pub outcome: Option<U::Outcome>,
    pub violation: Option<Violation>,
}

/// What a decision came to: exactly the states that exist.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict<A> {
    /// A ring slot no decision has used yet.
    Vacant,
    /// The envelope forced it; the policy was not asked.
    Forced(A),
    /// The policy chose it, with the value it expected when it has a model of value.
    Chosen { action: A, expected: Option<f64> },
    /// The policy answered an index past the admissible actions; the envelope's first was taken.
    Refused { action: A, index: usize },
    /// Nothing could be done; asked again on fresh observations.
    Deferred(Deferred),
}

impl<A: Copy> Verdict<A> {
    /// The action taken, when one was.
    pub fn action(&self) -> Option<A> {
        match *self {
            Self::Forced(action) | Self::Chosen { action, .. } | Self::Refused { action, .. } => Some(action),
            Self::Vacant | Self::Deferred(_) => None,
        }
    }
}

impl<U: UseCase> Record<U> {
    fn vacant() -> Self {
        Self { id: DecisionId(0), issued: 0, closed: None, read: Vec::new(), situation: U::empty(), policy: PolicyId { name: "", parameters: 0 }, verdict: Verdict::Vacant, outcome: None, violation: None }
    }

    /// The action taken, when one was.
    pub fn action(&self) -> Option<U::Action> {
        self.verdict.action()
    }
}

/// Envelope, then policy, then record. The executor carries out the action and reports its outcome
/// by id; the closed record is what the policy learns from, at that moment, and what replay reads.
pub struct Decider<U: UseCase> {
    policy: Box<dyn Policy<U>>,
    admissible: Vec<U::Action>,
    ring: Vec<Record<U>>,
    next: u64,
    clock: u64,
}

impl<U: UseCase> Decider<U> {
    /// `capacity`: how many records it keeps, declared (a view's window, charged like any memory).
    pub fn new(policy: Box<dyn Policy<U>>, capacity: NonZeroUsize) -> Self {
        Self { policy, admissible: Vec::new(), ring: (0..capacity.get()).map(|_| Record::vacant()).collect(), next: 0, clock: 0 }
    }

    pub fn policy(&self) -> PolicyId {
        self.policy.id()
    }

    fn slot(&self, id: DecisionId) -> usize {
        (id.0 % self.ring.len() as u64) as usize
    }

    /// Decide in `situation`, which was built from the revisions in `read`.
    pub fn decide(&mut self, situation: &U::Situation, read: &[InputRevision]) -> Decided<U::Action> {
        let id = DecisionId(self.next);
        self.next += 1;
        self.clock += 1;
        self.admissible.clear();
        let gate = U::envelope(situation, &mut self.admissible);
        let policy = self.policy.id();
        let verdict = match gate {
            Gate::Forced(action) => Verdict::Forced(action),
            Gate::Defer(why) => Verdict::Deferred(why),
            Gate::Choose => {
                let choice = self.policy.choose(situation, &self.admissible);
                match (self.admissible.get(choice.index), self.admissible.first()) {
                    (Some(&action), _) => Verdict::Chosen { action, expected: choice.expected },
                    (None, Some(&first)) => {
                        continuum_base::probe!(class = "sage.decide.refused", use_case = U::NAME, policy = policy.name, index = choice.index, "a policy chose past its admissible actions; the first was taken");
                        Verdict::Refused { action: first, index: choice.index }
                    }
                    // `Choose` with nothing written breaks the envelope's contract: deferred, named.
                    (None, None) => Verdict::Deferred(Deferred { condition: "admissible actions an envelope wrote for Choose", have: 0.0, need: 1.0 }),
                }
            }
        };
        let at = self.slot(id);
        let slot = &mut self.ring[at];
        if slot.closed.is_none() && slot.action().is_some() {
            continuum_base::probe!(class = "sage.decide.overwritten", use_case = U::NAME, id = slot.id.0, "a decision still open was overwritten: its outcome came later than the record window");
        }
        slot.id = id;
        slot.issued = self.clock;
        slot.closed = None;
        slot.read.clear();
        slot.read.extend_from_slice(read);
        U::retain(&mut slot.situation, situation);
        slot.policy = policy;
        slot.verdict = verdict;
        slot.outcome = None;
        slot.violation = None;
        match verdict {
            Verdict::Deferred(why) => Decided::Defer(id, why),
            Verdict::Forced(action) | Verdict::Chosen { action, .. } | Verdict::Refused { action, .. } => Decided::Act(id, action),
            Verdict::Vacant => Decided::Defer(id, Deferred { condition: "a decision that was never made", have: 0.0, need: 1.0 }),
        }
    }

    /// The executor's measured outcome for `id`: the record closes, its constraint is checked, and
    /// the policy learns from it now. `None` when `id` is not an open decision held here.
    pub fn outcome(&mut self, id: DecisionId, outcome: U::Outcome) -> Option<&Record<U>> {
        let at = self.slot(id);
        let slot = &mut self.ring[at];
        if slot.id != id || slot.closed.is_some() || slot.action().is_none() {
            return None;
        }
        self.clock += 1;
        slot.closed = Some(self.clock);
        slot.violation = U::constraint(&outcome);
        if let Some(violation) = &slot.violation {
            continuum_base::probe!(class = "sage.decide.violated", use_case = U::NAME, constraint = violation.constraint, measured = violation.measured, limit = violation.limit, "an outcome broke its use case's constraint");
        }
        slot.outcome = Some(outcome);
        self.policy.learn(slot);
        Some(slot)
    }

    /// The records it holds, in the order they were decided.
    pub fn records(&self) -> impl Iterator<Item = &Record<U>> {
        let first = self.next.saturating_sub(self.ring.len() as u64);
        (first..self.next).map(|id| &self.ring[self.slot(DecisionId(id))]).filter(move |record| record.id.0 >= first)
    }
}

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
