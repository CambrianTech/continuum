//! A request's LEASE on its mind's grant: the grant it was priced against, reserved at dispatch by
//! compare-and-reserve, and held by every regrant until the ENGINE'S terminal receipt for that
//! request, never merely until the caller lets go.
//!
//! Why (the M5, 0b2854327, 2026-11-05 01:25Z): Atlas's act was rendered against a 148,838 window;
//! 0.6 s later a re-home shrank her grant to 40,282 while she waited 26.3 s for a lane, and at
//! dispatch the engine counted 139,007 tokens and SENT them (3.45x her grant), because the only
//! count check compared against the slot's window. A recheck against a snapshot still races a
//! shrink (Codex), so the grant owner itself holds the reservation:
//!
//! - a reserve fails typed when her grant's identity moved since render or her exact count does
//!   not fit it (the caller re-renders, nothing is sent);
//! - a regrant takes every reserved grant off the pool FIRST, as a fixed claim, including a mind
//!   whose demand has dropped out, so the grants never sum past the pool ([`grant_with_reservations`]);
//! - custody: the reservation is released by the engine's TERMINAL RECEIPT ([`TerminalReceipt`]:
//!   its final frame with a finish reason, never a bare EOF; [`settle`]) or a FENCE. A caller that
//!   drops the request (cancellation, a transport error, a stream that ended without its finish)
//!   leaves it UNRESOLVED: the engine may still be decoding into that grant, so it stays held until
//!   explicit evidence ends it. A receipt releases ONLY its own entry: reservation ids are issued
//!   before the POST, so an id order proves nothing about the engine's execution order (an older
//!   request can be delayed in send or retry past a newer one's receipt). The evidence that ends an
//!   unresolved reservation today is the engine FENCE (its incarnation changed); a cancel receipt or
//!   a slot-drained observation from the engine are the follow-up that ends one sooner. Until then
//!   a cancelled request holds her grant fixed (never shrunk, never grown) until the fence;
//! - an attempt that was never sent (a connect refusal, a 503 before any token) releases its own
//!   reservation, and every retry re-reserves against her grant NOW, so a retry that lands on a
//!   replacement engine with a smaller grant is refused typed, never POSTed under the old lease
//!   ([`reserve_attempt`]).
//!
//! The lease is identified by its grant REVISION ([`super::GrantRevision`]), its engine incarnation
//! and its owner, never its size: a grant shrunk and regrown to the same size is a new revision.

use std::collections::BTreeMap;

use super::{Demand, GrantLease, WindowPolicy};
use crate::inference::engine_residency::EngineIncarnation;

/// Why a reservation was refused. Nothing was sent; the caller re-renders against her grant now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveRefused {
    /// Her grant's identity is no longer the one the request was priced against (`None`: withdrawn).
    Changed { leased: GrantLease, current: Option<GrantLease> },
    /// The engine's exact count plus the output reserve does not fit strictly inside the lease.
    Over { count: u32, grant: u32 },
}

/// One reservation's custody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Custody {
    /// Its request is in flight; the guard is held.
    Live,
    /// The caller let go before the engine's terminal receipt: held until a fence.
    Unresolved,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: u64,
    tokens: u32,
    engine: Option<EngineIncarnation>,
    custody: Custody,
}

/// The engine's terminal receipt for one request: its final frame carried a finish reason. A
/// transport EOF without one is NOT a receipt (the engine may still be decoding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalReceipt {
    pub finish: String,
}

/// The reservations, per mind.
pub(super) struct Reservations {
    held: std::sync::Mutex<(u64, BTreeMap<uuid::Uuid, Vec<Entry>>)>,
}

