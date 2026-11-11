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
    /// A lower bound over an irreducible floor (`floor <= at_least`): the bound may be clamped to a
    /// limit, the floor never is. Built only by residency (`working_set::resident_need`), never
    /// recorded in a ring; listed after `AtLeast` so a persisted bound reads as one (Codex on #5931:
    /// a floor between the ceiling and the bound was clamped below what her turn cannot go below).
    Floored { at_least: T, floor: T },
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
            Self::Exact(value) | Self::AtLeast { at_least: value } | Self::Floored { at_least: value, .. } => value,
        }
    }

    /// Whether this is a lower bound, never a size.
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::AtLeast { .. } | Self::Floored { .. })
    }

    /// The same observation of `f(value)`: a bound stays a bound.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Observed<U> {
        match self {
            Self::Exact(value) => Observed::Exact(f(value)),
            Self::AtLeast { at_least } => Observed::AtLeast { at_least: f(at_least) },
            Self::Floored { at_least, floor } => {
                let floor = f(floor);
                Observed::Floored { at_least: f(at_least), floor }
            }
        }
    }

    /// The same observation of `f(value)`, or `None` when `f` refuses any part (a zero is no size).
    pub fn try_map<U>(self, mut f: impl FnMut(T) -> Option<U>) -> Option<Observed<U>> {
        Some(match self {
            Self::Exact(value) => Observed::Exact(f(value)?),
            Self::AtLeast { at_least } => Observed::AtLeast { at_least: f(at_least)? },
            Self::Floored { at_least, floor } => {
                let floor = f(floor)?;
                Observed::Floored { at_least: f(at_least)?, floor }
            }
        })
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
            // The bound is clamped, never below her floor: a floor past the limit stands, as an
            // exact need does, and waits for room.
            Self::Floored { at_least, floor } => Self::AtLeast { at_least }.within(limit).max(floor),
        }
    }

    /// This need over an irreducible `floor` (a size her turn cannot be sent below): an exact need is
    /// raised to it, and a bound keeps it beside itself, so `within` may collapse the bound into what
    /// exists and never the floor. A floor past the bound is the need, exact. The ONE place a floor
    /// joins a need.
    pub fn over(self, floor: Option<Tokens>) -> Self {
        let Some(floor) = floor else { return self };
        match self {
            Self::Exact(need) => Self::Exact(need.max(floor)),
            Self::AtLeast { at_least } | Self::Floored { at_least, .. } if floor >= at_least => Self::Exact(floor),
            Self::AtLeast { at_least } => Self::Floored { at_least, floor },
            Self::Floored { at_least, floor: held } => Self::Floored { at_least, floor: held.max(floor) },
        }
    }
}
