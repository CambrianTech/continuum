//! The placer: the one SAGE component that owns a [`Ledger`] (UNIFIED-PAGING.md, slice 3a). Nobody
//! holds it, awaits it or locks it; a consumer spends a granted claim straight from its atomics.
//!
//! - **In: acts, by its `Teller`.** Lossless, in order, from any number of consumers. An event topic
//!   has one writer and drops a lagging subscriber's oldest, so it suits neither many askers nor an ask
//!   that must be answered. A consumer names each ask (`AskId`: its owner and its own sequence) and
//!   refers to it after: commit, unwind, release, forget. The authority tells it room the same way.
//! - **Out: state, never events.** Each owner's asks are a state keyed by the owner on `Domain::ASKS`:
//!   every live ask and where it stands (waiting with its shortfall, granted with the claim, committed,
//!   retired, or a terminal answer). It is the whole table each time, so a consumer that wakes late or
//!   restarts reads where everything stands; nothing hangs on catching one event. `Domain::SHORT` is
//!   each place's shortfall to its waits: the law's demand, and how far each wait is from its grant.
//!
//! One placer per domain: a domain names its places, its quantity and its topics, so bytes on a node's
//! tiers and minds awake on it are two placers on the same code.

use std::collections::HashMap;
use std::fmt::Debug;

use super::claim::{Claim, ClaimId, Generation, Need, Owner};
use super::ledger::{Answer, Ledger, NeverNow, Refusal, ShrinkTo};
use super::quantity::Quantity;
use crate::component::{Bus, Component, Ctx, Key, Taken, Wiring};
use crate::dataflow::{Publisher, TopicName};
use crate::Qos;

/// What one placer places: its places, the quantity they hold, and its topics.
pub trait Domain: Send + 'static {
    type Place: Copy + PartialEq + Debug + Send + Sync + 'static;
    type Quantity: Quantity;
    /// The placer's component name.
    const NAME: &'static str;
    /// Each owner's asks, a state keyed by the owner.
    const ASKS: TopicName;
    /// Each place's shortfall to its waits, a state.
    const SHORT: TopicName;
}

/// One ask, named by its consumer: its owner and the consumer's own sequence for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AskId {
    pub owner: Owner,
    pub seq: u64,
}

/// The key an owner's asks are published under.
pub fn key_of(owner: Owner) -> Key {
    Key(u128::from(owner.0))
}

/// What the placer is told.
#[derive(Debug)]
pub enum Act<P, Q> {
    Prepare { ask: AskId, generation: Generation, needs: Vec<Need<P, Q>> },
    /// The claim's spending is over: what it consumed is held.
    Commit(AskId),
    /// It will not be spent: everything it prepared returns.
    Unwind(AskId),
    /// Its holding was released by its allocator (the consumer's fence): free again.
    Release(AskId),
    /// A terminal answer has been read: drop it from the owner's table.
    Forget(AskId),
    /// A place's room, as the authority measures it.
    Room { place: P, capacity: Q, budget: Q },
    /// Its committed holding on `place` can run on `floor`, at `priority`: the law may ask it to shrink.
    Elastic { ask: AskId, place: P, floor: Q, priority: u8 },
    /// Its waiting ask's priority, as the law measures it (temperature, age): it may now ask lower
    /// holdings to shrink.
    Prioritize { ask: AskId, priority: u8 },
    /// Its holding on `place` released `by` after its own fence: free again, and its shrink answered.
    Shrunk { ask: AskId, place: P, by: Q },
}

/// Where one ask stands.
#[derive(Debug, Clone)]
pub enum Status<P, Q> {
    Waiting { short: Vec<Need<P, Q>> },
    Granted(Claim<P, Q>),
    Committed,
    /// Held by an earlier generation of its owner: charged until released.
    Retired,
    Never { place: P, need: Q, capacity: Q },
    Unknown { place: P },
    Stale,
    Refused(Refusal),
    /// Committed, and asked to shrink to `to` on `place` so a higher wait fits: it releases the rest
    /// after its own fence and says [`Act::Shrunk`].
    Shrink { place: P, to: Q },
}

/// One owner's asks, as the placer publishes them: (its sequence, where it stands).
#[derive(Debug, Clone)]
pub struct Asks<P, Q>(pub Vec<(u64, Status<P, Q>)>);

/// Each place's shortfall to its waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall<P, Q>(pub Vec<Need<P, Q>>);

struct Entry<P, Q> {
    seq: u64,
    claim: Option<ClaimId>,
    status: Status<P, Q>,
}

struct Table<P, Q> {
    generation: Generation,
    entries: Vec<Entry<P, Q>>,
    says: Publisher<Asks<P, Q>>,
    changed: bool,
}

