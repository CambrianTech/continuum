//! [`Execute`]: the decode a tier runs for the mind bound in its execution stream. The caller hands
//! over her tokens, never a sequence id and never the backend itself: the tier decodes on the stream
//! she is bound to, so no step can write another sequence's state or start a move behind the
//! pipeline's back (Codex on #6139). The decode's completion is returned like a transfer's fence, so
//! the engine can hold a handoff until work already submitted to the device has landed.

use super::{Tier, TierRefusal};
use crate::persona::MindId;

/// One submitted decode's completion, as the tier reports it.
pub trait Completion {
    /// Whether the device has finished the work.
    fn complete(&self) -> bool;
}

/// A tier that also runs the bound mind's decode.
pub trait Execute: Tier {
    /// One step of her work: her tokens and their positions, never where she is bound.
    type Work;
    /// The completion of one submitted step.
    type Done: Completion;

    /// Submit `work` as `mind`'s decode, on the stream she is bound to. Refused when she
    /// is not bound here at that epoch.
    fn submit(&mut self, mind: MindId, work: Self::Work) -> Result<Self::Done, TierRefusal>;

    /// Wait until every step submitted so far has finished on the device, so each completion it
    /// returned reports landed. A backend whose submit returns before the device finishes (libllama
    /// synchronizes only on result retrieval) waits here; the engine settles before a handoff.
    fn settle(&mut self) -> Result<(), TierRefusal>;

    /// Her state at a stable boundary, kept beside her and reused act after act
    /// (CONTINUUM-ENGINE 4.15): a request that holds her prompt up to it prefills only what follows.
    type Checkpoint;

    /// Take `persona`'s checkpoint at her current position without moving her; her last position it
    /// holds through. Refused when she is not bound here at `epoch`; a refusal leaves no checkpoint.
    fn checkpoint(&mut self, mind: MindId, into: &mut Self::Checkpoint) -> Result<u32, TierRefusal>;

    /// Put `persona` back at `from`'s position: what she holds past it is cut. Refused (and she is
    /// left as she was) when she is not bound here at `epoch` or the checkpoint is ahead of her.
    fn rewind(&mut self, mind: MindId, from: &Self::Checkpoint) -> Result<(), TierRefusal>;

    /// Empty her sequence: she holds nothing, attention or recurrent, and her next step starts at 0.
    /// Confirmed against the engine's own memory, never assumed.
    fn clear(&mut self, mind: MindId) -> Result<(), TierRefusal>;
}
