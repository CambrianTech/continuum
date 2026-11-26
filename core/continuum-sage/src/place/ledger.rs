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

use super::claim::{Claim, ClaimId, Generation, Need, Owner};
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
    Committed { owner: Owner, generation: Generation, held: Vec<Need<P, Q>> },
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
    /// `grant_waiting`'s scratch: places an earlier wait is still short on. Reused, never reallocated
    /// once it has held as many places as there are.
    blocked: Vec<P>,
}

impl<P: Copy + PartialEq + std::fmt::Debug, Q: Quantity> Default for Ledger<P, Q> {
    fn default() -> Self {
        Self { places: Vec::new(), claims: HashMap::new(), waiting: VecDeque::new(), generations: HashMap::new(), next: 0, blocked: Vec::new() }
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
            let Some(accounts) = self.accounts(need.place) else {
                return Answer::Unknown { place: need.place };
            };
            if need.amount > accounts.capacity {
                return Answer::Never { place: need.place, need: need.amount, capacity: accounts.capacity };
            }
        }
        let id = self.issue();
        // in order of asking, per place: a newcomer waits only behind an earlier wait short on a place
        // it also needs, so a wait on one place never holds up another
        let behind = self.waiting.iter().any(|wait| self.short_places(&wait.needs).any(|place| merged.iter().any(|need| need.place == place)));
        if !behind && self.fits(&merged) {
            return Answer::Granted(self.grant(id, owner, generation, merged));
        }
        let short = self.short(&merged);
        self.waiting.push_back(Waiting { id, owner, generation, needs: merged });
        Answer::Waiting { id, short }
    }

    /// Grant every wait that fits now, in order of asking per place: a wait is passed over only on a
    /// place where an earlier wait is still short, so the oldest wins on its own place and a wait short
    /// on one place never starves another. Granted claims are pushed to `granted` (a buffer the caller
    /// reuses).
    pub fn grant_waiting(&mut self, granted: &mut Vec<Claim<P, Q>>) {
        let mut blocked = std::mem::take(&mut self.blocked);
        blocked.clear();
        let mut at = 0;
        while at < self.waiting.len() {
            let wait = &self.waiting[at];
            let held_up = wait.needs.iter().any(|need| blocked.contains(&need.place));
            if !held_up && self.fits(&wait.needs) {
                if let Some(wait) = self.waiting.remove(at) {
                    let claim = self.grant(wait.id, wait.owner, wait.generation, wait.needs);
                    granted.push(claim);
                }
                continue;
            }
            blocked.extend(self.short_places(&self.waiting[at].needs));
            at += 1;
        }
        self.blocked = blocked;
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
        self.claims.insert(id, Stage::Committed { owner: claim.owner, generation: claim.generation, held });
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

    /// What waits are short on `place`: their needs there past what is free. The placement law's
    /// demand on that place. `None` for a place the ledger has not been told of.
    pub fn shortfall(&self, place: P) -> Option<Q> {
        let free = self.accounts(place)?.free().units();
        let wanted = self
            .waiting
            .iter()
            .flat_map(|wait| wait.needs.iter())
            .filter(|need| need.place == place)
            .fold(0u64, |sum, need| sum.saturating_add(need.amount.units()));
        Some(Q::of(wanted.saturating_sub(free)))
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

    /// The places where `needs` are short of what is free now.
    fn short_places<'n>(&'n self, needs: &'n [Need<P, Q>]) -> impl Iterator<Item = P> + 'n {
        needs.iter().filter(|need| self.accounts(need.place).is_some_and(|accounts| need.amount > accounts.free())).map(|need| need.place)
    }

    fn issue(&mut self) -> ClaimId {
        self.next += 1;
        ClaimId(self.next)
    }

    fn fits(&self, needs: &[Need<P, Q>]) -> bool {
        needs.iter().all(|need| self.accounts(need.place).is_some_and(|accounts| need.amount <= accounts.free()))
    }

    fn short(&self, needs: &[Need<P, Q>]) -> Vec<Need<P, Q>> {
        needs
            .iter()
            .filter_map(|need| {
                let free = self.accounts(need.place)?.free();
                (need.amount > free).then(|| Need { place: need.place, amount: Q::of(need.amount.units() - free.units()) })
            })
            .collect()
    }

    fn grant(&mut self, id: ClaimId, owner: Owner, generation: Generation, needs: Vec<Need<P, Q>>) -> Claim<P, Q> {
        for need in &needs {
            self.adjust(need.place, |accounts| accounts.prepared = Q::of(accounts.prepared.units().saturating_add(need.amount.units())));
        }
        let prepared: Arc<[(Need<P, Q>, AtomicU64)]> = needs.into_iter().map(|need| (need, AtomicU64::new(need.amount.units()))).collect();
        let claim = Claim { id, owner, generation, prepared };
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

    fn adjust(&mut self, place: P, change: impl FnOnce(&mut Accounts<Q>)) {
        if let Some((_, accounts)) = self.places.iter_mut().find(|(at, _)| *at == place) {
            change(accounts);
        }
    }
}
