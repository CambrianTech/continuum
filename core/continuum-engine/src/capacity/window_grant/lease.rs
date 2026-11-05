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
//! The lease is identified by its grant REVISION ([`super::GrantRevision`]), never its size: a grant
//! shrunk and regrown to the same size is a new revision, so a request priced before the shrink is
//! refused (the ABA case a size compare would pass).

use std::collections::BTreeMap;

use super::GrantLease;

/// Why a reservation was refused. Nothing was sent; the caller re-renders against her grant now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveRefused {
    /// Her grant's revision is no longer the one the request was priced against (`None`: withdrawn).
    Changed { leased: GrantLease, current: Option<GrantLease> },
    /// The engine's exact count plus the output reserve does not fit strictly inside the lease.
    Over { count: u32, grant: u32 },
}

/// The reservations in flight, per mind: the largest leased grant and how many requests hold it.
pub(super) struct Reservations {
    held: std::sync::Mutex<BTreeMap<uuid::Uuid, (u32, usize)>>,
}

impl Reservations {
    pub(super) const fn new() -> Self {
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
        leased: GrantLease,
        needed: u32,
        current: impl FnOnce() -> Option<GrantLease>,
    ) -> Result<GrantReservation, ReserveRefused> {
        let mut held = self.lock();
        let current = current();
        if current.map(|lease| lease.revision) != Some(leased.revision) {
            return Err(ReserveRefused::Changed { leased, current });
        }
        if needed >= leased.tokens {
            return Err(ReserveRefused::Over { count: needed, grant: leased.tokens });
        }
        let entry = held.entry(persona).or_insert((leased.tokens, 0));
        entry.0 = entry.0.max(leased.tokens);
        entry.1 += 1;
        Ok(GrantReservation { owner: self, persona })
    }

    /// Run a regrant with every reservation held: the publisher computes and publishes under this
    /// lock, so a reserve either sees the new grant or the regrant sees the reservation.
    fn honoring<R>(&self, regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
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
static RESERVATIONS: Reservations = Reservations::new();

/// Run a regrant (compute and publish) with every reservation held: a reserve either sees the new
/// grant or the regrant sees the reservation, and a reserved grant is never taken below it.
pub fn honoring_reservations<R>(regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
    RESERVATIONS.honoring(regrant)
}

/// Reserve `persona`'s lease of `leased` tokens for a request that needs `needed` (the engine's
/// exact prompt count plus its output reserve), against her grant as published now.
pub fn reserve(persona: uuid::Uuid, leased: GrantLease, needed: u32) -> Result<GrantReservation, ReserveRefused> {
    RESERVATIONS.reserve_against(persona, leased, needed, || super::current_lease(persona))
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
    use super::super::{stamp_revisions, PoolGrants};

    fn owner() -> &'static Reservations {
        Box::leak(Box::new(Reservations::new()))
    }

    /// The grants a publish would carry: `atlas` at `tokens` on engine `engine`, stamped against `current`.
    fn publish(current: Option<&PoolGrants>, atlas: uuid::Uuid, tokens: u32, engine: u32) -> PoolGrants {
        let mut next = PoolGrants { by_mind: BTreeMap::from([(atlas, tokens)]), engine: Some(engine), ..Default::default() };
        stamp_revisions(current, &mut next);
        next
    }

    // what this catches: Atlas on the M5 (0b2854327): a request priced at 148,838 dispatched after a
    // re-home shrank her grant to 40,282, sent at 139,007 counted tokens. (a) A shrink during the
    // lane wait refuses the reserve typed (re-render, nothing sent), and so does the ABA case:
    // shrunk and regrown to the SAME size is a new revision. A count over the lease refuses typed
    // with the count; a lease whose revision still stands reserves.
    #[test]
    fn a_moved_grant_even_regrown_to_its_size_or_a_count_over_it_refuses_typed() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let rev1 = publish(None, atlas, 148_838, 7);
        let rev1_again = publish(Some(&rev1), atlas, 148_838, 7);
        assert_eq!(rev1.lease_for(atlas), rev1_again.lease_for(atlas), "an unchanged grant keeps its revision");
        let rev2 = publish(Some(&rev1_again), atlas, 40_282, 7);
        let rev3 = publish(Some(&rev2), atlas, 148_838, 7);
        let priced = rev1.lease_for(atlas).expect("granted");
        let now = rev3.lease_for(atlas).expect("granted");
        assert_eq!(now.tokens, priced.tokens, "precondition: the same size again");
        let aba = owner.reserve_against(atlas, priced, 141_007, || Some(now));
        assert_eq!(aba.err(), Some(ReserveRefused::Changed { leased: priced, current: Some(now) }), "a new revision, refused");
        let shrunk = owner.reserve_against(atlas, priced, 141_007, || rev2.lease_for(atlas));
        assert!(matches!(shrunk, Err(ReserveRefused::Changed { .. })));
        let withdrawn = owner.reserve_against(atlas, priced, 141_007, || None);
        assert_eq!(withdrawn.err(), Some(ReserveRefused::Changed { leased: priced, current: None }));
        let small = rev2.lease_for(atlas).expect("granted");
        let over = owner.reserve_against(atlas, small, 141_007, || Some(small));
        assert_eq!(over.err(), Some(ReserveRefused::Over { count: 141_007, grant: 40_282 }));
        let held = owner.reserve_against(atlas, now, 141_007, || Some(now));
        assert!(held.is_ok(), "her lease's revision still stands: reserved");
        let relaunched = publish(Some(&rev3), atlas, 148_838, 8);
        assert_ne!(relaunched.lease_for(atlas), Some(now), "a new engine incarnation re-issues every revision");
    }

    // what this catches: (b) a shrink between the reserve and the send. The regrant runs under the
    // reservations and is held at the reserved grant, so the request never runs against a smaller
    // entitlement; (c) the request's terminal receipt (the guard dropped) releases it and the
    // deferred shrink applies at the next regrant.
    #[test]
    fn a_reserved_grant_defers_a_shrink_until_its_request_ends() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, atlas, 148_838, 7).lease_for(atlas).expect("granted");
        let reservation = owner.reserve_against(atlas, lease, 141_007, || Some(lease)).expect("reserved");
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
