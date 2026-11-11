//! What a load returns. Every caller matches it (or, for a [`crate::Rebuildable`] kind, lets the
//! trunk read a non-match as empty); nothing on a state path is `unwrap_or_default`.

use std::time::Duration;

/// The verdict on one saved value.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded<T> {
    /// Nothing was ever saved under this key.
    Absent,
    /// The saved value, valid under the live contract.
    Matched(T),
    /// A file of an older format the kind's [`crate::SavedState::upgrade`] read into the current
    /// value; the next save writes it at the current format.
    Upgraded { value: T, from: u32 },
    /// Saved under another contract: `what` is the first contract field that differs.
    Diverged { what: String },
    /// Older than the kind's measured retention age.
    Stale { age: Duration, max_age: Duration },
    /// Written by a newer format than this build knows.
    Newer { found: u32, known: u32 },
    /// Written by an older format; `found` 0 is a file from before the envelope.
    Older { found: u32, known: u32 },
    /// The bytes are there and cannot be trusted.
    Corrupt { cause: CorruptCause },
    /// No directory to look in (no continuum home): nothing can be saved or found.
    Unrooted,
}

/// Why a present file cannot be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorruptCause {
    /// It exists and could not be read.
    Unreadable(std::io::ErrorKind),
    /// It is not JSON (a torn or foreign write).
    NotJson,
    /// It is an envelope of another kind.
    OtherKind,
    /// Its key is not the one asked for (a file copied or renamed by hand).
    OtherKey,
    /// The envelope is whole but its contract or value does not decode at this format.
    Undecodable,
}

impl<T> Loaded<T> {
    /// The value when matched; otherwise the verdict itself, typed and without a value, for a
    /// caller that must refuse rather than start empty (it matches the variants it can act on).
    pub fn matched(self) -> Result<T, Loaded<std::convert::Infallible>> {
        match self {
            Loaded::Matched(value) | Loaded::Upgraded { value, .. } => Ok(value),
            Loaded::Absent => Err(Loaded::Absent),
            Loaded::Diverged { what } => Err(Loaded::Diverged { what }),
            Loaded::Stale { age, max_age } => Err(Loaded::Stale { age, max_age }),
            Loaded::Newer { found, known } => Err(Loaded::Newer { found, known }),
            Loaded::Older { found, known } => Err(Loaded::Older { found, known }),
            Loaded::Corrupt { cause } => Err(Loaded::Corrupt { cause }),
            Loaded::Unrooted => Err(Loaded::Unrooted),
        }
    }

    /// The probe class this verdict reports under: one per outcome.
    pub const fn probe_class(&self) -> &'static str {
        match self {
            Loaded::Absent => "state.load.absent",
            Loaded::Matched(_) => "state.load.matched",
            Loaded::Upgraded { .. } => "state.load.upgraded",
            Loaded::Diverged { .. } => "state.load.diverged",
            Loaded::Stale { .. } => "state.load.stale",
            Loaded::Newer { .. } => "state.load.newer",
            Loaded::Older { .. } => "state.load.older",
            Loaded::Corrupt { .. } => "state.load.corrupt",
            Loaded::Unrooted => "state.load.unrooted",
        }
    }

    /// What was found, for a caller that reports a non-match: never matched on.
    pub fn describe(&self) -> String {
        match self {
            Loaded::Absent => "nothing saved".into(),
            Loaded::Matched(_) => "matched".into(),
            Loaded::Upgraded { from, .. } => format!("upgraded from format {from}"),
            Loaded::Diverged { what } => format!("saved under another {what}"),
            Loaded::Stale { age, max_age } => format!("{}s old, valid for {}s", age.as_secs(), max_age.as_secs()),
            Loaded::Newer { found, known } => format!("format {found}, newer than this build's {known}"),
            Loaded::Older { found, known } => format!("format {found}, older than this build's {known}"),
            Loaded::Corrupt { cause } => format!("corrupt ({cause:?})"),
            Loaded::Unrooted => "no continuum home".into(),
        }
    }
}

impl<T> std::fmt::Display for Loaded<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}
