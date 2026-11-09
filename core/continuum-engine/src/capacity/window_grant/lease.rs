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

mod fair;
pub use fair::{publish_fair_grants, FairGrantState};

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
    /// A verified completion began custody transfer; the old holder cannot reacquire.
    Retiring { leased: GrantLease },
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

/// Owned authority to reclaim exactly one publication. Dropping it does not release the
/// charged claim. Physical reclamation must carry it through endpoint and slot custody.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a handoff ticket must remain owned until verified physical reclamation or fencing"]
pub struct GrantHandoff {
    persona: uuid::Uuid,
    lease: GrantLease,
    receipt: u64,
}

impl GrantHandoff {
    #[cfg(test)]
    pub(crate) fn for_test(persona: uuid::Uuid) -> Self {
        Self { persona, lease: GrantLease { tokens: 60, revision: super::GrantRevision::test(1),
            engine: None, owner: super::GrantOwner::this_process() }, receipt: 1 }
    }
    pub fn persona(&self) -> uuid::Uuid { self.persona }
    pub fn lease(&self) -> GrantLease { self.lease }
}

/// Why a proposed transfer cannot begin. None authorizes freeing cached KV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffRefused {
    GrantChanged,
    ReceiptChanged,
    CustodyOutstanding,
    AlreadyPending,
}

/// Engine-terminal progress survives a finish/restart between publisher ticks.
/// A revision is issued only for a verified terminal receipt of an owned request.
#[derive(Default)]
struct ReservationState {
    next_id: u64,
    entries: BTreeMap<uuid::Uuid, Vec<Entry>>,
    next_receipt: u64,
    receipts: BTreeMap<uuid::Uuid, (Option<EngineIncarnation>, u64)>,
    retiring: BTreeMap<uuid::Uuid, GrantLease>,
    reclaimed: BTreeMap<uuid::Uuid, GrantLease>,
    refused: BTreeMap<uuid::Uuid, GrantLease>,
    /// A queued waiter is owed the holder's next completed turn. This blocks reserve at the
    /// receipt boundary, rather than hoping the publisher catches an idle interval.
    yield_after: BTreeMap<uuid::Uuid, (GrantLease, u64)>,
}

/// The reservations and terminal progress share one lock: a publisher cannot observe a
/// completion without observing the reservation it released (or vice versa).
pub(super) struct Reservations {
    held: std::sync::Mutex<ReservationState>,
}

impl Reservations {
    pub(super) const fn new() -> Self {
        Self { held: std::sync::Mutex::new(ReservationState {
            next_id: 0, entries: BTreeMap::new(), next_receipt: 0, receipts: BTreeMap::new(), retiring: BTreeMap::new(), reclaimed: BTreeMap::new(), refused: BTreeMap::new(), yield_after: BTreeMap::new(),
        }) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ReservationState> {
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
        if held.retiring.get(&persona).is_some_and(|retired| retired.engine == leased.engine) {
            return Err(ReserveRefused::Retiring { leased });
        }
        if held.yield_after.get(&persona).is_some_and(|(armed, baseline)| {
            armed.same_identity(&leased) && held.receipts.get(&persona)
                .is_some_and(|(engine, revision)| *engine == leased.engine && revision > baseline)
        }) {
            return Err(ReserveRefused::Retiring { leased });
        }
        if needed >= leased.tokens {
            return Err(ReserveRefused::Over { count: needed, grant: leased.tokens });
        }
        held.next_id += 1;
        let id = held.next_id;
        held.entries.entry(persona).or_default().push(Entry { id, tokens: leased.tokens, engine: leased.engine, custody: Custody::Live });
        Ok(GrantReservation { owner: self, persona, id })
    }

    /// Run a regrant with every reservation held: the publisher computes and publishes under this
    /// lock, so a reserve either sees the new grant or the regrant sees the reservation. A
    /// reservation on another engine incarnation than `engine` is FENCED first: that engine is
    /// gone, so nothing of it can still be decoding into this pool.
    fn honoring<R>(&self, engine: Option<EngineIncarnation>, regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>) -> R) -> R {
        self.honoring_with_receipts(engine, |reserved, _| regrant(reserved))
    }

    fn honoring_with_receipts<R>(
        &self,
        engine: Option<EngineIncarnation>,
        regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>, &BTreeMap<uuid::Uuid, u64>) -> R,
    ) -> R {
        let mut held = self.lock();
        for entries in held.entries.values_mut() {
            entries.retain(|entry| entry.engine == engine);
        }
        held.entries.retain(|_, entries| !entries.is_empty());
        held.receipts.retain(|_, (incarnation, _)| *incarnation == engine);
        held.retiring.retain(|_, lease| lease.engine == engine);
        held.reclaimed.retain(|_, lease| lease.engine == engine);
        held.refused.retain(|_, lease| lease.engine == engine);
        held.yield_after.retain(|_, (lease, _)| lease.engine == engine);
        let mut reserved: BTreeMap<uuid::Uuid, u32> = held.entries.iter().map(|(mind, entries)| {
            (*mind, entries.iter().map(|e| e.tokens).max().unwrap_or(0)) // unwrap_or: empty entries were removed above
        }).collect();
        for (mind, lease) in &held.retiring {
            reserved.entry(*mind).and_modify(|tokens| *tokens = (*tokens).max(lease.tokens)).or_insert(lease.tokens);
        }
        let receipts = held.receipts.iter().map(|(mind, (_, revision))| (*mind, *revision)).collect();
        regrant(&reserved, &receipts)
    }

