//! The placer's synchronous core (UNIFIED-PAGING.md, step 3 of "Room by priority"; Cormac, 2026-11-26): one
//! ledger and every owner's table, changed by `apply` and settled by `settle`, with nothing about the bus.
//! The SAGE component wraps it (asks by its teller, answers as state); the kernel authority holds the same
//! core for its synchronous door (an acquire that cannot await a teller). One ledger either way. Whoever
//! holds it locks it only around one `apply` and its `settle`, never across an await.

use std::collections::HashMap;

use super::claim::{Claim, ClaimId, Generation, Need, Owner};
use super::component::{Act, AskId, Asks, Domain, Shortfall, Status};
use super::ledger::{Answer, Ledger, NeverNow, Refusal, ShrinkTo};
use super::quantity::Quantity;
use super::recording::{Recorded, Recording};

struct Entry<P, Q> {
    seq: u64,
    claim: Option<ClaimId>,
    status: Status<P, Q>,
}

struct Table<P, Q> {
    generation: Generation,
    entries: Vec<Entry<P, Q>>,
    changed: bool,
}

/// One domain's ledger and its owners' tables.
pub struct PlacerCore<D: Domain> {
    ledger: Ledger<D::Place, D::Quantity>,
    owners: HashMap<Owner, Table<D::Place, D::Quantity>>,
    asks_of: HashMap<ClaimId, AskId>,
    short: Shortfall<D::Place, D::Quantity>,
    /// Reused across settles: claims a pass granted, waits a capacity drop withdrew, shrinks asked.
    granted: Vec<Claim<D::Place, D::Quantity>>,
    never: Vec<NeverNow<D::Place, D::Quantity>>,
    shrinks: Vec<ShrinkTo<D::Place, D::Quantity>>,
    /// The last acts applied, for a field recording (capacity 0 by default: recording is the owner's choice).
    recording: Recording<D::Place, D::Quantity>,
}

impl<D: Domain> Default for PlacerCore<D> {
    fn default() -> Self {
        Self {
            ledger: Ledger::default(),
            owners: HashMap::new(),
            asks_of: HashMap::new(),
            short: Shortfall(Vec::new()),
            granted: Vec::new(),
            never: Vec::new(),
            shrinks: Vec::new(),
            recording: Recording::new(0),
        }
    }
}

impl<D: Domain> PlacerCore<D> {
    /// A core that keeps its last `capacity` acts as a field recording ([`super::replay`] reproduces it anywhere).
    pub fn recording(capacity: usize) -> Self {
        Self { recording: Recording::new(capacity), ..Self::default() }
    }

    /// The acts recorded so far.
    pub fn recorded(&self) -> &Recording<D::Place, D::Quantity> {
        &self.recording
    }

    /// The ledger, for a synchronous reader (the authority's board).
    pub fn ledger(&self) -> &Ledger<D::Place, D::Quantity> {
        &self.ledger
    }

    /// Where `ask` stands now, if its owner's table holds it.
    pub fn status(&self, ask: AskId) -> Option<&Status<D::Place, D::Quantity>> {
        self.owners.get(&ask.owner)?.entries.iter().find(|entry| entry.seq == ask.seq).map(|entry| &entry.status)
    }

    /// Whether `owner` has a table (its first ask has been prepared).
    pub fn knows(&self, owner: Owner) -> bool {
        self.owners.contains_key(&owner)
    }

