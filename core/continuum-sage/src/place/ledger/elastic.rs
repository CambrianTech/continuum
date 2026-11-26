//! Room by priority (UNIFIED-PAGING.md, "Room by priority"; Joel, 2026-11-26: "The memory really needs to
//! make room by priority. Only some are fixed size."). A committed holding may be elastic on a place: it
//! runs on anything from its floor up to what it holds, at a priority. When the first wait on a place is
//! short there, the ledger shares the place's budget by the one flexbox among every holding and that wait,
//! and asks each elastic holding the sharing leaves smaller to shrink to its share. It is a request, never a
//! reclamation: the holder shrinks after its own fenced release (`Ledger::shrink`), and only then is the wait
//! granted, from bytes that are free.
//!
//! - A holding not marked elastic is fixed: it keeps everything it holds (weights, a turn in flight).
//! - A wait with no priority asks nothing of anyone: it waits for what frees by itself.
//! - Shrinks are asked only when they would let the wait fit; a wait that could not fit even then asks
//!   no one to give up anything for it.
//! - The sharing's order is the flex passes': floors, then each holding's current size and the wait's need
//!   in priority order, so a wait outranks only the holdings below it.
//!
//! Every priority here is an input the law sets from measurements (a mind's temperature, a wait's age:
//! Cormac, 15:55Z), never a constant this file picks.

use super::super::claim::ClaimId;
use super::super::flex::{Flex, Flexbox, Landed, Share};
use super::super::quantity::Quantity;
use super::{Ledger, Refusal, Stage};

/// Ask a holding to shrink on a place to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkTo<P, Q> {
    pub id: ClaimId,
    pub place: P,
    pub to: Q,
}

/// The law's inputs and its scratch, reused across passes.
#[derive(Debug)]
pub(super) struct Law<P, Q> {
    /// Elastic holdings: (claim, place, floor, priority).
    elastic: Vec<(ClaimId, P, Q, u8)>,
    /// Waits' priorities.
    priorities: Vec<(ClaimId, u8)>,
    flexbox: Flexbox,
    claims: Vec<Flex<Q>>,
    holders: Vec<(ClaimId, Q)>,
    shares: Vec<Share<Q>>,
}

impl<P, Q> Default for Law<P, Q> {
    fn default() -> Self {
        Self { elastic: Vec::new(), priorities: Vec::new(), flexbox: Flexbox::default(), claims: Vec::new(), holders: Vec::new(), shares: Vec::new() }
    }
}

impl<P: Copy + PartialEq + std::fmt::Debug, Q: Quantity> Ledger<P, Q> {
    /// Mark committed claim `id` elastic on `place`: it can run on `floor` there, at `priority`. Refused for
    /// a claim not committed or not on the place, or a floor above what it holds there.
    pub fn elastic(&mut self, id: ClaimId, place: P, floor: Q, priority: u8) -> Result<(), Refusal> {
        let held = self.committed_on(id, place)?;
        if floor > held {
            return Err(Refusal::NotOnPlace(id));
        }
        self.law.elastic.retain(|(at, on, _, _)| (*at, *on) != (id, place));
        self.law.elastic.push((id, place, floor, priority));
        Ok(())
    }

    /// Give waiting claim `id` a priority: from now on its shortfall may ask lower holdings to shrink.
    /// Refused for a claim that is not waiting.
    pub fn prioritize(&mut self, id: ClaimId, priority: u8) -> Result<(), Refusal> {
        if !self.waiting.iter().any(|wait| wait.id == id) {
            return Err(Refusal::UnknownClaim(id));
        }
        self.law.priorities.retain(|(at, _)| *at != id);
        self.law.priorities.push((id, priority));
        Ok(())
    }

    /// The shrinks that would let each place's first short, prioritized wait fit, written to `out`
    /// (cleared first). A pass over every place told; a few holdings and a few waits each.
    pub fn shrinks(&mut self, out: &mut Vec<ShrinkTo<P, Q>>) {
        out.clear();
        self.law.elastic.retain(|(id, _, _, _)| matches!(self.claims.get(id), Some(Stage::Committed { .. })));
        self.law.priorities.retain(|(id, _)| self.waiting.iter().any(|wait| wait.id == *id));
        for at in 0..self.places.len() {
            let (place, accounts) = self.places[at];
            let free = accounts.free().units();
            let Some((need, priority)) = self.waiting.iter().find_map(|wait| {
                let need = wait.needs.iter().find(|need| need.place == place && need.amount.units() > free)?;
                let priority = self.law.priorities.iter().find(|(id, _)| *id == wait.id).map(|(_, priority)| *priority)?;
                Some((need.amount, priority))
            }) else {
                continue;
            };
            self.share_for(place, accounts.budget, need, priority, out);
        }
    }

    /// Share `place`'s `budget` among its holdings and a wait of `need` at `priority`; push the shrinks
    /// that would let the wait fit.
    fn share_for(&mut self, place: P, budget: Q, need: Q, priority: u8, out: &mut Vec<ShrinkTo<P, Q>>) {
        let Some(accounts) = self.accounts(place) else { return };
        let law = &mut self.law;
        law.claims.clear();
        law.holders.clear();
        let mut elastic_held = 0u64;
        for &(id, on, floor, held_priority) in law.elastic.iter().filter(|(_, on, _, _)| *on == place) {
            let Some(Stage::Committed { held, .. }) = self.claims.get(&id) else { continue };
            let Some(holds) = held.iter().find(|need| need.place == on).map(|need| need.amount) else { continue };
            elastic_held = elastic_held.saturating_add(holds.units());
            law.holders.push((id, holds));
            law.claims.push(Flex { priority: held_priority, floor, min: holds, max: holds, required: true });
        }
        if law.holders.is_empty() {
            return; // nothing on this place can give anything back
        }
        // everything else charged here (fixed holdings, retired ones, claims prepared) keeps all it has
        let fixed = Q::of(accounts.held.units().saturating_add(accounts.prepared.units()).saturating_sub(elastic_held));
        law.claims.push(Flex { priority: u8::MAX, floor: fixed, min: fixed, max: fixed, required: true });
        // its need is its min, never its floor: floors are granted before priority is weighed, and a wait has
        // no floor on anyone else's memory
        law.claims.push(Flex { priority, floor: Q::of(0), min: need, max: need, required: false });
        let wait = law.claims.len() - 1;
        law.flexbox.share(budget, &law.claims, |a, b| a.cmp(&b), &mut law.shares);
        if law.shares[wait].landed != Landed::Satisfied {
            return; // even with every elastic holding at its share the wait would not fit: ask no one
        }
        for (&(id, holds), share) in law.holders.iter().zip(&law.shares) {
            if share.amount < holds {
                out.push(ShrinkTo { id, place, to: share.amount });
            }
        }
    }

    fn committed_on(&self, id: ClaimId, place: P) -> Result<Q, Refusal> {
        match self.claims.get(&id) {
            Some(Stage::Committed { held, .. }) => held.iter().find(|need| need.place == place).map(|need| need.amount).ok_or(Refusal::NotOnPlace(id)),
            Some(_) => Err(Refusal::NotCommitted(id)),
            None => Err(Refusal::UnknownClaim(id)),
        }
    }
}
