//! What chooses: a policy over one use case, and the baseline every use case gets for free.

use super::record::Record;
use super::use_case::{NonEmpty, UseCase};

/// Which policy decided, and the revision of its parameters (a learned policy's checkpoint; a fixed
/// one's 0), so a record says exactly what made each choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyId {
    pub name: &'static str,
    pub parameters: u64,
}

/// A policy's answer: the action, and the value it expected of it when it has a model of value, so a
/// later outcome can grade the prediction.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice<A> {
    pub action: A,
    pub expected: Option<f64>,
}

/// A way of choosing for one use case. Many may exist for the same use case; they are compared on
/// recorded history (`replay`) and then live, inside the use case's envelope.
pub trait Policy<U: UseCase>: Send {
    fn id(&self) -> PolicyId;

    /// Choose one of `admissible` (the envelope already removed what is not allowed). A choice
    /// outside it is refused by the decider and the envelope's first action is taken instead, named.
    fn choose(&mut self, situation: &U::Situation, admissible: &NonEmpty<U::Action>) -> Choice<U::Action>;

    /// Learn from a closed record: the situation's revisions, the action and its measured outcome.
    /// A policy that does not learn ignores it.
    fn learn(&mut self, record: &Record<U>) {
        let _ = record;
    }
}

/// The first admissible action, always: the baseline every policy is compared against, and the policy
/// of a scaffolded use case until its owner builds one. Generic over every use case, so a new use
/// case runs with no policy edited.
#[derive(Debug, Clone, Copy, Default)]
pub struct Baseline;

impl<U: UseCase> Policy<U> for Baseline {
    fn id(&self) -> PolicyId {
        PolicyId { name: "baseline", parameters: 0 }
    }

    fn choose(&mut self, _: &U::Situation, admissible: &NonEmpty<U::Action>) -> Choice<U::Action> {
        Choice { action: admissible.first().clone(), expected: None }
    }
}
