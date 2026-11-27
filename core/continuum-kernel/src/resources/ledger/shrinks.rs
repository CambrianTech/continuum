//! The law's shrink asks for the authority's leases (UNIFIED-PAGING.md "Room by priority", step 3 part 6):
//! a lease its holder marks elastic can be asked to give back so a higher wait fits. The core raises the ask
//! on the lease's entry (Status::Shrink, said once); the authority collects the asks due for its leases and
//! the daemon delivers each to the lease's registered holder, outside the governor lock. The ask carries the
//! lease's own id, and the fence completes through that lease's own shrink or release, once: a late or
//! repeated answer finds nothing left to settle.

use continuum_sage::place::{Act, Asks, Bytes, Owner, Status};

use super::{LeaseError, ResourceKind, ResourceLeaseLedger};

/// How a lease may give back: never below `floor` bytes, at `priority` (lower gives back first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elastic {
    pub floor: u64,
    pub priority: u8,
}

/// One lease the law asks to give back: shrink `lease_id` (of `kind`, holding `held`) to `to` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShrinkDue {
    /// The lease's entry in the core: who it is, by id.
    pub ask: continuum_sage::place::AskId,
    pub lease_id: String,
    pub kind: ResourceKind,
    pub held: u64,
    pub to: u64,
}

impl ResourceLeaseLedger {
    /// Mark `lease_id` elastic: the law may ask it to give back down to `elastic.floor`.
    pub fn make_elastic(&mut self, lease_id: &str, elastic: Elastic) -> Result<(), LeaseError> {
        let held = self.leases.get(lease_id).ok_or_else(|| LeaseError::MissingLease { lease_id: lease_id.to_owned() })?;
        let (ask, kind) = (held.ask, held.lease.kind);
        self.core.apply(&Act::Elastic { ask, place: kind, floor: Bytes(elastic.floor), priority: elastic.priority });
        self.tell();
        Ok(())
    }

    /// The shrinks the law has asked of this authority's leases since last collected, into `out` (cleared
    /// first). Each is said once: the core marks an entry Shrink only when its target changes.
    pub fn shrinks_due(&mut self, out: &mut Vec<ShrinkDue>) {
        out.clear();
        let mut changed: Vec<(Owner, Asks<ResourceKind, Bytes>)> = Vec::new();
        self.core.changed(&mut changed);
        for (owner, Asks(entries)) in changed {
            for (seq, status) in entries {
                let Status::Shrink { place, to } = status else { continue };
                let held = self.leases.iter().find(|(_, held)| held.ask.owner == owner && held.ask.seq == seq);
                if let Some((lease_id, held)) = held {
                    out.push(ShrinkDue { ask: held.ask, lease_id: lease_id.clone(), kind: place, held: held.lease.bytes, to: to.0 });
                }
            }
        }
    }
}

impl ResourceLeaseLedger {
    /// Give waiting `ask` a priority (higher outranks): from now on its shortfall may ask elastic leases of a
    /// lower priority to give back.
    pub fn prioritize(&mut self, ask: continuum_sage::place::AskId, priority: u8) {
        self.core.apply(&Act::Prioritize { ask, priority });
        self.tell();
    }
}

impl ResourceLeaseLedger {
    /// The core entry of `lease_id`, while it is held.
    pub fn ask_of(&self, lease_id: &str) -> Option<continuum_sage::place::AskId> {
        self.leases.get(lease_id).map(|held| held.ask)
    }

    /// Whether `ask` is still a held lease.
    pub fn holds_ask(&self, ask: continuum_sage::place::AskId) -> bool {
        self.leases.values().any(|held| held.ask == ask)
    }
}
