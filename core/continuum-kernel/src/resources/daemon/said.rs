//! What the authority says on the node's bus: every governed kind's capacity and what is free,
//! derived from the board it publishes, in the order the board changed (card 9d856843). A
//! component waiting for memory watches it instead of sampling the board on its own wakes.

use continuum_sage::component::{Bus, Key, Source, Taken};
use continuum_sage::dataflow::TopicName;

use super::super::lease::ResourceKind;
use super::super::ledger::LeaseBoard;

/// The authority's projection: [`Free`], all kinds in one value.
pub const FREE: TopicName = TopicName("resources/free");

/// Every governed kind's capacity and what is free now. All kinds travel together, so a change
/// to one never coalesces away another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Free(pub Vec<KindFree>);

/// One kind on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindFree {
    pub kind: ResourceKind,
    pub capacity: u64,
    pub available: u64,
    /// The most one wait could be granted with everything demotable demoted: capacity less what our
    /// consumers can never demote while loaded (weights) and what others hold now. What a lesson is sized
    /// within, never `capacity` (Cormac, 2026-11-26: Fit sized a 29.5 GB ask on a card serving held 30.7 GB of).
    pub ceiling: u64,
}

impl Free {
    /// The projection of `board`.
    pub fn of(board: &LeaseBoard) -> Self {
        Self(
            board
                .kinds
                .iter()
                .map(|ledger| KindFree {
                    kind: ledger.kind,
                    capacity: ledger.capacity_bytes,
                    available: ledger.available_bytes,
                    ceiling: ledger.ceiling_bytes,
                })
                .collect(),
        )
    }

    /// `kind`, or `None` for a kind the authority does not govern.
    pub fn kind(&self, kind: ResourceKind) -> Option<KindFree> {
        self.0.iter().copied().find(|held| held.kind == kind)
    }
}

/// The authority's one writer of [`FREE`] and what it said last.
pub(super) struct Said {
    source: Source<Free>,
    last: Free,
}

impl Said {
    /// Claim [`FREE`] on `bus` and say `board`'s projection at once, so a watcher that attaches at
    /// any time reads a value. A second writer is refused.
    pub(super) fn on(bus: &Bus, board: &LeaseBoard) -> Result<Self, Taken> {
        let mut source = Source::on(bus, FREE, Key::ONE, "resource-authority")?;
        let last = Free::of(board);
        source.say(last.clone());
        Ok(Self { source, last })
    }

    /// Say `board`'s projection if it changed: an unchanged board says nothing.
    pub(super) fn board(&mut self, board: &LeaseBoard) {
        let free = Free::of(board);
        if free != self.last {
            self.source.say(free.clone());
            self.last = free;
        }
    }
}
