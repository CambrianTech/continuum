//! How many steps a turn may spend: every generation it makes (acts, spoken plans, retried
//! faulted generations). A type, so "no limit" is a variant and never a number standing for one
//! (`usize::MAX` was the live turn's sentinel, and a probe once read its remainder as
//! 18446744073709551614).

/// A turn's step budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnBudget {
    /// A live turn: she acts until she settles.
    Unbounded,
    /// A caller-bounded turn (an eval, a solve): at most this many steps.
    Steps(usize),
}

impl TurnBudget {
    /// Whether a turn that has spent `spent` steps may take another.
    pub fn allows(self, spent: usize) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Steps(n) => spent < n,
        }
    }

    /// Steps left after `spent`; `None` when the turn is unbounded.
    pub fn remaining(self, spent: usize) -> Option<usize> {
        match self {
            Self::Unbounded => None,
            Self::Steps(n) => Some(n.saturating_sub(spent)),
        }
    }

    /// The acts a workspace-deliverable turn may spend before its first write (the #390
    /// discovery gate): three quarters of a bounded budget, at least one; an unbounded turn has
    /// no discovery gate.
    pub fn discovery_steps(self) -> Option<usize> {
        match self {
            Self::Unbounded => None,
            Self::Steps(n) => Some((n / 4 * 3).max(1)),
        }
    }
}

impl From<usize> for TurnBudget {
    fn from(steps: usize) -> Self {
        Self::Steps(steps)
    }
}
