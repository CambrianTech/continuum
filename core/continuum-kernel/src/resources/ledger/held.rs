//! The authority's held leases, by lease id and by their entry in the core (Fable on #6287 at 8ffcc4d05c: the
//! shrink pass and the holder sweep each scanned every lease on the tick, under the governor lock). Both
//! indexes change only together, in `insert` and `remove`; reads see the lease map.

use std::collections::{BTreeMap, HashMap};

use continuum_sage::place::AskId;

use super::Held;

#[derive(Debug, Default)]
pub(super) struct HeldLeases {
    by_id: BTreeMap<String, Held>,
    by_ask: HashMap<AskId, String>,
}

impl HeldLeases {
    pub(super) fn insert(&mut self, lease_id: &str, held: Held) {
        // callers refuse a duplicate id first (LeaseError::DuplicateLease); a replaced entry still leaves no
        // stale index behind
        self.remove(lease_id);
        self.by_ask.insert(held.ask, lease_id.to_owned());
        self.by_id.insert(lease_id.to_owned(), held);
    }

    pub(super) fn remove(&mut self, lease_id: &str) -> Option<Held> {
        let held = self.by_id.remove(lease_id)?;
        self.by_ask.remove(&held.ask);
        Some(held)
    }

    /// Mutable access to a lease's accounting; its entry in the core is fixed for its life.
    pub(super) fn get_mut(&mut self, lease_id: &str) -> Option<&mut Held> {
        self.by_id.get_mut(lease_id)
    }

    /// The lease held under core entry `ask`, with its id.
    pub(super) fn by_ask(&self, ask: AskId) -> Option<(&String, &Held)> {
        let lease_id = self.by_ask.get(&ask)?;
        self.by_id.get_key_value(lease_id)
    }
}

impl std::ops::Deref for HeldLeases {
    type Target = BTreeMap<String, Held>;

    fn deref(&self) -> &Self::Target {
        &self.by_id
    }
}