/// The component around one domain's ledger.
pub struct Placer<D: Domain> {
    ledger: Ledger<D::Place, D::Quantity>,
    bus: &'static Bus,
    owners: HashMap<Owner, Table<D::Place, D::Quantity>>,
    asks_of: HashMap<ClaimId, AskId>,
    short: Publisher<Shortfall<D::Place, D::Quantity>>,
    last_short: Shortfall<D::Place, D::Quantity>,
    /// Reused across inputs: claims a pass granted, waits a capacity drop withdrew.
    granted: Vec<Claim<D::Place, D::Quantity>>,
    never: Vec<NeverNow<D::Place, D::Quantity>>,
    /// Reused: the shrinks the last pass asked for.
    shrinks: Vec<ShrinkTo<D::Place, D::Quantity>>,
}

impl<D: Domain> Placer<D> {
    /// Claim the domain's shortfall topic on `bus` (an owner's asks are claimed at its first ask);
    /// refused if another placer holds it.
    pub fn new(bus: &'static Bus) -> Result<Self, Taken> {
        Ok(Self {
            ledger: Ledger::default(),
            bus,
            owners: HashMap::new(),
            asks_of: HashMap::new(),
            short: bus.publisher(D::SHORT, Key::ONE)?,
            last_short: Shortfall(Vec::new()),
            granted: Vec::new(),
            never: Vec::new(),
            shrinks: Vec::new(),
        })
    }

    fn act(&mut self, act: Act<D::Place, D::Quantity>) {
        match act {
            Act::Prepare { ask, generation, needs } => self.prepare(ask, generation, &needs),
            Act::Commit(ask) => self.settle(ask, |ledger, id| ledger.commit(id), Some(Status::Committed)),
            Act::Unwind(ask) => self.settle(ask, |ledger, id| ledger.unwind(id), None),
            Act::Release(ask) => self.settle(ask, |ledger, id| ledger.release(id), None),
            Act::Forget(ask) => {
                if let Some(table) = self.owners.get_mut(&ask.owner) {
                    table.entries.retain(|entry| entry.seq != ask.seq || entry.claim.is_some());
                    table.changed = true;
                }
            }
            Act::Room { place, capacity, budget } => self.ledger.place(place, capacity, budget, &mut self.never),
            Act::Elastic { ask, place, floor, priority } => self.told(ask, |ledger, id| ledger.elastic(id, place, floor, priority)),
            Act::Prioritize { ask, priority } => self.told(ask, |ledger, id| ledger.prioritize(id, priority)),
            Act::Shrunk { ask, place, by } => {
                let freed = by.units();
                self.settle(ask, |ledger, id| ledger.shrink(id, place, by).map(|_left| ()), Some(Status::Committed));
                continuum_base::probe!(class = "place.shrunk", placer = D::NAME, owner = ask.owner.0, place = ?place, freed,
                    "a holding gave back what the law asked, after its fence");
            }
        }
    }

    /// Tell the ledger something about `ask`'s claim that leaves where it stands unchanged; a refusal is
    /// recorded on it.
    fn told(&mut self, ask: AskId, apply: impl FnOnce(&mut Ledger<D::Place, D::Quantity>, ClaimId) -> Result<(), Refusal>) {
        let Some(table) = self.owners.get_mut(&ask.owner) else { return };
        let Some(entry) = table.entries.iter_mut().find(|entry| entry.seq == ask.seq) else { return };
        let Some(id) = entry.claim else { return };
        if let Err(refused) = apply(&mut self.ledger, id) {
            entry.status = Status::Refused(refused);
            table.changed = true;
        }
    }

    fn prepare(&mut self, ask: AskId, generation: Generation, needs: &[Need<D::Place, D::Quantity>]) {
        // the owner's table first: an ask whose answer cannot be said is never prepared, so no claim
        // is held for an owner who could not read it
        if let Err(taken) = self.table(ask.owner) {
            continuum_base::probe!(class = "place.asks.taken", placer = D::NAME, owner = ask.owner.0, topic = taken.0 .0,
                "an owner's asks key is held elsewhere: its ask is not prepared");
            return;
        }
        let answer = self.ledger.prepare(ask.owner, generation, needs);
        let Some(table) = self.owners.get_mut(&ask.owner) else { return };
        if generation > table.generation {
            // a later incarnation: its predecessor's holdings stay charged as retired, everything else of it is gone
            table.generation = generation;
            table.entries.retain_mut(|entry| match entry.status {
                Status::Granted(_) | Status::Committed => {
                    entry.status = Status::Retired;
                    true
                }
                Status::Retired => true,
                _ => false,
            });
        }
        let (claim, status) = match answer {
            Answer::Granted(claim) => (Some(claim.id()), Status::Granted(claim)),
            Answer::Waiting { id, short } => (Some(id), Status::Waiting { short }),
            Answer::Never { place, need, capacity } => (None, Status::Never { place, need, capacity }),
            Answer::Unknown { place } => (None, Status::Unknown { place }),
            Answer::Stale { .. } => (None, Status::Stale),
        };
        table.entries.push(Entry { seq: ask.seq, claim, status });
        table.changed = true;
        if let Some(id) = claim {
            self.asks_of.insert(id, ask);
        }
    }

