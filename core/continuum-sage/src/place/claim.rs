//! A claim: what one owner holds or has had prepared, across every place it asked for, as one unit.
//!
//! The consumer holds a [`Claim`] and spends it with [`Claim::consume`], which answers at once from
//! the claim's own atomics and never waits: the engine thread calls it inside a graph (UNIFIED-PAGING.md,
//! "Prepare, consume, commit"). Everything that can wait (making room, demoting) happened before the
//! claim was granted, on the placer's side.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::quantity::Quantity;

/// One claim's identity, never reused by its ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClaimId(pub u64);

/// Who asked: an owner's identity, stable across its restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Owner(pub u64);

/// Which incarnation of an owner asked. A restarted owner has a later generation, and a claim made
/// by an earlier one can no longer be spent or committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// How much of `Q` one claim wants on one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need<P, Q> {
    pub place: P,
    pub amount: Q,
}

/// A granted claim, as its consumer holds it: what is prepared on each place, and what of it is not
/// yet consumed. Cloning shares the same remainder.
#[derive(Debug, Clone)]
pub struct Claim<P, Q> {
    pub(super) id: ClaimId,
    pub(super) owner: Owner,
    pub(super) generation: Generation,
    pub(super) prepared: Arc<[(Need<P, Q>, AtomicU64)]>,
}

impl<P: Copy + PartialEq, Q: Quantity> Claim<P, Q> {
    pub fn id(&self) -> ClaimId {
        self.id
    }

    pub fn owner(&self) -> (Owner, Generation) {
        (self.owner, self.generation)
    }

    /// Spend `amount` on `place` from what this claim had prepared there. Answers at once, never
    /// waits: false when the claim has no such place or not that much left, and nothing is spent.
    pub fn consume(&self, place: P, amount: Q) -> bool {
        let Some((_, left)) = self.prepared.iter().find(|(need, _)| need.place == place) else {
            return false;
        };
        left.fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| left.checked_sub(amount.units())).is_ok()
    }

    /// What is prepared on `place` and not yet consumed, if this claim holds that place.
    pub fn left(&self, place: P) -> Option<Q> {
        self.prepared.iter().find(|(need, _)| need.place == place).map(|(_, left)| Q::of(left.load(Ordering::Acquire)))
    }
}
