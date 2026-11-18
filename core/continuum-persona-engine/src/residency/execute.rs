//! [`Execute`]: the decode a tier runs for the mind bound in its execution stream. The caller hands
//! over her tokens, never a sequence id and never the backend itself: the tier decodes on the stream
//! she is bound to, so no step can write another sequence's state or start a move behind the
//! pipeline's back (Codex on #6139). The decode's completion is returned like a transfer's fence, so
//! the engine can hold a handoff until work already submitted to the device has landed.

use super::{Tier, TierRefusal};
use crate::persona::{Epoch, PersonaId};

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

    /// Submit `work` as `persona`'s decode at `epoch`, on the stream she is bound to. Refused when she
    /// is not bound here at that epoch.
    fn submit(&mut self, persona: PersonaId, epoch: Epoch, work: Self::Work) -> Result<Self::Done, TierRefusal>;

    /// Wait until every step submitted so far has finished on the device, so each completion it
    /// returned reports landed. A backend whose submit returns before the device finishes (libllama
    /// synchronizes only on result retrieval) waits here; the engine settles before a handoff.
    fn settle(&mut self) -> Result<(), TierRefusal>;
}
