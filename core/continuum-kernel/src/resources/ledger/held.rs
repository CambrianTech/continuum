//! The authority's held leases, by lease id and by their entry in the core (Fable on #6287 at 8ffcc4d05c: the
//! shrink pass and the holder sweep each scanned every lease on the tick, under the governor lock). Both
//! indexes change only together, in `insert` and `remove`; reads see the lease map.

use std::collections::{BTreeMap, HashMap};

use continuum_sage::place::AskId;

use super::super::lease::ResourceLease;
use super::super::observed::Observed;
use super::LeaseCoverage;

/// A live lease and when its holder declared it landed (Codex on #6124). Landing is never inferred from
/// the holder's growth: growth beside a lease (the same owner's KV, another process) cannot tell which bytes
/// are the lease's. A lease is owed in full on top of physical use until its holder declares it resident
/// AND a physical sample taken after that declaration has been accepted (before it, the reading cannot
/// include the bytes). A holder that never declares is over-counted (refuses), never under-counted.
#[derive(Debug, Clone)]
pub(super) struct Held {
    /// Everything of the lease but its size: the size is the core's (`PlacerCore::holding`), read on demand,
    /// never kept here as a second record (step 1, ONE-FRAME-ONE-MEMORY.md).
    pub(super) meta: LeaseMeta,
    pub(super) view: Option<std::sync::Arc<LeaseCoverage>>,
    /// When its holder declared it landed: only a reading observed at or after this can show it. None while
    /// undeclared.
    pub(super) landed_from: Option<Observed>,
    /// Its entry in the core, by which it is committed, shrunk, split and released. Private to this file and
    /// fixed for the lease's life: `HeldLeases` indexes leases by it (Fable on #6287 at d8fbe6bc5c).
    ask: AskId,
}

/// A lease's identity, owner and terms: what the core does not hold. Its bytes are never here.
#[derive(Debug, Clone)]
pub(super) struct LeaseMeta {
    pub(super) lease_id: String,
    pub(super) consumer_id: String,
    pub(super) kind: super::super::lease::ResourceKind,
    pub(super) acquired_at_ms: u64,
    pub(super) expires_at_ms: u64,
    pub(super) reclaim_policy: super::super::lease::ReclaimPolicy,
}

impl LeaseMeta {
    pub(super) fn is_expired(&self, now_ms: u64) -> bool {
        super::super::lease::expired(self.expires_at_ms, now_ms)
    }

    /// The lease as a caller sees it, at the size the core holds for it now.
    pub(super) fn with_bytes(&self, bytes: u64) -> ResourceLease {
        ResourceLease {
            lease_id: self.lease_id.clone(),
            consumer_id: self.consumer_id.clone(),
            kind: self.kind,
            bytes,
            acquired_at_ms: self.acquired_at_ms,
            expires_at_ms: self.expires_at_ms,
            reclaim_policy: self.reclaim_policy,
        }
    }
}

impl Held {
    /// A lease just granted under core entry `ask`: no view yet, not landed. Its bytes are the core's.
    pub(super) fn new(lease: &ResourceLease, ask: AskId) -> Self {
        let meta = LeaseMeta {
            lease_id: lease.lease_id.clone(),
            consumer_id: lease.consumer_id.clone(),
            kind: lease.kind,
            acquired_at_ms: lease.acquired_at_ms,
            expires_at_ms: lease.expires_at_ms,
            reclaim_policy: lease.reclaim_policy,
        };
        Self { meta, view: None, landed_from: None, ask }
    }

    pub(super) fn ask(&self) -> AskId {
        self.ask
    }
}

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
