//! What a value is worth: recorded or not, joined how, measured when and how well
//! (THE-PROVING-GROUND 4a, P2). A zero is a measurement, so "not recorded" is its own variant,
//! never a default number.

use serde::{Deserialize, Serialize};

use crate::closed::Slug;

/// A field the capture either recorded or did not. `Unavailable` is never read as zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Known<T> {
    Recorded(T),
    Unavailable,
}

/// A field joined from probes by persona and time rather than carried on the request
/// (the sent reasoning budget, the allocation receipt). An ambiguous join holds no value: several
/// concurrent requests could match, and guessing one would attribute another act's budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "join", rename_all = "snake_case", deny_unknown_fields)]
pub enum Joined<T> {
    /// One request matched, or the capture bound the receipt by request id.
    Exact { value: T },
    /// More than one request in flight could match; how many.
    Ambiguous { candidates: u32 },
    /// Nothing matched.
    Unresolved {},
}

impl<T> Joined<T> {
    /// The value only when the join is exact; an ambiguous or unresolved join has none to give.
    pub fn exact(&self) -> Option<&T> {
        match self {
            Self::Exact { value } => Some(value),
            Self::Ambiguous { .. } | Self::Unresolved {} => None,
        }
    }
}

/// Where a measurement came from: a node state file, an engine log, a bench run, a probe class.
/// A slug and a reference, never a description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The kind of source: `state/prefill-rate`, `engine-log`, `bench`, `probe/delib.window.flex`.
    pub kind: Slug,
    /// The node it was measured on.
    pub node: Slug,
}

/// A value measured on a node, or a domain never measured there (a declared shape for a machine we
/// do not have, P2). An unmeasured domain is never filled with another node's number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Measured<T> {
    Measured {
        value: T,
        /// Unix milliseconds of the measurement (the newest sample it summarizes).
        at_ms: u64,
        source: Source,
        /// How many samples it summarizes.
        samples: u64,
        error: Spread<T>,
    },
    Unmeasured {},
}

/// A measurement's published error. A prediction may decide alone only when its before/after
/// difference exceeds this on that metric for that node (THE-PROVING-GROUND 10.2), so an error not
/// yet measured is said, never assumed small.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Spread<T> {
    /// One standard deviation of the samples, in the value's unit.
    StdDev(T),
    NotYetMeasured,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an unrecorded field written as 0, which a replay would read as "measured
    // zero prefill" and score as the fastest act there ever was.
    #[test]
    fn unavailable_is_not_zero_on_the_wire() {
        let none: Known<u64> = Known::Unavailable;
        let zero: Known<u64> = Known::Recorded(0);
        let (a, b) = (serde_json::to_string(&none).expect("ser"), serde_json::to_string(&zero).expect("ser"));
        assert_ne!(a, b);
        assert_eq!(serde_json::from_str::<Known<u64>>(&a).expect("de"), Known::Unavailable);
        assert!(serde_json::from_str::<Known<u64>>("0").is_err(), "a bare number is not a Known");
    }

    // what this catches: an ambiguous join yielding a value, which would attribute a concurrent
    // act's reasoning budget to this one (4a: "never resolved by guessing").
    #[test]
    fn only_an_exact_join_has_a_value() {
        assert_eq!(Joined::Exact { value: 1_024_u32 }.exact(), Some(&1_024));
        assert_eq!(Joined::<u32>::Ambiguous { candidates: 2 }.exact(), None);
        assert_eq!(Joined::<u32>::Unresolved {}.exact(), None);
    }

    // what this catches: words beside a variant that carries nothing. serde ignores extra fields
    // beside an internally tagged UNIT variant even under deny_unknown_fields, which is why every
    // such variant here is an empty struct (`Unresolved {}`) or adjacently tagged.
    #[test]
    fn nothing_rides_beside_an_empty_variant() {
        let w = "\"the words\"";
        assert!(serde_json::from_str::<Joined<u32>>(&format!(r#"{{"join":"unresolved","note":{w}}}"#)).is_err());
        assert!(serde_json::from_str::<Measured<u64>>(&format!(r#"{{"state":"unmeasured","note":{w}}}"#)).is_err());
        assert!(serde_json::from_str::<Known<u32>>(&format!(r#"{{"state":"unavailable","value":{w}}}"#)).is_err());
        assert!(serde_json::from_str::<Spread<f64>>(&format!(r#"{{"state":"not_yet_measured","note":{w}}}"#)).is_err());
        assert!(serde_json::from_str::<Joined<u32>>(r#"{"join":"unresolved"}"#).is_ok(), "the clean form parses");
    }
}
