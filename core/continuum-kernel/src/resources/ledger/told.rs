//! What the authority tells its `place` ledger from its readings: each kind's capacity, its budget
//! (capacity less what is resident beyond our landed leases), which kind draws on which physical pool,
//! and each reservation as a floor. Told again after every reading or lease change, so `place` is the
//! one picture every grant and every "how much is free" reads (UNIFIED-PAGING.md slice 3a, the mapping
//! agreed with Fable and Codex on 2026-11-26).

use std::hash::{Hash, Hasher};

use continuum_sage::place::{Bytes, Owner};

use super::{ResourceKind, ResourceLeaseLedger};

impl ResourceLeaseLedger {
    /// Tell `place` everything it reads from: pools first (a budget is checked against them), then each
    /// kind's capacity and budget, then every floor.
    pub(super) fn tell(&mut self) {
        for kind in ResourceKind::ALL {
            let pool = self.backing.get(&kind).copied();
            self.place.back(kind, pool);
        }
        let mut never = Vec::new();
        for kind in ResourceKind::ALL {
            let capacity = self.capacity(kind);
            let budget = capacity.saturating_sub(self.external(kind));
            self.place.place(kind, Bytes(capacity), Bytes(budget), &mut never);
        }
        // a consumer's own unleased residency already fills its floor: the floor owed beyond its leases is
        // what that residency leaves (the kernel's floor less max(granted, measured))
        let floors: Vec<(Owner, ResourceKind, Bytes)> = self
            .reservations
            .iter()
            .map(|((kind, consumer), floor)| {
                let unleased = self.measured_by(consumer, *kind).saturating_sub(self.granted_to(consumer, *kind));
                (owner_of(consumer), *kind, Bytes(floor.saturating_sub(unleased)))
            })
            .collect();
        self.place.floors(floors);
    }

    /// Resident on `kind` beyond our leases proven landed in the accepted reading (Codex's rule: credit
    /// only the overlap of what is resident with live leases proven landed in that sample). A kind that
    /// draws on a pool reads its own residency from what our consumers measure on it (the host-wide
    /// reading counts every process); a pool, or a discrete device, reads its physical use, less what
    /// our landed leases on it and on every kind drawing on it hold.
    pub(super) fn external(&self, kind: ResourceKind) -> u64 {
        let resident = match self.backing.get(&kind) {
            Some(_) => self.measured(kind),
            None => self.physical_used(kind),
        };
        let landed = self.pool_kinds_drawing_on(kind).iter().map(|k| self.landed_on(*k)).fold(0u64, u64::saturating_add);
        resident.saturating_sub(landed)
    }

    /// `kind` and, when it is a pool, every kind drawing on it.
    fn pool_kinds_drawing_on(&self, kind: ResourceKind) -> Vec<ResourceKind> {
        if self.backing.contains_key(&kind) {
            return vec![kind];
        }
        std::iter::once(kind).chain(self.backing.iter().filter(|(_, pool)| **pool == kind).map(|(k, _)| *k)).collect()
    }

    /// The bytes of `kind`'s leases proven landed in its accepted reading.
    fn landed_on(&self, kind: ResourceKind) -> u64 {
        self.leases
            .values()
            .filter(|held| held.lease.kind == kind)
            .map(|held| held.lease.bytes.saturating_sub(held.pending(self.observed_for(kind))))
            .fold(0u64, u64::saturating_add)
    }
}

/// `consumer`'s owner on `place`: the same for its name every time, with no table to keep (step 3 of the
/// migration gives consumers a typed id and deletes this).
pub(super) fn owner_of(consumer: &str) -> Owner {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    consumer.hash(&mut hasher);
    Owner(hasher.finish())
}
