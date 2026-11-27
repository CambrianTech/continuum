//! What the learner owes, as state (UNIFIED-PAGING.md "Time divides as well as space: learning is PWM"):
//! every seat with a lesson owed, latest wins, on one key. The lane reads it to set learning's duty
//! (its `Thread::Learner(seat)` threads), never the journal. A seat in the map is Ready; a seat that
//! leaves it is Done. State and never events, so a coalesced say cannot leave the lane holding a
//! Ready it never sees retired.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::time::Instant;

use continuum_sage::dataflow::TopicName;

use super::{PersonaId, SeatId};

/// The node bus topic the trainer says what it owes on.
pub const LEARNING_OWED: TopicName = TopicName("learning/owed");

/// Every seat with a lesson owed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owed(pub BTreeMap<SeatId, LearnerOwed>);

/// What one seat owes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnerOwed {
    pub mind: PersonaId,
    /// Examples owed past her newest committed generation and the seat's own progress.
    pub examples: usize,
    /// Her next lesson's rows as the engine measured them (its `Loaded`), `None` until measured: the
    /// trainer never counts tokens, which have one owner.
    pub next: Option<Rows>,
    /// When this seat was created. The work was owed before that, but the trainer has no earlier time to
    /// report: its journal keeps arrival order, not arrival time (Fable on #6287). It is a lower bound on how
    /// long the work has waited, and the lane's aging reads it (learning is a share class, whose guarantee is
    /// its queue in check).
    pub since: Instant,
}

/// One lesson's rows, split because they are different work at different rates: the context the
/// walk decodes before her reply, and the reply it trains forward and back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rows {
    pub context: u32,
    pub reply: NonZeroU32,
}