    /// Commit, unwind or release the claim behind `ask`. On success the ask becomes `then`, or leaves
    /// the table; a refusal is recorded on it.
    fn settle(
        &mut self,
        ask: AskId,
        apply: impl FnOnce(&mut Ledger<D::Place, D::Quantity>, ClaimId) -> Result<(), Refusal>,
        then: Option<Status<D::Place, D::Quantity>>,
    ) {
        let Some(table) = self.owners.get_mut(&ask.owner) else { return };
        let Some(at) = table.entries.iter().position(|entry| entry.seq == ask.seq) else { return };
        let Some(id) = table.entries[at].claim else { return };
        table.changed = true;
        match (apply(&mut self.ledger, id), then) {
            (Ok(()), Some(status)) => table.entries[at].status = status,
            (Ok(()), None) => {
                table.entries.remove(at);
                self.asks_of.remove(&id);
            }
            (Err(refused), _) => table.entries[at].status = Status::Refused(refused),
        }
    }

    /// Make sure the owner has a table, claiming its topic key at its first ask; refused if the key is
    /// held elsewhere.
    fn table(&mut self, owner: Owner) -> Result<(), Taken> {
        if !self.owners.contains_key(&owner) {
            let says = self.bus.publisher(D::ASKS, key_of(owner))?;
            self.owners.insert(owner, Table { generation: Generation(0), entries: Vec::new(), says, changed: false });
        }
        Ok(())
    }

    /// After any act: grant what now fits, answer waits that can never fit, and say every table and
    /// the shortfall that changed.
    fn publish(&mut self, ctx: &mut Ctx<Self>) {
        self.ledger.grant_waiting(&mut self.granted);
        for claim in self.granted.drain(..) {
            let Some(ask) = self.asks_of.get(&claim.id()).copied() else { continue };
            if let Some(entry) = self.owners.get_mut(&ask.owner).and_then(|table| {
                table.changed = true;
                table.entries.iter_mut().find(|entry| entry.seq == ask.seq)
            }) {
                entry.status = Status::Granted(claim);
            }
        }
        for never in self.never.drain(..) {
            let Some(ask) = self.asks_of.remove(&never.id) else { continue };
            if let Some(entry) = self.owners.get_mut(&ask.owner).and_then(|table| {
                table.changed = true;
                table.entries.iter_mut().find(|entry| entry.seq == ask.seq)
            }) {
                (entry.claim, entry.status) = (None, Status::Never { place: never.place, need: never.need, capacity: never.capacity });
            }
        }
        // room by priority: each holding a higher wait outranks is asked to shrink, on its own table
        self.ledger.shrinks(&mut self.shrinks);
        for shrink in &self.shrinks {
            let Some(ask) = self.asks_of.get(&shrink.id).copied() else { continue };
            let Some(table) = self.owners.get_mut(&ask.owner) else { continue };
            let Some(entry) = table.entries.iter_mut().find(|entry| entry.seq == ask.seq) else { continue };
            if matches!(entry.status, Status::Shrink { place, to } if place == shrink.place && to == shrink.to) {
                continue; // already asked: said once
            }
            entry.status = Status::Shrink { place: shrink.place, to: shrink.to };
            table.changed = true;
            continuum_base::probe!(class = "place.shrink.asked", placer = D::NAME, owner = ask.owner.0, place = ?shrink.place, to = shrink.to.units(),
                "a holding was asked to shrink so a higher wait fits: the law's feedback, with place.shrunk when it lands");
        }
        for table in self.owners.values_mut().filter(|table| table.changed) {
            table.changed = false;
            let asks = Asks(table.entries.iter().map(|entry| (entry.seq, entry.status.clone())).collect());
            ctx.publish(&mut table.says, asks);
        }
        let short = Shortfall(self.ledger.shortfalls().filter(|need| need.amount.units() > 0).collect());
        if short != self.last_short {
            self.last_short.0.clone_from(&short.0);
            ctx.publish(&mut self.short, short);
        }
    }
}

impl<D: Domain> Component for Placer<D> {
    type Input = Act<D::Place, D::Quantity>;
    const NAME: &'static str = D::NAME;
    const QOS: Qos = Qos::Interactive;

    /// It hears nothing on the bus: every ask and every room arrives by its teller.
    fn wire(&self, _: &mut Wiring<'_, Self>) {}

    async fn handle(&mut self, act: Self::Input, ctx: &mut Ctx<Self>) {
        self.act(act);
        self.publish(ctx);
    }
}
