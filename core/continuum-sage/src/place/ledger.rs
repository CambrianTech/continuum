//! The one picture of what each place holds (UNIFIED-PAGING.md, "Every byte the engine holds is a
//! block", and slice 3a). A place has a proven capacity (what it could ever give, with everything
//! demotable demoted) and a budget (what its lease from the authority gives now, which external
//! pressure shrinks). Its budget is split into what is held by committed claims, what is prepared
//! for granted ones, and what is free.
//!
//! - A need past a place's capacity is the only refusal: [`Answer::Never`], a proven "never here".
//! - A need past what is free waits ([`Answer::Waiting`]), in order of asking, and is granted when
//!   room appears: a budget that grows, a claim unwound or released. A wait names the shortfall on
//!   each place, which is the demand the placement law answers by demoting.
//! - A grant is one claim across every place it asked for: all of it, or a wait.
//! - A claim belongs to one generation of its owner. A later generation retires every earlier one's
//!   claims and waits, so an owner that restarted cannot spend or commit a stale claim.
//!
//! One owner (the placer's component) holds the ledger by `&mut`; the consumer spends a granted claim
//! through its atomics ([`Claim::consume`]) and never touches the ledger.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod room;

use super::claim::{Claim, ClaimId, Expiry, Generation, Need, Owner};
use super::quantity::Quantity;

/// One place's accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accounts<Q> {
    /// What the place could ever give, every demotable holding demoted: the "never here" bound.
    pub capacity: Q,
    /// What its lease gives now.
    pub budget: Q,
    /// What committed claims hold.
    pub held: Q,
    /// What granted claims have prepared and not yet committed or unwound.
    pub prepared: Q,
}

impl<Q: Quantity> Accounts<Q> {
    /// What a new claim could be granted now.
    pub fn free(&self) -> Q {
        Q::of(self.budget.units().saturating_sub(self.held.units().saturating_add(self.prepared.units())))
    }

    /// What claims hold past the budget, after a budget shrank under them: what must be demoted.
    pub fn over(&self) -> Q {
        Q::of(self.held.units().saturating_add(self.prepared.units()).saturating_sub(self.budget.units()))
    }
}

/// The ledger's answer to a prepare.
#[derive(Debug)]
pub enum Answer<P, Q> {
    /// Every need is prepared, as one claim.
    Granted(Claim<P, Q>),
    /// Not now: queued as `id`, short `short` on each place that lacks room.
    Waiting { id: ClaimId, short: Vec<Need<P, Q>> },
    /// Never on this node: `need` is past `place`'s capacity.
    Never { place: P, need: Q, capacity: Q },
    /// A place this ledger has not been told of.
    Unknown { place: P },
    /// An earlier generation of an owner a later one has replaced.
    Stale { owner: Owner, generation: Generation },
}

/// Why a commit, unwind or release was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No live claim has this id (already released, unwound, or retired by its owner's restart).
    UnknownClaim(ClaimId),
    /// The claim is prepared, not committed, so there is nothing held to release.
    NotCommitted(ClaimId),
    /// The claim is committed already.
    Committed(ClaimId),
    /// The claim had lapsed: it is not renewed, only released.
    Lapsed(ClaimId),
    /// The claim holds nothing on that place.
    NotOnPlace(ClaimId),
}

/// A wait withdrawn because a place's capacity fell below what it needs: its owner hears it as never
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeverNow<P, Q> {
    pub id: ClaimId,
    pub owner: Owner,
    pub place: P,
    pub need: Q,
    pub capacity: Q,
}

#[derive(Debug)]
enum Stage<P, Q> {
    Prepared(Claim<P, Q>),
    Committed { owner: Owner, generation: Generation, held: Vec<Need<P, Q>>, expiry: Expiry },
    /// Held by an owner's earlier incarnation, which may still hold the memory (its process or GPU
    /// context not yet gone): charged until its release is fenced, never credited on the restart alone.
    Retired { held: Vec<Need<P, Q>> },
}

#[derive(Debug)]
struct Waiting<P, Q> {
    id: ClaimId,
    owner: Owner,
    generation: Generation,
    needs: Vec<Need<P, Q>>,
}

