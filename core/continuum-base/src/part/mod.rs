//! The root of every component's lineage (object model §3.1).
//!
//! A part is a thing that is someone, has a lifetime, and may be made of parts. Each generation
//! below adds only its difference: a model backend adds loading and release, a speech backend adds
//! speech, a leaf adds its device. What every part shares is written here once.

mod completion;
mod lifecycle;
mod outcome;
mod parts;

use std::num::NonZeroU64;

pub use completion::{Answer, Completion, OperationId, Revision, Snapshot};
pub use lifecycle::{Lifecycle, Verb};
pub use outcome::{Outcome, Settling};
pub use parts::{Executor, Parts};

/// Who a part is: the name callers address it by and what it is, in a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub name: &'static str,
    pub description: &'static str,
}

/// A component with an identity. Every long-lived component descends from it, directly or through
/// the generations between.
pub trait Part {
    fn identity(&self) -> Identity;
}

/// Which incarnation of a key a part is. Its owner issues it; a replacement gets a new one, so the
/// part it replaced is still told apart from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(NonZeroU64);

impl Generation {
    /// The generation for an owner's counter value, counted from 0.
    const fn from_counter(value: u64) -> Self {
        Self(NonZeroU64::MIN.saturating_add(value))
    }
}