    /// Start transfer only from the exact observed publication and terminal revision, with no
    /// request still owning custody. The claim stays charged until physical reclamation or fence.
    fn begin_handoff(
        &self,
        persona: uuid::Uuid,
        leased: GrantLease,
        receipt: u64,
        current: impl FnOnce() -> Option<GrantLease>,
    ) -> Result<GrantHandoff, HandoffRefused> {
        let mut held = self.lock();
        if leased.engine.is_none() || !current().is_some_and(|now| now.same_identity(&leased)) {
            return Err(HandoffRefused::GrantChanged);
        }
        if held.receipts.get(&persona) != Some(&(leased.engine, receipt)) {
            return Err(HandoffRefused::ReceiptChanged);
        }
        if held.entries.get(&persona).is_some_and(|entries| !entries.is_empty()) {
            return Err(HandoffRefused::CustodyOutstanding);
        }
        if held.retiring.contains_key(&persona) {
            return Err(HandoffRefused::AlreadyPending);
        }
        held.retiring.insert(persona, leased);
        Ok(GrantHandoff { persona, lease: leased, receipt })
    }

    /// Release only this request. An unsent attempt is not completed service. A receipt from
    /// an already-fenced request cannot create progress in the replacement engine.
    fn release(&self, persona: uuid::Uuid, id: u64, terminal: bool) {
        let mut held = self.lock();
        let released = held.entries.get_mut(&persona).and_then(|entries| {
            let index = entries.iter().position(|e| e.id == id)?;
            Some(entries.remove(index))
        });
        if held.entries.get(&persona).is_some_and(Vec::is_empty) {
            held.entries.remove(&persona);
        }
        if let (true, Some(entry)) = (terminal, released) {
            held.next_receipt += 1;
            let revision = held.next_receipt;
            held.receipts.insert(persona, (entry.engine, revision));
        }
    }