/// Every place's accounts, every live claim and every wait, owned once.
#[derive(Debug)]
pub struct Ledger<P, Q> {
    places: Vec<(P, Accounts<Q>)>,
    claims: HashMap<ClaimId, Stage<P, Q>>,
    waiting: VecDeque<Waiting<P, Q>>,
    generations: HashMap<Owner, Generation>,
    next: u64,
    /// The physical pool each place that has one draws on ([`Ledger::back`]).
    pools: Vec<(P, P)>,
    /// Each owner's guaranteed amount of a place ([`Ledger::floor`]).
    floors: Vec<room::Floor<P, Q>>,
    /// `grant_waiting`'s scratch: places an earlier wait is still short on. Reused, never reallocated
    /// once it has held as many places as there are.
    blocked: Vec<P>,
}

impl<P: Copy + PartialEq + std::fmt::Debug, Q: Quantity> Default for Ledger<P, Q> {
    fn default() -> Self {
        Self { places: Vec::new(), claims: HashMap::new(), waiting: VecDeque::new(), generations: HashMap::new(), next: 0, pools: Vec::new(), floors: Vec::new(), blocked: Vec::new() }
    }
}

impl<P: Copy + PartialEq + std::fmt::Debug, Q: Quantity> Ledger<P, Q> {
    /// Tell the ledger a place's capacity and its budget now. A place is added the first time.
    /// Afterwards call [`Ledger::grant_waiting`]: a budget that grew may grant a wait. A capacity that
    /// fell below a queued wait's need withdraws that wait into `never`, for its owner to hear.
    pub fn place(&mut self, place: P, capacity: Q, budget: Q, never: &mut Vec<NeverNow<P, Q>>) {
        let budget = budget.min(capacity);
        match self.places.iter_mut().find(|(at, _)| *at == place) {
            Some((_, accounts)) => (accounts.capacity, accounts.budget) = (capacity, budget),
            None => self.places.push((place, Accounts { capacity, budget, held: Q::of(0), prepared: Q::of(0) })),
        }
        self.waiting.retain(|wait| match wait.needs.iter().find(|need| need.place == place && need.amount > capacity) {
            Some(need) => {
                never.push(NeverNow { id: wait.id, owner: wait.owner, place, need: need.amount, capacity });
                false
            }
            None => true,
        });
    }

    pub fn accounts(&self, place: P) -> Option<Accounts<Q>> {
        self.places.iter().find(|(at, _)| *at == place).map(|(_, accounts)| *accounts)
    }

    /// Ask for `needs` (at most one per place; repeats are summed) as one claim of `owner`'s
    /// `generation`. A later generation than the ledger knows retires the earlier one's claims first.
    pub fn prepare(&mut self, owner: Owner, generation: Generation, needs: &[Need<P, Q>]) -> Answer<P, Q> {
        if let Some(stale) = self.generation_is_stale(owner, generation) {
            return stale;
        }
        let mut merged: Vec<Need<P, Q>> = Vec::with_capacity(needs.len());
        for need in needs {
            match merged.iter_mut().find(|at| at.place == need.place) {
                Some(at) => at.amount = Q::of(at.amount.units().saturating_add(need.amount.units())),
                None => merged.push(*need),
            }
        }
        for need in &merged {
            if self.pooled_capacity(need.place).is_none() {
                return Answer::Unknown { place: need.place };
            }
        }
        if let Some((place, need, capacity)) = self.past_capacity(&merged) {
            return Answer::Never { place, need, capacity };
        }
        let id = self.issue();
        // in order of asking, per place: a newcomer waits only behind an earlier wait short on a place
        // it also needs, so a wait on one place never holds up another
        // (a need within its owner's own floor is a guarantee and queues behind no one)
        let mut blocked = std::mem::take(&mut self.blocked);
        self.held_by_waits(&mut blocked);
        let behind = merged.iter().any(|need| blocked.contains(&need.place) && !self.within_floor(owner, need));
        self.blocked = blocked;
        if !behind && self.fits(owner, &merged) {
            return Answer::Granted(self.grant(id, owner, generation, merged));
        }
        let short = self.short(owner, &merged);
        self.waiting.push_back(Waiting { id, owner, generation, needs: merged });
        Answer::Waiting { id, short }
    }