    /// Apply one act. Call [`Self::settle`] after it, before anyone reads the tables.
    pub fn apply(&mut self, act: &Act<D::Place, D::Quantity>) {
        if self.recording.on() {
            if let Act::Commit(ask) = act {
                let spent = match self.status(*ask) { Some(Status::Granted(claim)) => Some(claim.spent().collect()), _ => None };
                if let Some(spent) = spent {
                    self.recording.record(Recorded::Spent { ask: *ask, spent });
                }
            }
            self.recording.record(Recorded::Act(act.clone())); // the recording owns its copy: the replay reads it back
        }
        match act {
            &Act::Prepare { ask, generation, ref needs } => self.prepare(ask, generation, needs),
            &Act::Commit(ask) => self.settle_ask(ask, |ledger, id| ledger.commit(id), Some(Status::Committed)),
            &Act::Unwind(ask) => self.settle_ask(ask, |ledger, id| ledger.unwind(id), None),
            &Act::Release(ask) => self.settle_ask(ask, |ledger, id| ledger.release(id), None),
            &Act::Forget(ask) => {
                if let Some(table) = self.owners.get_mut(&ask.owner) {
                    table.entries.retain(|entry| entry.seq != ask.seq || entry.claim.is_some());
                    table.changed = true;
                }
            }
            &Act::Room { place, capacity, budget } => self.ledger.place(place, capacity, budget, &mut self.never),
            &Act::Elastic { ask, place, floor, priority } => self.told(ask, |ledger, id| ledger.elastic(id, place, floor, priority)),
            &Act::Prioritize { ask, priority } => self.told(ask, |ledger, id| ledger.prioritize(id, priority)),
            &Act::Shrunk { ask, place, by } => {
                let freed = by.units();
                self.settle_ask(ask, |ledger, id| ledger.shrink(id, place, by).map(|_left| ()), Some(Status::Committed));
                continuum_base::probe!(class = "place.shrunk", placer = D::NAME, owner = ask.owner.0, place = ?place, freed,
                    "a holding gave back what the law asked, after its fence");
            }
        }
    }

    /// After any act: grant what now fits, answer waits that can never fit, and ask each holding a higher
    /// wait outranks to shrink. Answers whether the shortfall changed; the tables that changed are drained
    /// by [`Self::changed`].
    pub fn settle(&mut self) -> bool {
        self.ledger.grant_waiting(&mut self.granted);
        for claim in self.granted.drain(..) {
            let Some(ask) = self.asks_of.get(&claim.id()).copied() else { continue };
            if let Some(entry) = entry_of(&mut self.owners, ask) {
                entry.status = Status::Granted(claim);
            }
        }
        for never in self.never.drain(..) {
            let Some(ask) = self.asks_of.remove(&never.id) else { continue };
            if let Some(entry) = entry_of(&mut self.owners, ask) {
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
        let short = Shortfall(self.ledger.shortfalls().filter(|need| need.amount.units() > 0).collect());
        let changed = short != self.short;
        self.short = short;
        changed
    }

    /// Each owner whose table changed since the last call, with its whole table now: written to `out`
    /// (cleared first), and the changes marked said.
    pub fn changed(&mut self, out: &mut Vec<(Owner, Asks<D::Place, D::Quantity>)>) {
        out.clear();
        for (owner, table) in self.owners.iter_mut().filter(|(_, table)| table.changed) {
            table.changed = false;
            out.push((*owner, Asks(table.entries.iter().map(|entry| (entry.seq, entry.status.clone())).collect())));
        }
    }

    /// Each place's shortfall to its waits, as last settled.
    pub fn shortfall(&self) -> &Shortfall<D::Place, D::Quantity> {
        &self.short
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
        let answer = self.ledger.prepare(ask.owner, generation, needs);
        let table = self.owners.entry(ask.owner).or_insert_with(|| Table { generation: Generation(0), entries: Vec::new(), changed: false });
        if generation > table.generation {
            // a later incarnation: its predecessor's holdings stay charged as retired, everything else of it is gone
            table.generation = generation;
            table.entries.retain_mut(|entry| match entry.status {
                Status::Granted(_) | Status::Committed | Status::Shrink { .. } => {
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

    /// Commit, unwind, release or shrink the claim behind `ask`. On success the ask becomes `then`, or leaves
    /// the table; a refusal is recorded on it.
    fn settle_ask(
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
}

/// `ask`'s entry, its table marked changed.
fn entry_of<P, Q>(owners: &mut HashMap<Owner, Table<P, Q>>, ask: AskId) -> Option<&mut Entry<P, Q>> {
    let table = owners.get_mut(&ask.owner)?;
    table.changed = true;
    table.entries.iter_mut().find(|entry| entry.seq == ask.seq)
}