impl Reservations {
    pub(super) const fn new() -> Self {
        Self { held: std::sync::Mutex::new((0, BTreeMap::new())) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (u64, BTreeMap<uuid::Uuid, Vec<Entry>>)> {
        self.held.lock().unwrap_or_else(std::sync::PoisonError::into_inner) // unwrap_or_else: a poisoned map still holds every reservation; the data is plain values
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
        if !current.is_some_and(|lease| lease.same_identity(&leased)) {
            return Err(ReserveRefused::Changed { leased, current });
        }
        if needed >= leased.tokens {
            return Err(ReserveRefused::Over { count: needed, grant: leased.tokens });
        }
        held.0 += 1;
        let id = held.0;
        held.1.entry(persona).or_default().push(Entry { id, tokens: leased.tokens, engine: leased.engine, custody: Custody::Live });
        Ok(GrantReservation { owner: self, persona, id })
    }

    /// Run a regrant with every reservation held: the publisher computes and publishes under this
    /// lock, so a reserve either sees the new grant or the regrant sees the reservation. A
    /// reservation on another engine incarnation than `engine` is FENCED first: that engine is
    /// gone, so nothing of it can still be decoding into this pool.
    fn honoring<R>(&self, engine: Option<EngineIncarnation>, regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
        let mut held = self.lock();
        for entries in held.1.values_mut() {
            entries.retain(|entry| entry.engine == engine);
        }
        held.1.retain(|_, entries| !entries.is_empty());
        let reserved = held
            .1
            .iter()
            .map(|(mind, entries)| (*mind, entries.iter().map(|e| e.tokens).max().unwrap_or(0))) // unwrap_or: an empty list was just removed, so a mind here holds at least one entry
            .collect();
        regrant(&reserved)
    }

    /// Reservation `id` ends (its own terminal receipt, or an attempt never sent): its own entry
    /// only. Another request's reservation, older or newer, ends on its own evidence.
    fn release(&self, persona: uuid::Uuid, id: u64) {
        let mut held = self.lock();
        if let Some(entries) = held.1.get_mut(&persona) {
            entries.retain(|e| e.id != id);
            if entries.is_empty() {
                held.1.remove(&persona);
            }
        }
    }

    /// The caller let go of reservation `id` with no terminal receipt: it stays held, unresolved.
    fn abandon(&self, persona: uuid::Uuid, id: u64) {
        let mut held = self.lock();
        if let Some(entry) = held.1.get_mut(&persona).and_then(|entries| entries.iter_mut().find(|e| e.id == id)) {
            entry.custody = Custody::Unresolved;
        }
    }
}

/// A request's reservation of its lease. [`Self::complete`] on the engine's terminal receipt;
/// dropped without it, the reservation stays held, unresolved, until a fence.
#[must_use = "a reservation released at once protects nothing"]
pub struct GrantReservation {
    owner: &'static Reservations,
    persona: uuid::Uuid,
    id: u64,
}

impl GrantReservation {
    /// The engine's terminal receipt for this request: its own reservation ends.
    fn complete(self) {
        self.owner.release(self.persona, self.id);
        std::mem::forget(self);
    }

    /// The attempt was never sent (refused before any token): its own reservation ends.
    fn release_unsent(self) {
        self.owner.release(self.persona, self.id);
        std::mem::forget(self);
    }
}

/// Settle a request's reservation at the end of its stream: released on the engine's terminal
/// receipt, held unresolved without one (a bare EOF, a stream cut short).
pub fn settle(reservation: GrantReservation, receipt: Option<&TerminalReceipt>) {
    match receipt {
        Some(_) => reservation.complete(),
        None => drop(reservation),
    }
}

impl Drop for GrantReservation {
    fn drop(&mut self) {
        self.owner.abandon(self.persona, self.id);
    }
}

/// The node's reservations, beside the grants they lease from.
static RESERVATIONS: Reservations = Reservations::new();

/// Run a regrant (compute and publish) on engine incarnation `engine` with every reservation held:
/// a reserve either sees the new grant or the regrant sees the reservation.
pub fn honoring_reservations<R>(engine: Option<EngineIncarnation>, regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
    RESERVATIONS.honoring(engine, regrant)
}

/// One dispatch ATTEMPT's reservation, re-made before every POST: the previous attempt's (never
/// sent: a connect refusal or a 503 before any token) is released, and her lease is compared and
/// reserved against her grant as published NOW, so a retry that reaches a replacement engine or a
/// shrunken grant is refused typed and nothing is POSTed under the old lease.
pub fn reserve_attempt(
    slot_for: &mut Option<GrantReservation>,
    persona: uuid::Uuid,
    leased: GrantLease,
    needed: u32,
) -> Result<(), ReserveRefused> {
    reserve_attempt_with(&RESERVATIONS, slot_for, persona, leased, needed, || super::current_lease(persona))
}

fn reserve_attempt_with(
    owner: &'static Reservations,
    slot_for: &mut Option<GrantReservation>,
    persona: uuid::Uuid,
    leased: GrantLease,
    needed: u32,
    current: impl FnOnce() -> Option<GrantLease>,
) -> Result<(), ReserveRefused> {
    if let Some(previous) = slot_for.take() {
        previous.release_unsent();
    }
    *slot_for = Some(owner.reserve_against(persona, leased, needed, current)?);
    Ok(())
}

/// A regrant with the reservations as FIXED claims taken off the pool first: every reserved mind
/// keeps exactly her reserved grant (whether or not her demand is still listed), and the policy
/// shares only what they leave among the rest. The grants never sum past the pool.
pub fn grant_with_reservations<K: Ord + Copy, P: WindowPolicy<K>>(
    policy: &mut P,
    demands: &[Demand<K>],
    held: &BTreeMap<K, u32>,
    pool_tokens: u32,
    reserved: &BTreeMap<K, u32>,
) -> BTreeMap<K, u32> {
    let fixed = reserved.values().fold(0u32, |sum, tokens| sum.saturating_add(*tokens));
    let open: Vec<Demand<K>> = demands.iter().filter(|d| !reserved.contains_key(&d.mind)).copied().collect();
    let mut grants = policy.grant(&open, held, pool_tokens.saturating_sub(fixed));
    grants.extend(reserved.iter().map(|(mind, tokens)| (*mind, *tokens)));
    grants
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{stamp_revisions, DemandFollowing, GrantOwner, PoolGrants};
    use crate::inference::planning::window_allocator::test_tokens as t;

    fn owner() -> &'static Reservations {
        Box::leak(Box::new(Reservations::new()))
    }

    fn engine(pid: u32) -> Option<EngineIncarnation> {
        Some(EngineIncarnation { pid, started_s: 1_762_300_000, port: 8_085 })
    }

    /// The grants a publish would carry: `atlas` at `tokens` on `engine`, stamped against `current`.
    fn publish(current: Option<&PoolGrants>, atlas: uuid::Uuid, tokens: u32, on: Option<EngineIncarnation>) -> PoolGrants {
        let mut next = PoolGrants { by_mind: BTreeMap::from([(atlas, tokens)]), engine: on, ..Default::default() };
        stamp_revisions(current, &mut next);
        next
    }

    fn reserved_now(owner: &Reservations, on: Option<EngineIncarnation>) -> BTreeMap<uuid::Uuid, u32> {
        owner.honoring(on, |reserved| reserved.clone())
    }

    // what this catches: Atlas on the M5 (0b2854327): a request priced at 148,838 dispatched after a
    // re-home shrank her grant to 40,282, sent at 139,007 counted tokens. (a) A shrink during the
    // lane wait refuses the reserve typed (re-render, nothing sent), and so does the ABA case:
    // shrunk and regrown to the SAME size is a new revision; so does another engine incarnation
    // (the same pid restarted) and another owner. A count over the lease refuses typed with the count.
    #[test]
    fn a_moved_grant_even_regrown_to_its_size_or_a_count_over_it_refuses_typed() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let rev1 = publish(None, atlas, 148_838, engine(7));
        let rev1_again = publish(Some(&rev1), atlas, 148_838, engine(7));
        assert_eq!(rev1.lease_for(atlas), rev1_again.lease_for(atlas), "an unchanged grant keeps its revision");
        let rev2 = publish(Some(&rev1_again), atlas, 40_282, engine(7));
        let rev3 = publish(Some(&rev2), atlas, 148_838, engine(7));
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
        // The same pid restarted is another incarnation (its start time), never the same engine.
        let restarted = Some(EngineIncarnation { started_s: 1_762_400_000, ..engine(7).expect("an engine") });
        let relaunched = publish(Some(&rev3), atlas, 148_838, restarted).lease_for(atlas).expect("granted");
        assert!(!relaunched.same_identity(&now), "a new engine incarnation re-issues every revision");
        let foreign = GrantLease { owner: GrantOwner(uuid::Uuid::new_v4()), ..now };
        assert!(matches!(owner.reserve_against(atlas, foreign, 1, || Some(now)), Err(ReserveRefused::Changed { .. })), "another owner's lease is never this one's");
        let held = owner.reserve_against(atlas, now, 141_007, || Some(now)).expect("her lease still stands: reserved");
        held.complete();
    }

