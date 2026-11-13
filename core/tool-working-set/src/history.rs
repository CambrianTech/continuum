//! What a policy may know when it chooses a head: her past, never her future.

use uuid::Uuid;

use crate::epoch::Epoch;
use crate::vocab::{ToolId, Vocabulary};

pub use continuum_base::time::EpochMs;

/// The activity (room) an act happened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivityId(pub Uuid);

/// One tool call she made.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub at: EpochMs,
    pub tool: ToolId,
    /// The activity she was acting in, when the record names one.
    pub activity: Option<ActivityId>,
    /// Her act as she recorded it (the call and her reason), the text a relevance model reads.
    pub text: String,
}

/// One mind's past at an epoch turnover: everything a policy may read.
///
/// Built so it cannot hold the future: `calls` and `epochs` end before `at`.
#[derive(Debug, Clone, Copy)]
pub struct History<'a> {
    pub vocab: &'a Vocabulary,
    /// Her calls before `at`, oldest first.
    pub calls: &'a [Call],
    /// Her epochs that ended before `at`, oldest first; each indexes `calls`.
    pub epochs: &'a [Epoch],
    /// The turnover being decided.
    pub at: EpochMs,
    /// The activity she turns over in.
    pub activity: Option<ActivityId>,
    /// The other minds' chance of calling each tool in an epoch (indexed by [`ToolId::index`]),
    /// when a society is known.
    pub population: Option<&'a [f32]>,
}

impl History<'_> {
    /// Her most recent call, the state a transition model runs forward from.
    pub fn last_call(&self) -> Option<&Call> {
        self.calls.last()
    }

    /// The text of her `count` most recent acts, newest last.
    pub fn recent_text(&self, count: usize) -> String {
        let from = self.calls.len().saturating_sub(count);
        self.calls[from..].iter().map(|call| call.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}
