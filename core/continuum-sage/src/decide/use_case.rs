//! What a decision is: its situation, its actions, its outcome, its envelope and its constraints.
//! A decision runs every few milliseconds on the lane, so nothing here allocates in steady state: the
//! envelope fills a buffer the decider owns and reuses, and a policy answers an index into it.

use std::fmt::Debug;

/// One kind of decision. Its types are its own; nothing here knows what a lane, a drive or a peer is.
pub trait UseCase: 'static {
    /// What a policy sees at a decision: built by the use case's owner from owner-published state,
    /// handed by reference, copied into a record slot with [`UseCase::retain`] (reusing its buffers).
    type Situation: Debug + Send;
    /// One action available at a decision: small and `Copy` (indices and masks, never owned data).
    type Action: Copy + Debug + PartialEq + Send;
    /// What the executor measured after the action was taken.
    type Outcome: Copy + Debug + Send;

    /// Its name, as records and probes label it.
    const NAME: &'static str;

    /// The never-learned bound on every policy. Fill `admissible` (cleared by the caller) and answer
    /// [`Gate::Choose`], or answer the one action the situation forces, or [`Gate::Defer`] when no
    /// action may be taken now (no volume has room: the pressure owner reclaims, then the decision is
    /// asked again on fresh observations).
    fn envelope(situation: &Self::Situation, admissible: &mut Vec<Self::Action>) -> Gate<Self::Action>;

    /// An outcome no policy may trade against its objective (continuity after a move; the codec's law),
    /// named when it was broken. `None`: the outcome kept every constraint.
    fn constraint(outcome: &Self::Outcome) -> Option<Violation> {
        let _ = outcome;
        None
    }

    /// Copy `from` into a record's slot, reusing the slot's buffers (a `Vec`'s `clone_from`), so a
    /// steady-state decision allocates nothing.
    fn retain(slot: &mut Self::Situation, from: &Self::Situation);

    /// An empty situation for a record slot not yet used.
    fn empty() -> Self::Situation;
}

/// What the envelope decided before any policy is asked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate<A> {
    /// A policy chooses among the actions the envelope wrote (never none: the gate is `Defer` then).
    Choose,
    /// Only this.
    Forced(A),
    /// Nothing may be done now; the decision is asked again when its observations change.
    Defer(Deferred),
}

/// Why a decision was deferred: which condition, measured against what it needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deferred {
    pub condition: &'static str,
    pub have: f64,
    pub need: f64,
}

/// A constraint an outcome broke: which, what was measured and the limit it passed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Violation {
    pub constraint: &'static str,
    pub measured: f64,
    pub limit: f64,
}
