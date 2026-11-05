//! A request's LEASE on its mind's grant: the grant it was priced against, reserved at dispatch by
//! compare-and-reserve, and honored by every regrant until the request's terminal receipt.
//!
//! Why (the M5, 0b2854327, 2026-11-05 01:25Z): Atlas's act was rendered against a 148,838 window;
//! 0.6 s later a re-home shrank her grant to 40,282 while she waited 26.3 s for a lane, and at
//! dispatch the engine counted 139,007 tokens and SENT them (3.45x her grant), because the only
//! count check compared against the slot's window. A recheck against a snapshot still races a
//! shrink (Codex), so the grant owner itself holds the reservation: a reserve fails typed when her
//! grant moved since render or her exact count does not fit it (the caller re-renders, nothing is
//! sent), and a regrant never takes a reserved grant below what is reserved; the shrink applies at
//! the first publish after the reservation is released (its guard is dropped with the request).
//!
//! The lease is identified by the granted tokens: a grant that moved away and back to the same
//! size is the same entitlement, which is all the reservation protects.

use std::collections::BTreeMap;

/// Why a reservation was refused. Nothing was sent; the caller re-renders against her grant now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveRefused {
    /// Her grant is no longer the one the request was priced against (`None`: withdrawn).
    Changed { leased: u32, current: Option<u32> },
    /// The engine's exact count plus the output reserve does not fit strictly inside the lease.
    Over { count: u32, grant: u32 },
}

/// The reservations in flight, per mind: the largest leased grant and how many requests hold it.
pub struct Reservations {
    held: std::sync::Mutex<BTreeMap<uuid::Uuid, (u32, usize)>>,
}

impl Reservations {
    pub const fn new() -> Self {
        Self { held: std::sync::Mutex::new(BTreeMap::new()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<uuid::Uuid, (u32, usize)>> {
        self.held.lock().unwrap_or_else(std::sync::PoisonError::into_inner) // unwrap_or_else: a poisoned map still holds every reservation; the data is plain counts
    }

    /// Compare-and-reserve: `current` is her grant as the owner publishes it now, read UNDER this
    /// lock (`reserve` does), so no regrant can slip between the compare and the reserve.
    fn reserve_against(
        &'static self,
        persona: uuid::Uuid,
        leased: u32,
        needed: u32,
        current: impl FnOnce() -> Option<u32>,
    ) -> Result<GrantReservation, ReserveRefused> {
        let mut held = self.lock();
        let current = current();
        if current != Some(leased) {
            return Err(ReserveRefused::Changed { leased, current });
        }
        if needed >= leased {
            return Err(ReserveRefused::Over { count: needed, grant: leased });
        }
        let entry = held.entry(persona).or_insert((leased, 0));
        entry.0 = entry.0.max(leased);
        entry.1 += 1;
        Ok(GrantReservation { owner: self, persona })
    }

    /// Run a regrant with every reservation held: the publisher computes and publishes under this
    /// lock, so a reserve either sees the new grant or the regrant sees the reservation.
    pub fn honoring<R>(&self, regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
        let held = self.lock();
        let reserved: BTreeMap<uuid::Uuid, u32> = held.iter().map(|(mind, (tokens, _))| (*mind, *tokens)).collect();
        regrant(&reserved)
    }
}

/// A request's reservation of its lease; dropped with the request (its terminal receipt), which
/// releases it. The deferred shrink then applies at the next publish.
#[must_use = "a reservation released at once protects nothing"]
pub struct GrantReservation {
    owner: &'static Reservations,
    persona: uuid::Uuid,
}

impl Drop for GrantReservation {
    fn drop(&mut self) {
        let mut held = self.owner.lock();
        if let Some(entry) = held.get_mut(&self.persona) {
            entry.1 -= 1;
            if entry.1 == 0 {
                held.remove(&self.persona);
            }
        }
    }
}

/// The node's reservations, beside the grants they lease from.
pub static RESERVATIONS: Reservations = Reservations::new();

/// Reserve `persona`'s lease of `leased` tokens for a request that needs `needed` (the engine's
/// exact prompt count plus its output reserve), against her grant as published now.
pub fn reserve(persona: uuid::Uuid, leased: u32, needed: u32) -> Result<GrantReservation, ReserveRefused> {
    RESERVATIONS.reserve_against(persona, leased, needed, || super::current_grant(persona))
}

/// A regrant's result with every reserved grant held at least at its reservation: a mind whose
/// request is in flight never runs against a smaller entitlement, even when her demand dropped out.
pub fn honor(grants: &mut BTreeMap<uuid::Uuid, u32>, reserved: &BTreeMap<uuid::Uuid, u32>) {
    for (mind, tokens) in reserved {
        let grant = grants.entry(*mind).or_insert(0);
        *grant = (*grant).max(*tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> &'static Reservations {
        Box::leak(Box::new(Reservations::new()))
    }

    // what this catches: Atlas on the M5 (0b2854327): a request priced at 148,838 dispatched after a
    // re-home shrank her grant to 40,282, sent at 139,007 counted tokens. (a) A shrink during the
    // lane wait refuses the reserve typed (re-render, nothing sent); an exact count over the lease
    // refuses typed with the count; a lease that still holds reserves.
    #[test]
    fn a_shrink_before_dispatch_or_a_count_over_the_lease_refuses_typed() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let shrunk = owner.reserve_against(atlas, 148_838, 141_007, || Some(40_282));
        assert_eq!(shrunk.err(), Some(ReserveRefused::Changed { leased: 148_838, current: Some(40_282) }));
        let withdrawn = owner.reserve_against(atlas, 148_838, 141_007, || None);
        assert_eq!(withdrawn.err(), Some(ReserveRefused::Changed { leased: 148_838, current: None }));
        let over = owner.reserve_against(atlas, 40_282, 141_007, || Some(40_282));
        assert_eq!(over.err(), Some(ReserveRefused::Over { count: 141_007, grant: 40_282 }));
        let held = owner.reserve_against(atlas, 148_838, 141_007, || Some(148_838));
        assert!(held.is_ok(), "her lease still holds: reserved");
    }

    // what this catches: (b) a shrink between the reserve and the send. The regrant runs under the
    // reservations and is held at the reserved grant, so the request never runs against a smaller
    // entitlement; (c) the request's terminal receipt (the guard dropped) releases it and the
    // deferred shrink applies at the next regrant.
    #[test]
    fn a_reserved_grant_defers_a_shrink_until_its_request_ends() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let reservation = owner.reserve_against(atlas, 148_838, 141_007, || Some(148_838)).expect("reserved");
        let shrink = || BTreeMap::from([(atlas, 40_282u32)]);
        let during = owner.honoring(|reserved| {
            let mut grants = shrink();
            honor(&mut grants, reserved);
            grants
        });
        assert_eq!(during.get(&atlas), Some(&148_838), "the shrink waits for her request");
        let dropped_out = owner.honoring(|reserved| {
            let mut grants = BTreeMap::new();
            honor(&mut grants, reserved);
            grants
        });
        assert_eq!(dropped_out.get(&atlas), Some(&148_838), "even with her demand gone, the lease holds");
        drop(reservation);
        let after = owner.honoring(|reserved| {
            let mut grants = shrink();
            honor(&mut grants, reserved);
            grants
        });
        assert_eq!(after.get(&atlas), Some(&40_282), "released at its terminal receipt: the deferred shrink applies");
    }
}
