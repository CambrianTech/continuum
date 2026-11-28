//! The decision's record, the decider that writes it, and the replay that compares policies on it.
//! hot-path: the lane decides at every Core step (THE-LANE-ON-SAGE.md, "The types").
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
    /// Every record slot holds an open decision: nothing is decided until `open`'s outcome arrives.
    Full { open: DecisionId },
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
        Self { id: DecisionId(0), issued: 0, closed: None, read: Vec::new(), situation: U::empty(), policy: PolicyId { name: "", parameters: 0 }, verdict: Verdict::Vacant, outcome: None, violation: None } // cold: a ring slot is built once, at the decider's construction
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
        Self { policy, admissible: Vec::new(), ring: (0..capacity.get()).map(|_| Record::vacant()).collect(), next: 0, clock: 0 } // cold: the decider is built once; its buffers are reused at every decision
    }

    pub fn policy(&self) -> PolicyId {
        self.policy.id()
    }

    /// One of the policy's parameters by name, as it stands now.
    pub fn param(&self, name: &str) -> Option<super::Param> {
        self.policy.param(name)
    }

    /// The policy's learned state, appended to `into`, for its owner's saved state.
    pub fn save(&self, into: &mut Vec<u8>) {
        self.policy.save(into);
    }

    /// Resume what the policy learned before; refused bytes leave it as it was.
    pub fn load(&mut self, from: &[u8]) -> Result<(), super::Refused> {
        self.policy.load(from)
    }

    fn slot(&self, id: DecisionId) -> usize {
        (id.0 % self.ring.len() as u64) as usize
    }

    /// The actions the last decision's envelope offered, as it wrote them (a forced gate may write none): what a
    /// caller reads to tell an action never offered from one offered and not chosen.
    pub fn admissible(&self) -> &[U::Action] {
        &self.admissible
    }

    /// Decide in `situation`, which was built from the revisions in `read`.
    pub fn decide(&mut self, situation: &U::Situation, read: &[InputRevision]) -> Decided<U::Action> {
        let id = DecisionId(self.next);
        // The slot this decision would reuse still holds an open one: never overwrite it (its late
        // outcome would vanish before its constraint is checked and the policy learns). Nothing is
        // decided until that outcome arrives (Codex on #6278).
        let reused = &self.ring[self.slot(id)];
        if reused.closed.is_none() && reused.action().is_some() {
            return Decided::Full { open: reused.id };
        }
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

