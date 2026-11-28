//! The authority's synchronous door into the core (UNIFIED-PAGING.md, step 3 of "Room by priority";
//! Cormac, 2026-11-26): what a caller that cannot await the teller needs, each a params struct in and a
//! typed answer out. A take, a split and a held claim register in their owner's table like any ask, so
//! the law's shrink pass finds them; back and floors are what the core is told, as `Act::Room` is.

use super::super::claim::{Claim, Generation, Need, Owner};
use super::super::component::{AskId, Domain, Status};
use super::super::ledger::{Refusal, Untaken};
use super::{Entry, PlacerCore, Table};

/// Take `needs` for `ask` now, ahead of any wait, or not at all (the foreground's no-wait path).
pub struct TakeAsk<P, Q> {
    pub ask: AskId,
    pub generation: Generation,
    pub needs: Vec<Need<P, Q>>,
}

/// Move `moved` of `from`'s committed holding to a new committed claim `into` (of `generation`), every
/// charge unchanged.
pub struct SplitAsk<P, Q> {
    pub from: AskId,
    pub moved: Need<P, Q>,
    pub into: AskId,
    pub generation: Generation,
}

/// Which physical pool `place` draws on (`None`: its own).
pub struct Back<P> {
    pub place: P,
    pub pool: Option<P>,
}

impl<D: Domain> PlacerCore<D> {
    /// Take `ask.needs` now: granted as a claim the caller spends and commits (`Act::Commit`), recorded in
    /// its owner's table; or untaken, with nothing queued and nothing recorded.
    pub fn take(&mut self, ask: TakeAsk<D::Place, D::Quantity>) -> Result<Claim<D::Place, D::Quantity>, Untaken<D::Place, D::Quantity>> {
        let TakeAsk { ask, generation, needs } = ask;
        let claim = self.ledger.take(ask.owner, generation, &needs)?;
        self.record(ask, generation, Some(claim.id()), Status::Granted(claim.clone()));
        Ok(claim)
    }

    /// Split part of `from`'s committed holding into the committed claim `into`.
    pub fn split(&mut self, split: SplitAsk<D::Place, D::Quantity>) -> Result<(), Refusal> {
        let SplitAsk { from, moved, into, generation } = split;
        let Some(id) = self.claim_of(from) else { return Err(Refusal::NoClaim) };
        let new = self.ledger.split(id, moved, into.owner, generation)?;
        self.record(into, generation, Some(new), Status::Committed);
        if self.ledger.has_claim(id) {
            return Ok(());
        }
        // moved to nothing, the source claim is gone: so is its entry
        self.forget_claim(from, id);
        Ok(())
    }

    /// Tell the core which pool `place` draws on.
    pub fn back(&mut self, back: Back<D::Place>) {
        self.ledger.back(back.place, back.pool);
    }

    /// Replace every floor (a guarantee not named again is withdrawn).
    pub fn floors(&mut self, floors: impl IntoIterator<Item = (Owner, D::Place, D::Quantity)>) {
        self.ledger.floors(floors);
    }

    /// What `ask` holds on `place` now (its claim's committed or retired holding): the one record a lease's
    /// size is read from.
    pub fn holding(&self, ask: AskId, place: D::Place) -> Option<D::Quantity> {
        self.ledger.holding(self.claim_of(ask)?, place)
    }

    fn claim_of(&self, ask: AskId) -> Option<super::super::claim::ClaimId> {
        self.owners.get(&ask.owner)?.entries.iter().find(|entry| entry.seq == ask.seq)?.claim
    }

    fn forget_claim(&mut self, ask: AskId, id: super::super::claim::ClaimId) {
        self.asks_of.remove(&id);
        if let Some(table) = self.owners.get_mut(&ask.owner) {
            table.entries.retain(|entry| entry.seq != ask.seq);
            table.changed = true;
        }
    }

    fn record(&mut self, ask: AskId, generation: Generation, claim: Option<super::super::claim::ClaimId>, status: Status<D::Place, D::Quantity>) {
        let table = self.owners.entry(ask.owner).or_insert_with(|| Table { generation, entries: Vec::new(), changed: false });
        table.entries.push(Entry { seq: ask.seq, claim, status });
        table.changed = true;
        if let Some(id) = claim {
            self.asks_of.insert(id, ask);
        }
    }
}

impl<D: Domain> PlacerCore<D> {
    /// `ask` leaves its owner's table whatever stands in it (a holding shrunk to nothing, a refused take),
    /// so a table never keeps an ask nobody will settle.
    pub fn drop_ask(&mut self, ask: AskId) {
        if let Some(id) = self.claim_of(ask) {
            self.asks_of.remove(&id);
        }
        if let Some(table) = self.owners.get_mut(&ask.owner) {
            table.entries.retain(|entry| entry.seq != ask.seq);
            table.changed = true;
        }
    }
}

impl<D: Domain> std::fmt::Debug for PlacerCore<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlacerCore").field("ledger", &self.ledger).field("owners", &self.owners.len()).field("asks", &self.asks_of.len()).finish()
    }
}

impl<D: Domain> PlacerCore<D> {
    /// How many asks the core holds across every owner's table, and how many claims it maps back to asks:
    /// what a churn test reads to prove nothing is left behind.
    pub fn held_entries(&self) -> (usize, usize) {
        (self.owners.values().map(|table| table.entries.len()).sum(), self.asks_of.len())
    }
}
