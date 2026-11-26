//! What chooses: a policy over one use case, and the baseline every use case gets for free.
//! hot-path: the lane decides at every Core step (THE-LANE-ON-SAGE.md, "The types").

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

impl Choice {
    /// A rule's choice with no model of value: the action it wanted where the envelope offered it,
    /// else the envelope's own first, since an action the envelope did not offer is one it decided
    /// against. Index 0 here is a real action, never a stand-in for an absent one.
    pub fn wanted_or_envelopes_first<A: PartialEq>(admissible: &[A], wanted: &A) -> Self {
        const ENVELOPES_FIRST: usize = 0;
        let index = match admissible.iter().position(|action| action == wanted) {
            Some(offered) => offered,
            None => ENVELOPES_FIRST,
        };
        Self { index, expected: None }
    }
}

/// One parameter's value: typed, never text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Param {
    Real(f64),
    Count(u64),
    Flag(bool),
}

/// Why a parameter or a saved state was refused. The algorithm is unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refused {
    /// No parameter by this name.
    Unknown(&'static str),
    /// The value is not of this parameter's type, or outside its range.
    Value { param: &'static str, value: Param },
    /// A saved state of another algorithm, another version, or truncated.
    State { algorithm: &'static str, bytes: usize },
}

/// A way of choosing for one use case: an algorithm in the OpenCV sense. Many exist for one use case,
/// each created by name from its [`super::Algorithms`] registry, with typed parameters to read and set
/// and a learned state to save and load as bytes. They are compared on recorded history (`replay`)
/// and then live, inside the use case's envelope.
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

    /// The names of its parameters, for whoever tunes or reports it.
    fn params(&self) -> &'static [&'static str] {
        &[]
    }

    fn param(&self, name: &str) -> Option<Param> {
        let _ = name;
        None
    }

    fn set_param(&mut self, name: &'static str, value: Param) -> Result<(), Refused> {
        let _ = value;
        Err(Refused::Unknown(name))
    }

    /// Its learned state as bytes (appended to `into`), so a restart resumes what it learned.
    fn save(&self, into: &mut Vec<u8>) {
        let _ = into;
    }

    fn load(&mut self, from: &[u8]) -> Result<(), Refused> {
        if from.is_empty() { Ok(()) } else { Err(Refused::State { algorithm: self.id().name, bytes: from.len() }) }
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
