//! What a decision is: its situation, its actions, its outcome, its envelope and its constraints.

use std::fmt::Debug;

/// One kind of decision. Its types are its own; nothing here knows what a lane, a drive or a peer is.
pub trait UseCase: 'static {
    /// What a policy sees at a decision: built by the use case's owner from owner-published state.
    type Situation: Clone + Debug + Send;
    /// One action available at a decision.
    type Action: Clone + Debug + PartialEq + Send;
    /// What the executor measured after the action was taken.
    type Outcome: Clone + Debug + Send;

    /// Its name, as records and probes label it.
    const NAME: &'static str;

    /// The never-learned bound on every policy: the actions admissible now, or the one action the
    /// situation forces (a mind past her deadline is stepped next, whatever any policy prefers).
    fn envelope(situation: &Self::Situation) -> Envelope<Self::Action>;

    /// An outcome no policy may trade against its objective (continuity after a move; the codec's law),
    /// named when it was broken. `None`: the outcome kept every constraint.
    fn constraint(outcome: &Self::Outcome) -> Option<Violation> {
        let _ = outcome;
        None
    }
}

/// What the envelope allows at one decision.
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope<A> {
    /// Any of these; a policy chooses among them.
    Admissible(NonEmpty<A>),
    /// Only this.
    Forced(A),
}

/// At least one, by construction: admissible actions (a use case with nothing to do offers its own
/// idle action, so a policy is never handed an empty choice), or a situation's subjects where an empty
/// set means no decision is asked at all.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T> {
    first: T,
    rest: Vec<T>,
}

impl<T> NonEmpty<T> {
    pub fn new(first: T, rest: impl IntoIterator<Item = T>) -> Self {
        Self { first, rest: rest.into_iter().collect() }
    }

    pub fn first(&self) -> &T {
        &self.first
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    pub fn contains(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|candidate| candidate == item)
    }
}

/// A constraint an outcome broke: which, what was measured and the limit it passed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Violation {
    pub constraint: &'static str,
    pub measured: f64,
    pub limit: f64,
}
