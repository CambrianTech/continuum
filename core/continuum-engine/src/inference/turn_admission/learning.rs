//! What a learning step needs from its admission (card 23069702 part B): the slot her page was
//! placed in, and the custody mark after its `POST /learn` is answered.

use super::{Dispatch, TurnAdmission};

impl TurnAdmission {
    /// The slot her page was placed in, once admission has pinned it.
    pub(crate) fn placed_slot(&self) -> Option<u32> {
        self.uncommitted.as_ref().map(|(_, _, slot)| *slot)
    }

    /// The engine confirmed the exact served tail is back in the slot after a learning step: the
    /// fork's `/learn` truncates KV only when the step did not begin or did not finish
    /// (server-context-learn.inc, fork 316ad8b92, exit at 491-511), and `success` requires it
    /// began and finished (515-541). The slot holds what paging left, as if nothing was sent.
    pub(crate) fn served_tail_restored(&mut self) {
        self.dispatch = Dispatch::Unsent;
    }
}
