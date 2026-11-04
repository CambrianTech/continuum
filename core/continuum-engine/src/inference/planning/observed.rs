//! One observation of a size, and whether what measured it censored it: `Exact` is a
//! measurement, `AtLeast` is a lower bound (the measurement hit a limit before it ended). The one
//! type for both, below every reader, so no reader can take a bound for a size (Aris on the M5,
//! 2026-11-04: her need was measured only from prompts her grant had bounded, so it could never
//! exceed the grant). Her working set records it (`working_set::NeedSample`), her lane
//! requirement carries it (`window_allocator::Measure::Held`), and the grant reads it
//! (`capacity::window_grant::need_within`).

use serde::{Deserialize, Serialize};

/// A size as observed. Persisted untagged: an exact value is the bare value, so a record written
/// before bounds existed reads unchanged; a bound is `{"at_least": v}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Observed<T> {
    /// A measurement.
    Exact(T),
    /// A lower bound: the true size is at least this, never known to be this.
    AtLeast { at_least: T },
}

impl<T: Default> Default for Observed<T> {
    /// An exact default value: in a ring of samples, its empty slot.
    fn default() -> Self {
        Self::Exact(T::default())
    }
}

impl<T> Observed<T> {
    /// The value observed: the size, or the bound.
    pub fn value(self) -> T {
        match self {
            Self::Exact(value) | Self::AtLeast { at_least: value } => value,
        }
    }

    /// Whether this is a lower bound, never a size.
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::AtLeast { .. })
    }

    /// The same observation of `f(value)`: a bound stays a bound.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Observed<U> {
        match self {
            Self::Exact(value) => Observed::Exact(f(value)),
            Self::AtLeast { at_least } => Observed::AtLeast { at_least: f(at_least) },
        }
    }
}

impl<T> Observed<Option<T>> {
    /// `None` when the value is absent, else the observation of the value.
    pub fn transpose(self) -> Option<Observed<T>> {
        match self {
            Self::Exact(value) => value.map(Observed::Exact),
            Self::AtLeast { at_least } => at_least.map(|at_least| Observed::AtLeast { at_least }),
        }
    }
}
