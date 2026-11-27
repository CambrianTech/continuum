//! A claim: what one owner holds or has had prepared, across every place it asked for, as one unit.
//!
//! The consumer holds a [`Claim`] and spends it with [`Claim::consume`], which answers at once from
//! the claim's own atomics and never waits: the engine thread calls it inside a graph (UNIFIED-PAGING.md,
//! "Prepare, consume, commit"). Everything that can wait (making room, demoting) happened before the
//! claim was granted, on the placer's side.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    pub(super) expiry: Expiry,
}

impl<P: Copy + PartialEq, Q: Quantity> Claim<P, Q> {
    pub fn id(&self) -> ClaimId {
        self.id
    }

    pub fn owner(&self) -> (Owner, Generation) {
        (self.owner, self.generation)
    }

    /// Spend `amount` on `place` from what this claim had prepared there. Answers at once, never
    /// waits: false when the claim has no such place, not that much left, or has lapsed, and nothing
    /// is spent.
    pub fn consume(&self, place: P, amount: Q) -> bool {
        if self.expiry.lapsed(Instant::now()) {
            return false; // a lapsed claim is not spent
        }
        let Some((_, left)) = self.prepared.iter().find(|(need, _)| need.place == place) else {
            return false;
        };
        left.fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| left.checked_sub(amount.units())).is_ok()
    }

    /// What it has consumed on each place so far, read from its atomics (a recording keeps it at commit, so a
    /// replay spends what the field spent).
    pub(super) fn spent(&self) -> impl Iterator<Item = Need<P, Q>> + '_ {
        self.prepared.iter().map(|(need, left)| Need { place: need.place, amount: Q::of(need.amount.units().saturating_sub(left.load(Ordering::Acquire))) })
    }

    /// What is prepared on `place` and not yet consumed, if this claim holds that place: nothing once
    /// it has lapsed.
    pub fn left(&self, place: P) -> Option<Q> {
        let lapsed = self.expiry.lapsed(Instant::now());
        self.prepared.iter().find(|(need, _)| need.place == place).map(|(_, left)| Q::of(if lapsed { 0 } else { left.load(Ordering::Acquire) }))
    }
}

/// When a claim lapses, readable from any thread without the ledger: nanoseconds past `origin`,
/// `u64::MAX` for none. Past it the claim is overdue: its remainder reads nothing and cannot be spent,
/// and the ledger lists it for reclaim, but its holding is freed only by a release (a lease that
/// lapsed has not yet given its memory back).
#[derive(Debug, Clone)]
pub(super) struct Expiry {
    origin: Instant,
    until: Arc<AtomicU64>,
}

impl Expiry {
    pub(super) fn never() -> Self {
        Self { origin: Instant::now(), until: Arc::new(AtomicU64::new(u64::MAX)) }
    }

    /// Lapse at `until` (an instant before the claim was made lapses at once).
    pub(super) fn set(&self, until: Instant) {
        let nanos = until.saturating_duration_since(self.origin).as_nanos();
        // a term past u64 nanoseconds (584 years) still ends: it is held just below "none"
        self.until.store(nanos.min(u128::from(u64::MAX - 1)) as u64, Ordering::Release);
    }

    /// Whether it has lapsed at `now`.
    pub(super) fn lapsed(&self, now: Instant) -> bool {
        let until = self.until.load(Ordering::Acquire);
        until != u64::MAX && u128::from(until) <= now.saturating_duration_since(self.origin).as_nanos()
    }
}
