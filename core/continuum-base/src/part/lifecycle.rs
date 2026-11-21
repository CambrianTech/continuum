//! The four lifecycle verbs every part answers (object model §3.1). A part writes one method, `act`;
//! the walk over its parts is written once, in [`Parts`](super::Parts).

use async_trait::async_trait;

use super::{Outcome, Part};

/// A lifecycle verb, as a value, so one walk serves all four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verb {
    /// Stage for use.
    Prepare,
    /// Quiesce, keeping what an equivalent resume needs, and custody of its bytes until released.
    Suspend,
    /// Bind what suspend kept.
    Resume,
    /// Release everything, safely.
    Shutdown,
}

impl Verb {
    /// Whether a part's own parts act before it: a part stops after what depends on it, and starts
    /// before it.
    #[must_use]
    pub const fn parts_first(self) -> bool {
        matches!(self, Self::Suspend | Self::Shutdown)
    }
}

/// A part with a lifetime. `act` takes `&self`: parts are shared by handle while they run, and each
/// takes its own exclusive access for each state change. Repeating a verb is idempotent.
#[async_trait]
pub trait Lifecycle: Part + Send + Sync {
    /// Why a verb failed. Cloned into every observer's snapshot.
    type Error: Clone + Send + Sync + 'static;

    /// Answer `verb`.
    async fn act(&self, verb: Verb) -> Outcome<Self::Error>;
}