    // what this catches: (b) a shrink between the reserve and the send, and Codex's overcommit: pool
    // 100, A reserved at 60 and gone from the demands, B asking for 100. The reservation is a fixed
    // claim taken off the pool FIRST, so A keeps 60, B is granted from the 40 left, and the grants
    // never sum past the pool; (c) the engine's terminal receipt releases it and the shrink applies.
    #[test]
    fn a_reserved_grant_is_a_fixed_claim_inside_the_pool_until_its_terminal_receipt() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let owner = owner();
        let lease = publish(None, a, 60, engine(7)).lease_for(a).expect("granted");
        let reservation = owner.reserve_against(a, lease, 50, || Some(lease)).expect("reserved");
        let b_wants = Demand { mind: b, need: t(30), target: t(100), wish: t(100), in_flight: false, ceiling: 100 };
        let grants = owner.honoring(engine(7), |reserved| {
            grant_with_reservations(&mut DemandFollowing, &[b_wants], &BTreeMap::from([(a, 60)]), 100, reserved)
        });
        assert_eq!(grants.get(&a), Some(&60), "her reservation holds though her demand is gone");
        assert!(grants.values().sum::<u32>() <= 100, "never past the pool: {grants:?}");
        reservation.complete();
        let after = owner.honoring(engine(7), |reserved| {
            grant_with_reservations(&mut DemandFollowing, &[b_wants], &BTreeMap::new(), 100, reserved)
        });
        assert_eq!(after.get(&a), None, "released at its terminal receipt: the deferred change applies");
        assert_eq!(after.get(&b), Some(&100));
    }

    // ONE-ACT: fairness must use engine custody, not a sampled service-loop boolean. The
    // waiter cannot take the fixed claim before the receipt, but a completed holder must not
    // retain it indefinitely merely because she was granted first.
    #[test]
    #[ignore = "ONE-ACT slice 5: terminal receipt must hand an oversubscribed grant to its waiter"]
    fn a_waiter_inherits_capacity_only_after_the_holders_terminal_receipt() {
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let owner = owner();
        let lease = publish(None, a, 60, engine(7)).lease_for(a).expect("granted");
        let reservation = owner.reserve_against(a, lease, 50, || Some(lease)).expect("reserved");
        let demands = [a, b].map(|mind| Demand {
            mind, need: t(60), target: t(60), wish: t(60), in_flight: false, ceiling: 100,
        });
        let held = BTreeMap::from([(a, 60)]);
        let grants = owner.honoring(engine(7), |reserved| {
            grant_with_reservations(&mut DemandFollowing, &demands, &held, 100, reserved)
        });
        assert_eq!(grants.get(&a), Some(&60), "the engine still owns her reservation");
        assert_eq!(grants.get(&b), None, "a waiter cannot steal live capacity");
        assert!(grants.values().sum::<u32>() <= 100);
        settle(reservation, Some(&TerminalReceipt { finish: "stop".into() }));
        let after = owner.honoring(engine(7), |reserved| {
            assert!(reserved.is_empty(), "the holder has a verified terminal receipt");
            grant_with_reservations(&mut DemandFollowing, &demands, &held, 100, reserved)
        });
        assert!(after.values().sum::<u32>() <= 100);
        assert_eq!(after.get(&b), Some(&60), "a verified completion owes the waiting mind admission");
        assert_eq!(after.get(&a), None, "the old holder waits rather than overspending the pool");
    }

    // what this catches: a cancellation, or a stream that ended without its finish, taken for a
    // terminal receipt. Either leaves the reservation held, unresolved; a fence (the engine
    // incarnation changed) ends it.
    #[test]
    fn a_dropped_or_unfinished_stream_holds_the_grant_until_a_fence() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, atlas, 148_838, engine(7)).lease_for(atlas).expect("granted");
        let cancelled = owner.reserve_against(atlas, lease, 141_007, || Some(lease)).expect("reserved");
        drop(cancelled);
        assert_eq!(reserved_now(owner, engine(7)).get(&atlas), Some(&148_838), "unresolved: still held");
        let eof = owner.reserve_against(atlas, lease, 1_000, || Some(lease)).expect("reserved");
        settle(eof, None);
        assert_eq!(reserved_now(owner, engine(7)).get(&atlas), Some(&148_838), "an EOF without a finish is no receipt");
        assert_eq!(reserved_now(owner, engine(8)).get(&atlas), None, "a fence: the engine incarnation changed");
    }

    // what this catches (Codex on #5787): a newer request's receipt releasing an OLDER request that
    // was delayed in send or retry. Ids are issued before the POST, so they prove no engine order: a
    // receipt releases only its own entry, and the older one stays held until its own evidence.
    #[test]
    fn a_receipt_releases_only_its_own_reservation() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, atlas, 148_838, engine(7)).lease_for(atlas).expect("granted");
        let delayed = owner.reserve_against(atlas, lease, 1_000, || Some(lease)).expect("reserved first, sent late");
        let newer = owner.reserve_against(atlas, lease, 1_000, || Some(lease)).expect("reserved");
        settle(newer, Some(&TerminalReceipt { finish: "stop".into() }));
        assert_eq!(reserved_now(owner, engine(7)).get(&atlas), Some(&148_838), "the delayed older request stays held");
        settle(delayed, Some(&TerminalReceipt { finish: "stop".into() }));
        assert_eq!(reserved_now(owner, engine(7)).get(&atlas), None, "released by its own receipt");
    }

    // what this catches (Codex on #5787), at the reserve helper (the HTTP-level zero-POST pin is
    // `a_stale_lease_posts_nothing` in the openai adapter's tests): an attempt re-reserves before
    // every POST. Engine A was reserved; the retry's lease is compared against engine B's smaller
    // grant, refused typed, and nothing stays reserved under the old lease.
    #[test]
    fn the_reserve_helper_re_reserves_each_attempt_and_refuses_a_replacement_engine() {
        let atlas = uuid::Uuid::new_v4();
        let owner = owner();
        let on_a = publish(None, atlas, 148_838, engine(7));
        let lease_a = on_a.lease_for(atlas).expect("granted");
        let mut attempt = None;
        reserve_attempt_with(owner, &mut attempt, atlas, lease_a, 141_007, || Some(lease_a)).expect("the first attempt reserves");
        let on_b = publish(Some(&on_a), atlas, 40_282, engine(8)).lease_for(atlas).expect("granted");
        let retried = reserve_attempt_with(owner, &mut attempt, atlas, lease_a, 141_007, || Some(on_b));
        assert_eq!(retried, Err(ReserveRefused::Changed { leased: lease_a, current: Some(on_b) }), "refused: the old lease reserves nothing");
        assert!(attempt.is_none(), "no reservation carried to the retry");
        assert_eq!(reserved_now(owner, engine(7)).get(&atlas), None, "the unsent attempt released its own");
    }
}
