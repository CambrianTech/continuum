//! An ask: a lease that waits instead of being refused (UNIFIED-PAGING.md slice 3a; the 5090 on
//! 2026-11-26, where a learning seat was refused 5.7 GB with 5 MB free and nothing ever woke it).
//!
//! The ask is a wait on the authority's one `place` ledger. It is granted, in order of asking per memory,
//! from what is free once a lease is released, shrunk, or a reading frees room, and becomes the lease it
//! asked for without being asked again. A foreground acquire is taken ahead of it and never queues behind
//! it; which waits deserve room made for them is the law's to decide, not a queue's.

use continuum_sage::place::{Act, AskId, Bytes, Need, SplitAsk, Status};

use super::{LeaseError, LeaseRequest, ResourceLease, ResourceLeaseLedger};

/// What an ask became.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// Granted now, as this lease.
    Granted(ResourceLease),
    /// Waiting as `ask`: taken once granted ([`ResourceLeaseLedger::take_granted`]), or withdrawn.
    Waiting { ask: AskId, short: u64 },
}

/// Why a stage was not carved from its reserve: the lease side (released, lapsed, of another kind, holding
/// less, a duplicate id), or the refusal `place` gave the split, kept as its own type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CarveError {
    #[error("{0:?}")]
    Lease(LeaseError),
    #[error("place refused the split: {0:?}")]
    Place(continuum_sage::place::Refusal),
}