    /// The caller let go of reservation `id` with no terminal receipt: it stays held, unresolved.
    fn abandon(&self, persona: uuid::Uuid, id: u64) {
        let mut held = self.lock();
        if let Some(entry) = held.entries.get_mut(&persona).and_then(|entries| entries.iter_mut().find(|e| e.id == id)) {
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
        self.owner.release(self.persona, self.id, true);
        std::mem::forget(self);
    }

    /// The attempt was never sent (refused before any token): its own reservation ends.
    fn release_unsent(self) {
        self.owner.release(self.persona, self.id, false);
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

/// Read fixed claims and per-mind terminal receipt revisions in one publication transaction.
/// Revisions remain visible across publisher ticks, including when another request has already
/// started. They are scoped to the engine incarnation and are NOT proof that cached KV was
/// saved or freed: physical custody must still be established before reallocating those cells.
pub fn honoring_reservations_and_receipts<R>(
    engine: Option<EngineIncarnation>,
    regrant: impl FnOnce(&BTreeMap<uuid::Uuid, u32>, &BTreeMap<uuid::Uuid, u64>) -> R,
) -> R {
    RESERVATIONS.honoring_with_receipts(engine, regrant)
}

/// Fence the old holder's next dispatch while preserving its charged capacity. This is the
/// first phase of handoff, NOT a grant to a waiter. There is deliberately no unverified
/// "release retirement" API: the physical save-and-erase receipt integration is still required.
pub fn begin_grant_handoff(persona: uuid::Uuid, leased: GrantLease, receipt: u64) -> Result<GrantHandoff, HandoffRefused> {
    RESERVATIONS.begin_handoff(persona, leased, receipt, || super::current_lease(persona))
}

/// A physical proof is retained even if the operation's requester disappeared. The charged
/// retirement is NOT released here: the publisher must consume it with the next grant update.
pub(crate) fn record_reclaimed_grant(proof: crate::inference::slots::grant_handoff::ReclaimedGrant) -> Result<(), String> {
    RESERVATIONS.record_reclaimed(proof)
}

impl Reservations {
fn record_reclaimed(&self, proof: crate::inference::slots::grant_handoff::ReclaimedGrant) -> Result<(), String> {
    let ticket = proof.into_ticket();
    let mut state = self.lock();
    if !state.retiring.get(&ticket.persona).is_some_and(|lease| lease.same_identity(&ticket.lease)) {
        return Err("reclaimed grant was already fenced or replaced".into());
    }
    state.reclaimed.insert(ticket.persona, ticket.lease);
    Ok(())
}
}

pub(crate) fn is_grant_retiring(persona: uuid::Uuid) -> bool {
    let state = RESERVATIONS.lock();
    state.retiring.contains_key(&persona) || state.yield_after.get(&persona).is_some_and(|(lease, baseline)| {
        state.receipts.get(&persona).is_some_and(|(engine, revision)| *engine == lease.engine && revision > baseline)
    })
}

/// A refused transfer returns authority, never free capacity. The publisher must restore
/// the charged holder atomically before allowing another dispatch.
pub(crate) fn record_refused_grant(proof: crate::inference::slots::grant_handoff::RefusedGrant) -> Result<(), String> {
    let ticket = proof.into_ticket();
    let mut state = RESERVATIONS.lock();
    if !state.retiring.get(&ticket.persona).is_some_and(|lease| lease.same_identity(&ticket.lease)) {
        return Err("refused grant was already fenced or replaced".into());
    }
    state.refused.insert(ticket.persona, ticket.lease);
    Ok(())
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

    #[test]
    fn pending_handoff_blocks_reacquisition_but_keeps_physical_capacity_charged() {
        let mind = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, mind, 60, engine(7)).lease_for(mind).expect("granted");
        let active = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        settle(active, Some(&TerminalReceipt { finish: "stop".into() }));
        let revision = owner.honoring_with_receipts(engine(7), |_, receipts| receipts[&mind]);
        let ticket = owner.begin_handoff(mind, lease, revision, || Some(lease)).expect("handoff starts");
        assert_eq!(ticket.persona(), mind);
        assert_eq!(ticket.lease(), lease);
        assert_eq!(owner.begin_handoff(mind, lease, revision, || Some(lease)), Err(HandoffRefused::AlreadyPending));
        drop(ticket); // cancellation must not return physically occupied cells

        let again = owner.reserve_against(mind, lease, 50, || Some(lease));
        assert_eq!(again.err(), Some(ReserveRefused::Retiring { leased: lease }));
        owner.honoring_with_receipts(engine(7), |fixed, _| {
            assert_eq!(fixed.get(&mind), Some(&60), "terminal receipt did not erase cached KV");
        });
        owner.honoring_with_receipts(engine(8), |fixed, receipts| {
            assert!(fixed.is_empty(), "replacement engine fences physical custody");
            assert!(receipts.is_empty());
        });
        let replacement = publish(None, mind, 60, engine(8)).lease_for(mind).expect("new engine grant");
        owner.reserve_against(mind, replacement, 50, || Some(replacement)).expect("not blocked by old retirement").release_unsent();
    }

    #[test]
    fn handoff_revalidates_receipt_publication_and_unresolved_custody() {
        let mind = uuid::Uuid::new_v4();
        let owner = owner();
        let grant = publish(None, mind, 60, engine(7));
        let lease = grant.lease_for(mind).expect("granted");
        let finished = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        settle(finished, Some(&TerminalReceipt { finish: "stop".into() }));
        let revision = owner.honoring_with_receipts(engine(7), |_, receipts| receipts[&mind]);
        assert_eq!(owner.begin_handoff(mind, lease, revision + 1, || Some(lease)), Err(HandoffRefused::ReceiptChanged));
        let changed = publish(Some(&grant), mind, 70, engine(7)).lease_for(mind).expect("changed");
        assert_eq!(owner.begin_handoff(mind, lease, revision, || Some(changed)), Err(HandoffRefused::GrantChanged));
        let overlapping = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("second request");
        assert_eq!(owner.begin_handoff(mind, lease, revision, || Some(lease)), Err(HandoffRefused::CustodyOutstanding));
        drop(overlapping);
        assert_eq!(owner.begin_handoff(mind, lease, revision, || Some(lease)), Err(HandoffRefused::CustodyOutstanding));
        assert_eq!(reserved_now(owner, engine(7)).get(&mind), Some(&60));
    }

    #[test]
    fn terminal_progress_survives_two_requests_between_publisher_ticks() {
        let mind = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, mind, 60, engine(7)).lease_for(mind).expect("granted");
        let first = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        settle(first, Some(&TerminalReceipt { finish: "stop".into() }));
        let first_revision = owner.honoring_with_receipts(engine(7), |fixed, receipts| {
            assert!(fixed.is_empty());
            *receipts.get(&mind).expect("terminal progress")
        });
        let second = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved again");
        settle(second, Some(&TerminalReceipt { finish: "stop".into() }));
        let third = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("another active request");
        let revision = owner.honoring_with_receipts(engine(7), |fixed, receipts| {
            assert_eq!(fixed.get(&mind), Some(&60), "new request still owns capacity");
            let revision = *receipts.get(&mind).expect("completion was not lost to restart");
            assert!(revision > first_revision);
            revision
        });
        drop(third);
        owner.honoring_with_receipts(engine(7), |fixed, receipts| {
            assert_eq!(fixed.get(&mind), Some(&60), "unresolved custody remains fixed");
            assert_eq!(receipts.get(&mind), Some(&revision), "cancellation is not a completion");
        });
    }

    #[test]
    fn unsent_attempts_and_fenced_receipts_do_not_claim_completed_service() {
        let mind = uuid::Uuid::new_v4();
        let owner = owner();
        let lease = publish(None, mind, 60, engine(7)).lease_for(mind).expect("granted");
        let unsent = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        unsent.release_unsent();
        owner.honoring_with_receipts(engine(7), |fixed, receipts| {
            assert!(fixed.is_empty());
            assert!(receipts.is_empty(), "a refused attempt did not serve the mind");
        });
        let finished = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        settle(finished, Some(&TerminalReceipt { finish: "stop".into() }));
        let delayed = owner.reserve_against(mind, lease, 50, || Some(lease)).expect("reserved");
        owner.honoring_with_receipts(engine(8), |fixed, receipts| {
            assert!(fixed.is_empty());
            assert!(receipts.is_empty(), "old-engine progress cannot authorize new-engine handoff");
        });
        settle(delayed, Some(&TerminalReceipt { finish: "stop".into() }));
        owner.honoring_with_receipts(engine(8), |fixed, receipts| {
            assert!(fixed.is_empty());
            assert!(receipts.is_empty(), "a late receipt cannot resurrect the fenced entry");
        });
    }

    // The former terminal-only fairness gate is superseded by
    // fair::tests::http_reclamation_proof_publishes_the_waiter_without_freeing_early:
    // a terminal receipt alone cannot authorize reuse of resident KV.

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
