//! Capacity as a placement sees it: observed (a candidate) or committed (an accepted lease).
//!
//! Joel and Codex, 2026-11-25: a remote GPU that looks idle is a candidate, never a plan; an accepted
//! lease tells the scheduler what it can count on and for how long. The same model holds for this
//! machine, my own machines, and another person's grid over the P2P mesh.
//!
//! A lease is generic over what it promises (Cormac, Codex, 2026-11-26): work a second for the grid,
//! bytes on a tier, slots of something held. They differ in dimension and lifetime, so each is its own
//! type and one is never spent as another (UNIFIED-PAGING.md, slice 3a).

use std::time::{Duration, Instant};

use super::measured::Measured;

/// What a unit of work costs where it is offered: a stated price per unit (credits, or zero on a
/// machine I own), never assumed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub per_unit: f64,
}

/// Work a second a holder serves.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rate(pub f64);

/// An accepted lease: how much of `Q` was agreed, until when, at what price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lease<Q> {
    pub amount: Q,
    pub until: Instant,
    pub price: Price,
}

/// One holder's capacity to do work, as the ledger keeps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Capacity {
    /// Seen, not promised: the achieved rate as of when it was observed (it may already be gone).
    Observed { rate: Measured<f64>, as_of: Instant },
    /// Promised for a term: counted on until the lease ends.
    Committed(Lease<Rate>),
}

impl Capacity {
    /// What a scheduler may count on at `now`: a lease's rate until it ends, nothing otherwise. An
    /// observation is only a candidate to ask for a lease.
    pub fn committed_at(&self, now: Instant) -> f64 {
        match self {
            Self::Committed(lease) if now < lease.until => lease.amount.0,
            _ => 0.0,
        }
    }

    /// How old an observation is; a lease is not an observation.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        match self {
            Self::Observed { as_of, .. } => Some(now.saturating_duration_since(*as_of)),
            Self::Committed(_) => None,
        }
    }
}
