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
    /// Then grant every waiting ask that now fits; a grant can change a floor, so tell again until none is.
    pub(super) fn tell(&mut self) {
        loop {
            self.tell_place();
            if !self.grant_asks() {
                return;
            }
        }
    }

    fn tell_place(&mut self) {
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
        // a floor is met by what its holder is granted or measured to hold, whichever is more: measured
        // residency is already in the reading, so withholding the whole floor on top counted it twice
        // (Codex, 2026-09-27: a build reserving 10.5 GiB, resident at 6 GiB, cost serving 16.5 GiB). So
        // place is told the floor less its holder's unleased residency; its leases fill the rest
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

    /// What `consumer` replacing itself on `kind` would give back on `kind` and on the pool it draws on:
    /// the residency it measures there leaves what is resident (but never below what the others measure on
    /// that memory: a footprint estimate can exceed the reading), and its leases up to that residency stop
    /// being held. `place` answers the room from it ([`Ledger::room_if`]).
    ///
    /// [`Ledger::room_if`]: continuum_sage::place::Ledger::room_if
    pub(super) fn freed_by_replacing(&self, consumer: &str, kind: ResourceKind) -> Vec<(ResourceKind, Bytes)> {
        let resident = self.measured_by(consumer, kind);
        let released = self.granted_to(consumer, kind).min(resident);
        let its_landed = self
            .leases
            .values()
            .filter(|held| held.lease.kind == kind && held.lease.consumer_id == consumer)
            .map(|held| held.lease.bytes.saturating_sub(held.pending(self.observed_for(kind))))
            .fold(0u64, u64::saturating_add);
        let axes = std::iter::once(kind).chain(self.backing.get(&kind).copied());
        axes.map(|axis| {
            let kinds = self.pool_kinds_drawing_on(axis);
            let reading = match self.backing.get(&axis) {
                Some(_) => self.measured(axis),
                None => self.physical_used(axis),
            };
            let retained = self
                .measured
                .iter()
                .flat_map(|(owner, footprints)| { let kinds = &kinds; footprints.iter().filter(move |f| kinds.contains(&f.kind) && (owner != consumer || f.kind != kind)) })
                .map(|f| f.bytes)
                .fold(0u64, u64::saturating_add);
            let landed = kinds.iter().map(|k| self.landed_on(*k)).fold(0u64, u64::saturating_add);
            let after = reading.saturating_sub(resident).max(retained).saturating_sub(landed.saturating_sub(its_landed));
            (axis, Bytes(self.external(axis).saturating_sub(after).saturating_add(released)))
        })
        .collect()
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