    /// Grant every wait that fits now, in order of asking per place. Granted claims are pushed to
    /// `granted` (a buffer the caller reuses). Which places each remaining wait holds is
    /// [`Self::holds`]'s rule.
    pub fn grant_waiting(&mut self, granted: &mut Vec<Claim<P, Q>>) {
        let mut blocked = std::mem::take(&mut self.blocked);
        blocked.clear();
        let mut at = 0;
        while at < self.waiting.len() {
            let wait = &self.waiting[at];
            let held_up = wait.needs.iter().any(|need| blocked.contains(&need.place) && !self.within_floor(wait.owner, need));
            if !held_up && self.fits(wait.owner, &wait.needs) {
                if let Some(wait) = self.waiting.remove(at) {
                    let claim = self.grant(wait.id, wait.owner, wait.generation, wait.needs);
                    granted.push(claim);
                }
                continue;
            }
            self.holds(at, held_up, &mut blocked);
            at += 1;
        }
        self.blocked = blocked;
    }

    /// The places every queued wait holds, into `blocked` (cleared first), without granting.
    fn held_by_waits(&self, blocked: &mut Vec<P>) {
        blocked.clear();
        for at in 0..self.waiting.len() {
            let wait = &self.waiting[at];
            let held_up = wait.needs.iter().any(|need| blocked.contains(&need.place) && !self.within_floor(wait.owner, need));
            self.holds(at, held_up, blocked);
        }
    }

    /// What the wait at `at` holds from later asks. A wait behind an earlier one holds only the places
    /// it is short on, so a wait short on one place never starves another. The first wait in line on
    /// its places (nobody ahead holds any of them) also holds its places that fit, so a stream of
    /// later asks cannot spend them while it waits for its short place and leave it short there next
    /// (Cormac, 2026-11-26 09:24Z: a seat needing Vram and Ram).
    fn holds(&self, at: usize, held_up: bool, blocked: &mut Vec<P>) {
        let wait = &self.waiting[at];
        let needs = &wait.needs;
        if held_up {
            blocked.extend(self.short_places(wait.owner, needs));
        } else {
            blocked.extend(needs.iter().map(|need| need.place));
        }
    }

    /// The claim's consumer is done spending it: what it consumed on each place becomes held, and
    /// what it did not is returned. Its remainder is taken in the same atomic, so a consume after the
    /// commit fails rather than spending what nobody holds.
    pub fn commit(&mut self, id: ClaimId) -> Result<(), Refusal> {
        let claim = match self.claims.remove(&id) {
            Some(Stage::Prepared(claim)) => claim,
            Some(other) => {
                self.claims.insert(id, other);
                return Err(Refusal::Committed(id));
            }
            None => return Err(Refusal::UnknownClaim(id)),
        };
        let held = self.settle(&claim);
        self.claims.insert(id, Stage::Committed { owner: claim.owner, generation: claim.generation, held, expiry: claim.expiry.clone() });
        Ok(())
    }

    /// A granted claim that will not be spent (cancelled, or its allocation failed): everything it
    /// prepared is returned. A queued wait with this id is withdrawn.
    pub fn unwind(&mut self, id: ClaimId) -> Result<(), Refusal> {
        if let Some(at) = self.waiting.iter().position(|wait| wait.id == id) {
            self.waiting.remove(at);
            return Ok(());
        }
        match self.claims.remove(&id) {
            Some(Stage::Prepared(claim)) => {
                self.return_prepared(&claim);
                Ok(())
            }
            Some(other) => {
                self.claims.insert(id, other);
                Err(Refusal::Committed(id))
            }
            None => Err(Refusal::UnknownClaim(id)),
        }
    }

    /// A committed claim's holdings, or a retired incarnation's, are released once the owner's
    /// allocator has released them (the caller's fence): what it held is free again.
    pub fn release(&mut self, id: ClaimId) -> Result<(), Refusal> {
        match self.claims.remove(&id) {
            Some(Stage::Committed { held, .. } | Stage::Retired { held }) => {
                self.return_held(&held);
                Ok(())
            }
            Some(prepared @ Stage::Prepared(_)) => {
                self.claims.insert(id, prepared);
                Err(Refusal::NotCommitted(id))
            }
            None => Err(Refusal::UnknownClaim(id)),
        }
    }

