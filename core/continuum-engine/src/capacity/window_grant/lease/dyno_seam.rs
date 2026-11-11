//! The dyno's handle on ONE test-owned grant owner (`capacity::dyno::service`): the production
//! [`Reservations`] (compare-and-reserve, terminal settlement, fair publication, physical
//! receipts) behind a leaked instance, so a virtual-clock scenario drives exactly the transitions
//! the serving daemon and the dispatch path drive, without the process-global owner other tests
//! share. Forwarding and a read-only observation only: no decision is made here.
#![cfg(test)]

use super::*;
use crate::capacity::window_grant::PoolGrants;
use crate::inference::slots::grant_handoff::{ReclaimedGrant, RefusedGrant};
use super::fair::FairGrantState;

/// One grant owner of the dyno's own.
pub(crate) struct OwnerUnderTest {
    owner: &'static Reservations,
}

/// What the owner charges on one engine incarnation, read without fencing anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Observation {
    /// Per mind, the claim charged against the pool: her largest reserved lease (live or
    /// unresolved), or the publication she is retiring from.
    pub charged: BTreeMap<uuid::Uuid, u32>,
    /// Requests whose guard is held.
    pub live: BTreeMap<uuid::Uuid, usize>,
    /// Requests whose caller let go before a terminal receipt (held until a fence).
    pub unresolved: BTreeMap<uuid::Uuid, usize>,
    /// Minds whose publication is being handed off (charged until physical proof or fence).
    pub retiring: BTreeMap<uuid::Uuid, GrantLease>,
}

impl OwnerUnderTest {
    pub(crate) fn new() -> Self {
        Self { owner: Box::leak(Box::new(Reservations::new())) }
    }

    /// One daemon publication (`publish_fair_grants` on this owner): the grants it publishes and
    /// the handoff tickets it issues.
    pub(crate) fn publish(
        &self,
        fair: &mut FairGrantState,
        demands: &[Demand<uuid::Uuid>],
        pool: u32,
        ceiling: u32,
        engine: Option<EngineIncarnation>,
        current: Option<PoolGrants>,
    ) -> (PoolGrants, Vec<GrantHandoff>) {
        self.owner.fair_publish(fair, demands, pool, ceiling, engine, || current, |next| next)
    }

    /// One dispatch attempt's reservation (`reserve_attempt` on this owner) against `current`, her
    /// lease as published at the instant of the reserve.
    pub(crate) fn reserve(
        &self,
        slot_for: &mut Option<GrantReservation>,
        persona: uuid::Uuid,
        leased: GrantLease,
        needed: u32,
        current: Option<GrantLease>,
    ) -> Result<(), ReserveRefused> {
        reserve_attempt_with(self.owner, slot_for, persona, leased, needed, || current)
    }

    /// The engine's physical save-then-erase receipt for a handoff.
    pub(crate) fn reclaimed(&self, ticket: GrantHandoff) -> Result<(), String> {
        self.owner.record_reclaimed(ReclaimedGrant::scripted(ticket))
    }

    /// The engine positively refused the save or the erase.
    pub(crate) fn refused(&self, ticket: GrantHandoff) -> Result<(), String> {
        self.owner.record_refused(RefusedGrant::scripted(ticket))
    }

    /// What the owner holds on `engine` now.
    pub(crate) fn observe(&self, engine: Option<EngineIncarnation>) -> Observation {
        let state = self.owner.lock();
        let mut seen = Observation::default();
        for (mind, entries) in &state.entries {
            for entry in entries.iter().filter(|entry| entry.engine == engine) {
                let claim = seen.charged.entry(*mind).or_insert(0);
                *claim = (*claim).max(entry.tokens);
                let count = match entry.custody {
                    Custody::Live => seen.live.entry(*mind).or_insert(0),
                    Custody::Unresolved => seen.unresolved.entry(*mind).or_insert(0),
                };
                *count += 1;
            }
        }
        for (mind, lease) in state.retiring.iter().filter(|(_, lease)| lease.engine == engine) {
            let claim = seen.charged.entry(*mind).or_insert(0);
            *claim = (*claim).max(lease.tokens);
            seen.retiring.insert(*mind, *lease);
        }
        seen
    }
}
