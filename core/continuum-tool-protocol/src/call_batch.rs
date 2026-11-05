//! A batch of tool calls carrying each call's disposition: committed (it may run) or held (it
//! never runs, and says why). The disposition is decided once, by the one classifier
//! (`ToolCall::ending`), from where the batch came from, and it travels with the calls to every
//! executor and every dispatch path: a [`CommittedCall`] is constructible only here, so code that
//! runs a call can only run one the classifier committed (ONE-ACT 6b; Codex on #5790).
//!
//! The failure this ends: the Cut status existed only where `contribute.rs` recomputed it, while
//! the act seam's background dispatch, its effect marks, and her executor saw a bare call whose
//! JSON happened to be complete, and would have run it.

use serde::{Deserialize, Serialize};

use crate::call_ending::{CallEnding, GenerationEnd};
use crate::ToolCall;

/// Where a batch of calls came from, which decides whether they may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "end", rename_all = "snake_case")]
pub enum CallOrigin {
    /// A model's generation, and why the engine says it stopped: only a reported finish commits.
    Generated(GenerationEnd),
    /// The substrate composed the calls itself (her workspace's setup, an eval's observation):
    /// no generation, so nothing could have cut them.
    Substrate,
}

/// A call the classifier committed: the only thing an executor dispatches.
#[derive(Debug, Clone, PartialEq)]
pub struct CommittedCall(ToolCall);

impl CommittedCall {
    pub fn call(&self) -> &ToolCall {
        &self.0
    }

    /// The same committed call with its input augmented by the substrate (a page budget her act
    /// layer fills): the witness stays, because the call she committed is unchanged in kind.
    pub fn with_input(self, input: serde_json::Value) -> Self {
        CommittedCall(ToolCall { input, ..self.0 })
    }
}

/// A call that never runs, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldCall {
    call: ToolCall,
    ending: CallEnding,
}

impl HeldCall {
    pub fn call(&self) -> &ToolCall {
        &self.call
    }

    /// Why it is held: never [`CallEnding::Committed`].
    pub fn ending(&self) -> CallEnding {
        self.ending
    }
}

/// One call of a batch, with its disposition.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposed {
    Committed(CommittedCall),
    Held(HeldCall),
}

impl Disposed {
    pub fn call(&self) -> &ToolCall {
        match self {
            Disposed::Committed(committed) => committed.call(),
            Disposed::Held(held) => held.call(),
        }
    }
}

/// A batch of calls, each disposed by the one classifier.
#[derive(Debug, Clone, PartialEq)]
pub struct CallBatch(Vec<Disposed>);

impl CallBatch {
    /// Dispose every call by where the batch came from.
    pub fn classify(calls: impl IntoIterator<Item = ToolCall>, origin: CallOrigin) -> Self {
        CallBatch(
            calls
                .into_iter()
                .map(|call| match origin {
                    CallOrigin::Substrate => Disposed::Committed(CommittedCall(call)),
                    CallOrigin::Generated(end) => match call.ending(end) {
                        CallEnding::Committed => Disposed::Committed(CommittedCall(call)),
                        ending => Disposed::Held(HeldCall { call, ending }),
                    },
                })
                .collect(),
        )
    }

    /// Every call, committed or held, in her order.
    pub fn entries(&self) -> &[Disposed] {
        &self.0
    }

    pub fn committed(&self) -> impl Iterator<Item = &CommittedCall> {
        self.0.iter().filter_map(|entry| match entry {
            Disposed::Committed(committed) => Some(committed),
            Disposed::Held(_) => None,
        })
    }

    pub fn held(&self) -> impl Iterator<Item = &HeldCall> {
        self.0.iter().filter_map(|entry| match entry {
            Disposed::Held(held) => Some(held),
            Disposed::Committed(_) => None,
        })
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The batch without the committed calls `taken` claims (the act seam's background
    /// dispatch), in order; held calls always stay.
    pub fn without_committed(self, mut taken: impl FnMut(&CommittedCall) -> bool) -> Self {
        CallBatch(self.0.into_iter().filter(|entry| !matches!(entry, Disposed::Committed(c) if taken(c))).collect())
    }

    /// Each committed call's input augmented by the substrate; held calls untouched.
    pub fn map_committed(self, mut f: impl FnMut(CommittedCall) -> CommittedCall) -> Self {
        CallBatch(
            self.0
                .into_iter()
                .map(|entry| match entry {
                    Disposed::Committed(committed) => Disposed::Committed(f(committed)),
                    held => held,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(id: &str) -> ToolCall {
        ToolCall { id: id.into(), name: "code/write".into(), input: serde_json::json!({"file_path": "a.rs", "content": "x"}) }
    }

    // what this catches: a batch whose calls are complete JSON committed although the engine never
    // reported its generation finished (the limit cut it, it errored, or the backend said
    // nothing). Only a reported finish, or the substrate's own calls, commit.
    #[test]
    fn only_a_reported_finish_commits_valid_json() {
        for end in [GenerationEnd::OutputLimit, GenerationEnd::Failed, GenerationEnd::Unreported] {
            let batch = CallBatch::classify([parsed("a"), parsed("b")], CallOrigin::Generated(end));
            assert_eq!(batch.committed().count(), 0, "{end:?}");
            assert_eq!(batch.held().count(), 2, "{end:?}");
        }
        assert_eq!(CallBatch::classify([parsed("a")], CallOrigin::Generated(GenerationEnd::Finished)).committed().count(), 1);
        assert_eq!(CallBatch::classify([parsed("a")], CallOrigin::Substrate).committed().count(), 1);
    }
}