    /// Give a live claim a term: it lapses at `until`. One already lapsed at `now` is not renewed: a
    /// lapsed claim is overdue for reclaim, and only its release frees it.
    pub fn renew(&mut self, id: ClaimId, until: std::time::Instant, now: std::time::Instant) -> Result<(), Refusal> {
        let expiry = match self.claims.get(&id) {
            Some(Stage::Prepared(claim)) => &claim.expiry,
            Some(Stage::Committed { expiry, .. }) => expiry,
            Some(Stage::Retired { .. }) | None => return Err(Refusal::UnknownClaim(id)),
        };
        if expiry.lapsed(now) {
            return Err(Refusal::Lapsed(id));
        }
        expiry.set(until);
        Ok(())
    }

    /// The live claims lapsed at `now`, pushed to `out` (a buffer the caller reuses): what the law
    /// reclaims first. Lapsing frees nothing; their holdings stay charged until released.
    pub fn overdue(&self, now: std::time::Instant, out: &mut Vec<ClaimId>) {
        out.extend(self.claims.iter().filter_map(|(id, stage)| match stage {
            Stage::Prepared(claim) if claim.expiry.lapsed(now) => Some(*id),
            Stage::Committed { expiry, .. } if expiry.lapsed(now) => Some(*id),
            _ => None,
        }));
    }

    /// Part of a committed or retired claim's holding on `place` was released (its allocator gave back
    /// `amount`, the caller's fence): that much is free again. Answers what it still holds there; a
    /// claim holding nothing anywhere is gone.
    pub fn shrink(&mut self, id: ClaimId, place: P, amount: Q) -> Result<Q, Refusal> {
        let held = match self.claims.get_mut(&id) {
            Some(Stage::Committed { held, .. } | Stage::Retired { held }) => held,
            Some(Stage::Prepared(_)) => return Err(Refusal::NotCommitted(id)),
            None => return Err(Refusal::UnknownClaim(id)),
        };
        let Some(on) = held.iter_mut().find(|need| need.place == place) else {
            return Err(Refusal::NotOnPlace(id));
        };
        let freed = on.amount.min(amount);
        on.amount = Q::of(on.amount.units() - freed.units());
        let left = on.amount;
        let empty = held.iter().all(|need| need.amount.units() == 0);
        if empty {
            self.claims.remove(&id);
        }
        self.adjust(place, |accounts| accounts.held = Q::of(accounts.held.units().saturating_sub(freed.units())));
        Ok(left)
    }

    /// What waits are short on `place`: their needs there past what is free. The placement law's
    /// demand on that place. `None` for a place the ledger has not been told of.
    pub fn shortfall(&self, place: P) -> Option<Q> {
        let free = self.pooled_free(place)?.units();
        let wanted = self
            .waiting
            .iter()
            .flat_map(|wait| wait.needs.iter())
            .filter(|need| need.place == place)
            .fold(0u64, |sum, need| sum.saturating_add(need.amount.units()));
        Some(Q::of(wanted.saturating_sub(free)))
    }

