//! An ask: a lease that waits instead of being refused (UNIFIED-PAGING.md slice 3a; the 5090 on
//! 2026-11-26, where a learning seat was refused 5.7 GB with 5 MB free and nothing ever woke it).
//!
//! The ask is a wait on the authority's one `place` ledger. It is granted, in order of asking per memory,
//! from what is free once a lease is released, shrunk, or a reading frees room, and becomes the lease it
//! asked for without being asked again. A foreground acquire is taken ahead of it and never queues behind
//! it; which waits deserve room made for them is the law's to decide, not a queue's.

use continuum_sage::place::{Answer, Bytes, ClaimId, Generation, Need};

use super::{told, LeaseError, LeaseRequest, ResourceLease, ResourceLeaseLedger};

/// What an ask became.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// Granted now, as this lease.
    Granted(ResourceLease),
    /// Waiting as `ask`: taken once granted ([`ResourceLeaseLedger::take_granted`]), or withdrawn.
    Waiting { ask: ClaimId, short: u64 },
}

/// A waiting ask: the lease it becomes.
#[derive(Debug, Clone)]
pub(super) struct Asking {
    req: LeaseRequest,
    lease_id: String,
    asked_at_ms: u64,
}

impl ResourceLeaseLedger {
    /// Ask for `req` as the lease `lease_id`: granted now if it fits (floors and earlier waits on its memory
    /// honored), otherwise queued until it does. Only a need past the memory's capacity is refused.
    pub fn ask(&mut self, req: &LeaseRequest, lease_id: &str, now_ms: u64) -> Result<Asked, LeaseError> {
        let lease_id = lease_id.to_owned();
        if self.leases.contains_key(&lease_id) || self.asks.values().any(|asking| asking.lease_id == lease_id) {
            return Err(LeaseError::DuplicateLease { lease_id });
        }
        let need = [Need { place: req.kind, amount: Bytes(req.bytes) }];
        let answer = self.place.prepare(told::owner_of(&req.consumer_id), Generation(1), &need);
        let asked = match answer {
            Answer::Granted(claim) => Asked::Granted(self.hold(claim, req, lease_id, now_ms)?),
            Answer::Waiting { id, short } => {
                self.asks.insert(id, Asking { req: req.clone(), lease_id, asked_at_ms: now_ms });
                let short = short.iter().map(|need| need.amount.0).fold(0, u64::max); // a wait behind an earlier one on its memory may lack nothing itself
                Asked::Waiting { ask: id, short }
            }
            Answer::Never { capacity, .. } => return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: capacity.0 }),
            Answer::Unknown { .. } | Answer::Stale { .. } => return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: 0 }),
        };
        self.tell();
        Ok(asked)
    }

    /// Withdraw `ask`: its owner no longer wants it. A waiting ask leaves the queue; one granted and not yet
    /// taken is released, so memory nobody will use is never held. Answers whether there was anything to
    /// withdraw.
    pub fn withdraw(&mut self, ask: ClaimId) -> bool {
        if self.asks.remove(&ask).is_some() {
            return self.place.unwind(ask).is_ok();
        }
        match self.granted.remove(&ask) {
            Some(lease) => self.release(&lease.lease_id).is_ok(),
            None => false,
        }
    }

    /// The lease `ask` became, once granted; taken once (its owner then holds and releases it).
    pub fn take_granted(&mut self, ask: ClaimId) -> Option<ResourceLease> {
        self.granted.remove(&ask)
    }

    /// Spend `req.bytes` of the lease `reserve` (a granted ask its owner keeps as a reserve) as a lease of
    /// their own, `lease_id`: the reserve shrinks and the new lease takes exactly those bytes, with nothing
    /// told in between, so no wait and no other taker can have them first. Refused, changing nothing, when
    /// the reserve is of another kind or holds less.
    pub fn carve(&mut self, reserve: &str, req: &LeaseRequest, lease_id: &str, now_ms: u64) -> Result<ResourceLease, LeaseError> {
        let (kind, bytes, claim) = match self.leases.get(reserve) {
            Some(held) => (held.lease.kind, held.lease.bytes, held.claim),
            None => return Err(LeaseError::MissingLease { lease_id: reserve.to_owned() }),
        };
        if kind != req.kind || bytes < req.bytes {
            return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: if kind == req.kind { bytes } else { 0 } });
        }
        if self.leases.contains_key(lease_id) {
            return Err(LeaseError::DuplicateLease { lease_id: lease_id.to_owned() });
        }
        let left = bytes - req.bytes;
        if left == 0 {
            if let Some(held) = self.leases.remove(reserve) {
                if let Some(view) = held.view { view.bytes.store(0, std::sync::atomic::Ordering::Release); }
            }
        } else if let Some(held) = self.leases.get_mut(reserve) {
            held.lease.bytes = left;
            if let Some(view) = &held.view { view.bytes.store(left, std::sync::atomic::Ordering::Release); }
        }
        if self.place.shrink(claim, kind, Bytes(req.bytes)).is_err() {
            continuum_base::probe!(class = "resources.ledger.carve_unknown", reserve = %reserve, "a reserve had no claim left on place");
        }
        let taken = self.place.take(told::owner_of(&req.consumer_id), Generation(1), &[Need { place: req.kind, amount: Bytes(req.bytes) }]);
        let lease = match taken {
            Ok(claim) => self.hold(claim, req, lease_id.to_owned(), now_ms),
            // the bytes just freed fit by construction; only a floor told since could refuse them
            Err(_untaken) => Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: 0 }),
        };
        self.tell();
        lease
    }

    /// Grant every waiting ask that now fits and turn each into its lease. Run after every change that can
    /// free room; returns whether any was granted (a grant can change floors, so the caller tells again).
    pub(super) fn grant_asks(&mut self) -> bool {
        let mut claims = Vec::new();
        self.place.grant_waiting(&mut claims);
        let any = !claims.is_empty();
        for claim in claims {
            let id = claim.id();
            let Some(asking) = self.asks.remove(&id) else {
                continuum_base::probe!(class = "resources.ledger.grant_unasked", claim = ?id, "place granted a wait the authority holds no ask for");
                continue;
            };
            match self.hold(claim, &asking.req, asking.lease_id, asking.asked_at_ms) {
                Ok(lease) => {
                    self.granted.insert(id, lease);
                }
                Err(defect) => continuum_base::probe!(class = "resources.ledger.grant_unheld", claim = ?id, error = ?defect, "a granted ask could not be held"),
            }
        }
        any
    }
}
