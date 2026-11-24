//! [`Learns`]: the bound mind learning a reply in place, on the stream she is bound to
//! (IN-PROCESS-LEARNING.md, L2). Like [`Execute`], the caller hands over the reply and never the
//! backend or a sequence id, so a reply cannot train or restore another mind's state.
//!
//! A reply runs in chunks. Between chunks the tier asks the caller whether to go on; a caller with a
//! turn waiting says no, and the reply drains: no further gradient runs, her served tail is restored,
//! and the steps already committed stay. So a turn waits at most one chunk, never a whole reply.

use super::{Execute, TierRefusal};
use crate::persona::MindId;

/// A tier whose bound mind can learn a reply into a shadow.
pub trait Learns: Execute {
    /// The shadow a reply learns into, and how.
    type Shadow;
    /// What pays for a shadow's optimizer while the tier retains it.
    type Lease;
    /// One reply: what she holds before it, its tokens, their labels and the tail to restore.
    type Reply;
    /// The memory owner's admission for one reply's workspace.
    type Admission;
    /// How a reply ended.
    type Learned;

    /// Retain `shadow` with the lease that pays for its optimizer. One already retained hands the
    /// new lease back.
    fn retain(&mut self, shadow: &Self::Shadow, lease: Self::Lease) -> Result<(), Self::Lease>;

    /// Release `shadow` once native code frees its optimizer: its lease comes back to drop. Refused
    /// (and still retained) while native code holds it.
    fn release(&mut self, mind: MindId, shadow: &Self::Shadow) -> Result<Self::Lease, TierRefusal>;

    /// Learn `reply` as `mind`, into `shadow`, asking `go_on` between chunks. Refused before any
    /// native state changed, with the admission handed back, when she is not bound here or the reply
    /// cannot begin.
    fn learn(
        &mut self,
        mind: MindId,
        shadow: &Self::Shadow,
        reply: &Self::Reply,
        admission: Self::Admission,
        go_on: &mut dyn FnMut() -> bool,
    ) -> Result<Self::Learned, (TierRefusal, Self::Admission)>;
}
