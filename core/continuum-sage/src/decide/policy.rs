//! What chooses: a policy over one use case, and the baseline every use case gets for free.

use super::record::Record;
use super::use_case::UseCase;

/// Which policy decided, and the revision of its parameters (a learned policy's checkpoint; a fixed
/// one's 0), so a record says exactly what made each choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyId {
    pub name: &'static str,
    pub parameters: u64,
}

/// A policy's answer: the index of its action among the admissible ones (never a copy of it), and
/// the value it expected, when it has a model of value, so a later outcome can grade the prediction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub index: usize,
    pub expected: Option<f64>,
}

/// A way of choosing for one use case. Many may exist for the same use case; they are compared on
/// recorded history (`replay`) and then live, inside the use case's envelope.
pub trait Policy<U: UseCase>: Send {
    fn id(&self) -> PolicyId;

    /// Choose among `admissible` (never empty). An index past its end is refused by the decider, which
    /// takes the first and names the refusal.
    fn choose(&mut self, situation: &U::Situation, admissible: &[U::Action]) -> Choice;

    /// Learn from a closed record, at the moment its outcome became known. A policy that does not
    /// learn ignores it.
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

    fn choose(&mut self, _: &U::Situation, _: &[U::Action]) -> Choice {
        Choice { index: 0, expected: None }
    }
}
