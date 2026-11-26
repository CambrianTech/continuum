//! [`Learns`]: the bound mind learning a reply in place, on the stream she is bound to
//! (IN-PROCESS-LEARNING.md, L2). Like [`Execute`], the caller hands over the reply and never the
//! backend or a sequence id, so a reply cannot train or restore another mind's state.
//!
//! A reply runs in chunks. Between chunks the tier asks the caller whether to go on; a caller with a
//! turn waiting says no, and the reply drains: no further gradient runs, her served tail is restored,
//! and the steps already committed stay. So a turn waits at most one chunk, never a whole reply.

use std::num::NonZeroU32;

use super::{Execute, TierRefusal};
use crate::persona::MindId;

/// A lesson's geometry as a seat is measured for it: the reply rows it trains, and the whole lesson
/// (her context and the reply) its attention reads. A seat sized for one holds a lesson no longer
/// on either; the workspace grows with both (attention priced at the 262,144-token served window
/// quoted 78 to 105 GB for lessons on the M5's 35B, 2026-11-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LessonRows {
    reply: NonZeroU32,
    whole: NonZeroU32,
}

impl LessonRows {
    /// A lesson of `whole` rows whose last `reply` are trained; none when the reply is longer than
    /// the lesson holding it.
    pub fn new(reply: NonZeroU32, whole: NonZeroU32) -> Option<Self> {
        (reply <= whole).then_some(Self { reply, whole })
    }

    /// The rows it trains.
    pub fn reply(self) -> NonZeroU32 {
        self.reply
    }

    /// The rows its attention reads: her context and the reply.
    pub fn whole(self) -> NonZeroU32 {
        self.whole
    }

    /// Whether a seat sized for `self` holds `lesson`.
    pub fn holds(self, lesson: Self) -> bool {
        lesson.reply <= self.reply && lesson.whole <= self.whole
    }
}

/// A tier whose bound mind can learn a reply into a shadow.
pub trait Learns: Execute {
    /// The shadow a reply learns into, and how.
    type Shadow;
    /// What pays for a shadow's optimizer while the tier retains it.
    type Lease;
    /// One reply, naming the mind it is hers: what she holds before it, its tokens, their labels and
    /// the tail to restore.
    type Reply;
    /// The memory owner's admission for one reply's workspace.
    type Admission;
    /// How a reply ended.
    type Learned;

    /// One row of memory the backend measured.
    type Row;

    /// Attach `shadow` alone, at full scale: native code measures and trains only an attached shadow.
    /// Until [`Self::detach`] confirms it off, the shadow must not be dropped.
    fn attach(&mut self, mind: MindId, shadow: &Self::Shadow) -> Result<(), TierRefusal>;

    /// Take every adapter off the context; her next decode applies her own genes. Until this
    /// succeeds, an attached shadow is still referenced by the context.
    fn detach(&mut self, mind: MindId) -> Result<(), TierRefusal>;

    /// What an attached `shadow`'s optimizer, and a lesson of up to `rows`, would hold, allocating
    /// nothing: `(optimizer, workspace)`.
    fn measure(&mut self, mind: MindId, shadow: &Self::Shadow, rows: LessonRows) -> Result<(Vec<Self::Row>, Vec<Self::Row>), TierRefusal>;

    /// Save `shadow` as a generation in `into` (its weights and its optimizer state, bound to
    /// `binding`): the optimizer steps it holds.
    fn save(&mut self, mind: MindId, shadow: &Self::Shadow, into: &std::path::Path, binding: &str) -> Result<u64, TierRefusal>;

    /// Retain `shadow` with the lease that pays for its optimizer. One already retained hands the
    /// new lease back.
    fn retain(&mut self, shadow: &Self::Shadow, lease: Self::Lease) -> Result<(), Self::Lease>;

    /// Release `shadow` once native code frees its optimizer: its lease comes back to drop. Refused
    /// (and still retained) while native code holds it.
    fn release(&mut self, mind: MindId, shadow: &Self::Shadow) -> Result<Self::Lease, TierRefusal>;

    /// Learn `reply` into `shadow` as the mind it names, asking `go_on` between chunks. Refused before any
    /// native state changed, with the admission handed back, when she is not bound here or the reply
    /// cannot begin.
    fn learn(&mut self, shadow: &Self::Shadow, reply: &Self::Reply, admission: Self::Admission, go_on: &mut dyn FnMut() -> bool) -> Result<Self::Learned, (TierRefusal, Self::Admission)>;
}
