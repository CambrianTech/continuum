//! One observation of a size, and whether what measured it censored it: `Exact` is a
//! measurement, `AtLeast` is a lower bound (the measurement hit a limit before it ended). The one
//! type for both, below every reader, so no reader can take a bound for a size (Aris on the M5,
//! 2026-11-04: her need was measured only from prompts her grant had bounded, so it could never
//! exceed the grant). Her working set records it (`working_set::NeedSample`), her lane
//! requirement carries it (`window_allocator::Measure::Held`), and every limit reads it through
//! one rule ([`Observed::within`]): the grant, the lane plan and the grid.

use serde::{Deserialize, Serialize};

use super::Tokens;

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

impl Observed<Tokens> {
    /// This need read against `limit`, the most a reader can give her (one sequence of the pool,
    /// one lane of a plan, the widest window on the grid). An exact need is her need as measured:
    /// above the limit she waits for room or is not served, never trimmed. A bound above the limit
    /// says only that she needs more than the limit holds, so she is given all of it: read raw, a
    /// bound dropped a mind holding nothing from the grant, she never took a turn, and the bound
    /// never aged out of her ring (Cormac on #5773: starvation forever on fixed slots or a small
    /// pool). A zero limit holds nothing to clamp to: the bound stands, as an exact need would.
    /// The ONE reading of a need against a limit: `Measure` exposes no raw need to compare.
    pub fn within(self, limit: u32) -> Tokens {
        match self {
            Self::Exact(need) => need,
            Self::AtLeast { at_least } => Tokens::new(limit).map_or(at_least, |limit| at_least.min(limit)), // map_or: a zero limit holds nothing to clamp to
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
