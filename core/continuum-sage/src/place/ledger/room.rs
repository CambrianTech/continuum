//! What a claim can be granted on a place: its own free room, the free room of the physical pool it
//! draws on, and less every other owner's floor not yet filled. The one rule for "room"
//! (UNIFIED-PAGING.md, slice 3a; the kernel authority's `available_for`, moved here).
//!
//! - **Pools.** A place may draw on another place's physical memory (on unified memory, VRAM on RAM).
//!   Every amount charged to it is charged to its pool too, once, and its room is the smaller of its
//!   own and its pool's (Codex: one physical memory, one charge).
//! - **Floors.** An owner may be guaranteed an amount of a place (a live call's VRAM). Another owner's
//!   room is net of what that floor still lacks; the floor's owner may fill it. A floor is a guarantee
//!   like capacity, enforced here, never traded by the law (Fable, on the mapping).

use super::super::claim::{Need, Owner};
use super::super::quantity::Quantity;
use super::{Ledger, Stage};

/// `amount` of `place` guaranteed to `owner`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Floor<P, Q> {
    pub(super) owner: Owner,
    pub(super) place: P,
    pub(super) amount: Q,
}

impl<P: Copy + PartialEq + std::fmt::Debug, Q: Quantity> Ledger<P, Q> {
    /// `place` draws on `pool`'s physical memory (`None`: its own). Tell it before granting on it.
    pub fn back(&mut self, place: P, pool: Option<P>) {
        self.pools.retain(|(at, _)| *at != place);
        if let Some(pool) = pool.filter(|pool| *pool != place) {
            self.pools.push((place, pool));
        }
    }

    /// Guarantee `owner` `amount` of `place` (zero withdraws the floor).
    pub fn floor(&mut self, owner: Owner, place: P, amount: Q) {
        self.floors.retain(|floor| floor.owner != owner || floor.place != place);
        if amount.units() > 0 {
            self.floors.push(Floor { owner, place, amount });
        }
    }

    /// What `owner` could be granted on `place` now: the smaller of the place's free room and its pool's,
    /// less what other owners' floors on that physical memory still lack. `None` for a place the ledger
    /// has not been told of.
    pub fn room_for(&self, owner: Owner, place: P) -> Option<Q> {
        let free = self.pooled_free(place)?.units();
        let root = self.root(place);
        let owed = self
            .floors
            .iter()
            .filter(|floor| floor.owner != owner && self.root(floor.place) == root)
            .map(|floor| floor.amount.units().saturating_sub(self.holding_of(floor.owner, floor.place).units()))
            .fold(0u64, u64::saturating_add);
        Some(Q::of(free.saturating_sub(owed)))
    }

    /// Whether `need` fits within what `owner`'s own floor on its place still lacks: a guarantee, so it
    /// never queues behind another owner's wait.
    pub(super) fn within_floor(&self, owner: Owner, need: &Need<P, Q>) -> bool {
        self.floors
            .iter()
            .find(|floor| floor.owner == owner && floor.place == need.place)
            .is_some_and(|floor| need.amount.units() <= floor.amount.units().saturating_sub(self.holding_of(owner, need.place).units()))
    }

    /// The place's free room, bounded by its pool's.
    pub(super) fn pooled_free(&self, place: P) -> Option<Q> {
        let own = self.accounts(place)?.free();
        match self.pool_of(place) {
            Some(pool) => Some(own.min(self.accounts(pool)?.free())),
            None => Some(own),
        }
    }

    /// The place's capacity, bounded by its pool's: the "never here" bound.
    pub(super) fn pooled_capacity(&self, place: P) -> Option<Q> {
        let own = self.accounts(place)?.capacity;
        match self.pool_of(place) {
            Some(pool) => Some(own.min(self.accounts(pool)?.capacity)),
            None => Some(own),
        }
    }

    pub(super) fn pool_of(&self, place: P) -> Option<P> {
        self.pools.iter().find(|(at, _)| *at == place).map(|(_, pool)| *pool)
    }

    /// The physical memory `place` draws on: its pool, or itself.
    fn root(&self, place: P) -> P {
        self.pool_of(place).unwrap_or(place) // unwrap_or: a place with no pool is its own physical memory, a fact
    }

    /// What `owner` holds or has prepared on `place`.
    fn holding_of(&self, owner: Owner, place: P) -> Q {
        let on = |needs: &[Need<P, Q>]| needs.iter().filter(|need| need.place == place).map(|need| need.amount.units()).fold(0u64, u64::saturating_add);
        let units = self
            .claims
            .values()
            .map(|stage| match stage {
                Stage::Prepared(claim) if claim.owner == owner => claim.prepared.iter().filter(|(need, _)| need.place == place).map(|(need, _)| need.amount.units()).fold(0u64, u64::saturating_add),
                Stage::Committed { owner: held_by, held, .. } if *held_by == owner => on(held),
                _ => 0,
            })
            .fold(0u64, u64::saturating_add);
        Q::of(units)
    }
}