impl From<LeaseError> for CarveError {
    fn from(error: LeaseError) -> Self {
        Self::Lease(error)
    }
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
    /// honored), otherwise a wait in the core until it does. Only a need past the memory's capacity is refused.
    pub fn ask(&mut self, req: &LeaseRequest, lease_id: &str, now_ms: u64) -> Result<Asked, LeaseError> {
        let lease_id = lease_id.to_owned();
        if self.leases.contains_key(&lease_id) || self.asking.values().any(|asking| asking.lease_id == lease_id) {
            return Err(LeaseError::DuplicateLease { lease_id });
        }
        let ask = self.ask_id(&req.consumer_id);
        let needs = vec![Need { place: req.kind, amount: Bytes(req.bytes) }];
        self.core.apply(Act::Prepare { ask, generation: self.generation(), needs });
        self.core.settle();
        let asked = match self.core.status(ask).cloned() {
            Some(Status::Granted(claim)) => Asked::Granted(self.hold((ask, claim), req, lease_id, now_ms)?),
            Some(Status::Waiting { short }) => {
                self.asking.insert(ask, Asking { req: req.clone(), lease_id, asked_at_ms: now_ms });
                let short = short.iter().map(|need| need.amount.0).fold(0, u64::max); // a wait behind an earlier one on its memory may lack nothing itself
                Asked::Waiting { ask, short }
            }
            Some(Status::Never { capacity, .. }) => {
                self.core.apply(Act::Forget(ask));
                return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: capacity.0 });
            }
            _ => {
                self.core.drop_ask(ask);
                return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: 0 });
            }
        };
        self.tell();
        Ok(asked)
    }

    /// A foreground acquire of `req` was taken while asks wait on its kind: probed with how many wait and
    /// how long the oldest has. A foreground take passes waits by design (serving never queues behind a
    /// seat), so a steady stream of them can starve an ask; this is the evidence its aging bound, the
    /// decide::Policy, is fitted to (Fable on 7d59f5639).
    pub(super) fn note_passed(&self, req: &LeaseRequest, now_ms: u64) {
        let waiting = self
            .asking
            .iter()
            .filter(|(ask, asking)| asking.req.kind == req.kind && matches!(self.core.status(**ask), Some(Status::Waiting { .. })));
        let (count, oldest) = waiting.fold((0u32, now_ms), |(count, oldest), (_, asking)| (count + 1, oldest.min(asking.asked_at_ms)));
        if count > 0 {
            continuum_base::probe!(class = "resources.ask.passed", consumer = %req.consumer_id, kind = ?req.kind, bytes = req.bytes, waiting = count, oldest_wait_ms = now_ms.saturating_sub(oldest),
                "a foreground take passed asks waiting on its kind");
        }
    }

    /// Withdraw `ask`: its owner no longer wants it. A waiting ask leaves the core's queue; one granted and
    /// not yet taken is unwound, so memory nobody will use is never held. Answers whether there was anything
    /// to withdraw.
    pub fn withdraw(&mut self, ask: AskId) -> bool {
        if self.asking.remove(&ask).is_none() {
            return false;
        }
        let present = self.core.status(ask).is_some();
        self.core.apply(Act::Unwind(ask));
        self.tell();
        present
    }

    /// The lease `ask` became, once the core granted it: spent and committed now, taken once (its owner then
    /// holds and releases it). `None` while it waits.
    pub fn take_granted(&mut self, ask: AskId) -> Option<ResourceLease> {
        let Some(Status::Granted(claim)) = self.core.status(ask).cloned() else { return None };
        let asking = self.asking.remove(&ask)?;
        match self.hold((ask, claim), &asking.req, asking.lease_id, asking.asked_at_ms) {
            Ok(lease) => Some(lease),
            Err(defect) => {
                continuum_base::probe!(class = "resources.ledger.grant_unheld", ask = ?ask, error = ?defect, "a granted ask could not be held");
                None
            }
        }
    }

    /// Spend `req.bytes` of the lease `reserve` (a granted ask its owner keeps as a reserve) as a lease of
    /// their own, `lease_id`: one transition in the core (a split), every charge unchanged, so no wait and
    /// no other taker can have the bytes first. Refused, changing nothing, when the reserve has lapsed, is of
    /// another kind or holds less.
    pub fn carve(&mut self, reserve: &str, req: &LeaseRequest, lease_id: &str, now_ms: u64) -> Result<ResourceLease, CarveError> {
        let (kind, bytes, from) = match self.leases.get(reserve) {
            // a lapsed reserve stays charged for its fenced release; nothing is carved from it into a fresh term
            Some(held) if held.lease.is_expired(now_ms) => return Err(LeaseError::ExpiredLease { lease_id: reserve.to_owned() }.into()),
            Some(held) => (held.lease.kind, held.lease.bytes, held.ask),
            None => return Err(LeaseError::MissingLease { lease_id: reserve.to_owned() }.into()),
        };
        if kind != req.kind || bytes < req.bytes {
            return Err(LeaseError::InsufficientCapacity { kind: req.kind, requested: req.bytes, available: if kind == req.kind { bytes } else { 0 } }.into());
        }
        if self.leases.contains_key(lease_id) {
            return Err(LeaseError::DuplicateLease { lease_id: lease_id.to_owned() }.into());
        }
        let into = self.ask_id(&req.consumer_id);
        let split = SplitAsk { from, moved: Need { place: kind, amount: Bytes(req.bytes) }, into, generation: self.generation() };
        self.core.split(split).map_err(CarveError::Place)?;
        let left = bytes - req.bytes;
        if left == 0 {
            if let Some(held) = self.leases.remove(reserve) {
                if let Some(view) = held.view { view.bytes.store(0, std::sync::atomic::Ordering::Release); }
            }
        } else if let Some(held) = self.leases.get_mut(reserve) {
            held.lease.bytes = left;
            if let Some(view) = &held.view { view.bytes.store(left, std::sync::atomic::Ordering::Release); }
        }
        let lease = ResourceLease {
            lease_id: lease_id.to_owned(),
            consumer_id: req.consumer_id.clone(),
            kind: req.kind,
            bytes: req.bytes,
            acquired_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(req.ttl_ms),
            reclaim_policy: req.reclaim_policy,
        };
        self.leases.insert(lease_id.to_owned(), super::Held { lease: lease.clone(), landed_from: None, view: None, ask: into });
        self.tell(); // after the transition: what the floors and waits see is its result
        Ok(lease)
    }
}
