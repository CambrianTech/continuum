//! An ask on the authority: memory a consumer waits for instead of being refused (the ledger's `ask`).
//! Granted now as a [`LeaseGuard`], or waiting as an [`AskTicket`] its owner keeps: once room is released
//! the authority grants it as a lease without being asked again, and the owner takes the guard with
//! [`AskTicket::granted`] on its next wake (the board it hears says something changed). Dropping the ticket
//! withdraws the ask, and releases a grant its owner never took, so no wait and no lease outlives the
//! one who wanted it (custody: Codex, 2026-11-09).

use std::sync::Arc;

use continuum_base::time::EpochMs;
use continuum_sage::place::ClaimId;

use super::super::ledger::Asked;
use super::super::governor::ResourceGovernor;
use super::super::lease::{LeaseError, LeaseRequest};
use super::{LeaseGuard, ResourceDaemon};

/// What an ask became.
pub enum Ask {
    Granted(LeaseGuard),
    /// Waiting: short `short` bytes of its kind when it asked.
    Waiting { ticket: AskTicket, short: u64 },
}

/// A waiting ask, owned by the one who asked.
pub struct AskTicket {
    daemon: Arc<ResourceDaemon>,
    ask: ClaimId,
    taken: bool,
}

impl AskTicket {
    /// The lease the ask became, once the authority granted it; taken once.
    pub fn granted(&mut self) -> Option<LeaseGuard> {
        let lease = self.daemon.governor.lock().take_granted(self.ask)?;
        self.taken = true;
        Some(LeaseGuard { daemon: Arc::clone(&self.daemon), lease_id: lease.lease_id, released: false })
    }
}

impl Drop for AskTicket {
    fn drop(&mut self) {
        if self.taken {
            return;
        }
        let mut governor = self.daemon.governor.lock();
        if governor.withdraw(self.ask) {
            self.daemon.publish_locked(&governor);
        }
    }
}

impl ResourceDaemon {
    /// Ask for `req`, decided on what is physically resident now ([`Self::measured_now`]): granted now, or
    /// waiting until room is released. Refused only past what the memory could ever hold.
    pub fn ask(self: &Arc<Self>, req: &LeaseRequest) -> Result<Ask, LeaseError> {
        let mut governor = self.measured_now();
        let asked = governor.ask(req, EpochMs::now().millis())?;
        self.publish_locked(&governor);
        drop(governor);
        Ok(match asked {
            Asked::Granted(lease) => Ask::Granted(LeaseGuard { daemon: Arc::clone(self), lease_id: lease.lease_id, released: false }),
            Asked::Waiting { ask, short } => Ask::Waiting { ticket: AskTicket { daemon: Arc::clone(self), ask, taken: false }, short },
        })
    }

    /// Spend `req.bytes` of `reserve` (a granted ask held as a seat's reserve) as a lease of their own, under
    /// one lock: nobody can take those bytes between the reserve shrinking and the lease taking them. The
    /// reserve keeps what is left, and is released when its guard drops.
    pub fn carve(self: &Arc<Self>, reserve: &LeaseGuard, req: &LeaseRequest) -> Result<LeaseGuard, LeaseError> {
        let mut governor = self.governor.lock();
        let lease = governor.carve(reserve.lease_id(), req, EpochMs::now().millis())?;
        self.publish_locked(&governor);
        Ok(LeaseGuard { daemon: Arc::clone(self), lease_id: lease.lease_id, released: false })
    }

    /// The governor, locked, after every source was measured and ingested now: an admission between ticks
    /// never reads a sample up to a tick old. The sources are read outside the lock (a slow reader never
    /// stalls other allocations), then ingested in order under it, so the grant is decided against the
    /// newest accepted sample (Codex on #6124).
    pub(super) fn measured_now(&self) -> parking_lot::MutexGuard<'_, ResourceGovernor> {
        let scans: Vec<_> = self.sources.iter().map(|s| (s.kind(), s.ceiling_bytes(), s.sample(), s.backed_by())).collect();
        let mut governor = self.governor.lock();
        for (kind, ceiling, sample, backing) in scans {
            governor.set_capacity(kind, ceiling);
            governor.set_physical_sample(kind, sample);
            governor.set_backing(kind, backing);
        }
        governor
    }
}