    /// Every place's shortfall to its waits, in the order places were told.
    pub fn shortfalls(&self) -> impl Iterator<Item = Need<P, Q>> + '_ {
        self.places.iter().filter_map(|(place, _)| self.shortfall(*place).map(|amount| Need { place: *place, amount }))
    }

    /// How many waits are queued.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    fn generation_is_stale(&mut self, owner: Owner, generation: Generation) -> Option<Answer<P, Q>> {
        match self.generations.get(&owner).copied() {
            Some(known) if generation < known => return Some(Answer::Stale { owner, generation }),
            Some(known) if generation == known => return None,
            _ => {}
        }
        self.generations.insert(owner, generation);
        self.retire_before(owner, generation);
        None
    }

    /// Every claim and wait of `owner` from a generation before `generation` is retired. Its waits and
    /// whatever its prepared claims never consumed return at once. What it consumed or committed stays
    /// charged as `Retired` until [`Ledger::release`] after a fenced release, because the earlier
    /// incarnation's process or GPU context may still hold that memory (Codex, 2026-11-26 07:20Z).
    fn retire_before(&mut self, owner: Owner, generation: Generation) {
        self.waiting.retain(|wait| wait.owner != owner || wait.generation >= generation);
        let retired: Vec<ClaimId> = self
            .claims
            .iter()
            .filter(|(_, stage)| match stage {
                Stage::Prepared(claim) => claim.owner == owner && claim.generation < generation,
                Stage::Committed { owner: held_by, generation: held_in, .. } => *held_by == owner && *held_in < generation,
                Stage::Retired { .. } => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in retired {
            match self.claims.remove(&id) {
                Some(Stage::Prepared(claim)) => {
                    let held = self.settle(&claim);
                    if held.iter().any(|need| need.amount.units() > 0) {
                        self.claims.insert(id, Stage::Retired { held });
                    }
                }
                Some(Stage::Committed { held, .. }) => {
                    self.claims.insert(id, Stage::Retired { held });
                }
                Some(retired @ Stage::Retired { .. }) => {
                    self.claims.insert(id, retired);
                }
                None => {}
            }
        }
    }

    /// Close a prepared claim's spending: take its remainders in one atomic each (a later consume
    /// fails), return what was prepared, charge what was consumed as held, and answer what is held.
    fn settle(&mut self, claim: &Claim<P, Q>) -> Vec<Need<P, Q>> {
        let mut held = Vec::with_capacity(claim.prepared.len());
        for (need, left) in claim.prepared.iter() {
            let consumed = Q::of(need.amount.units().saturating_sub(left.swap(0, Ordering::AcqRel)));
            self.adjust(need.place, |accounts| {
                accounts.prepared = Q::of(accounts.prepared.units().saturating_sub(need.amount.units()));
                accounts.held = Q::of(accounts.held.units().saturating_add(consumed.units()));
            });
            held.push(Need { place: need.place, amount: consumed });
        }
        held
    }

    /// The places where `owner`'s `needs` are short of the room it has now.
    fn short_places(&self, owner: Owner, needs: &[Need<P, Q>]) -> impl Iterator<Item = P> {
        self.short(owner, needs).into_iter().map(|need| need.place)
    }

    fn issue(&mut self) -> ClaimId {
        self.next += 1;
        ClaimId(self.next)
    }

    fn fits(&self, owner: Owner, needs: &[Need<P, Q>]) -> bool {
        self.short(owner, needs).is_empty()
    }

    fn grant(&mut self, id: ClaimId, owner: Owner, generation: Generation, needs: Vec<Need<P, Q>>) -> Claim<P, Q> {
        for need in &needs {
            self.adjust(need.place, |accounts| accounts.prepared = Q::of(accounts.prepared.units().saturating_add(need.amount.units())));
        }
        let prepared: Arc<[(Need<P, Q>, AtomicU64)]> = needs.into_iter().map(|need| (need, AtomicU64::new(need.amount.units()))).collect();
        let claim = Claim { id, owner, generation, prepared, expiry: Expiry::never() };
        self.claims.insert(id, Stage::Prepared(claim.clone()));
        claim
    }

    fn return_prepared(&mut self, claim: &Claim<P, Q>) {
        for (need, left) in claim.prepared.iter() {
            left.store(0, Ordering::Release); // an unwound claim can no longer be spent
            self.adjust(need.place, |accounts| accounts.prepared = Q::of(accounts.prepared.units().saturating_sub(need.amount.units())));
        }
    }

    fn return_held(&mut self, held: &[Need<P, Q>]) {
        for need in held {
            self.adjust(need.place, |accounts| accounts.held = Q::of(accounts.held.units().saturating_sub(need.amount.units())));
        }
    }

    /// Apply `change` to `place`'s accounts and, when it draws on a pool, to the pool's: one physical
    /// memory, charged once there.
    fn adjust(&mut self, place: P, change: impl Fn(&mut Accounts<Q>)) {
        let pool = self.pool_of(place);
        for (at, accounts) in self.places.iter_mut() {
            if *at == place || Some(*at) == pool {
                change(accounts);
            }
        }
    }
}
