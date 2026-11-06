//! ONE-ACT section 4's feasible service opportunity, as a typed predicate over the owners' state
//! at one publication boundary: "an eligible waiter owed service gets the next *feasible*
//! opportunity; release of one turn does not imply that sufficient compatible residency was
//! released."
//!
//! This is the SPEC the dyno checks the production grant owner against, read from facts the
//! scenario itself established (which receipts it delivered, which requests it holds) and from
//! what the owner charges, never from the owner's private bookkeeping (its waiter queue, its
//! service baselines). The owner decides; this only says what a decision must have done.
//!
//! At a publication, for the OLDEST eligible waiter W (eligible: her need fits one sequence and
//! the pool; owed: her published grant is below her need):
//! - [`Opportunity::Grant`]: the cells not claimed by anyone (published, charged or physically
//!   resident) plus what W herself holds cover her need. The publication must grant her.
//! - [`Opportunity::Reclaim`]: they do not, no handoff is already in flight, and some other holder
//!   H has completed a turn since her grant (a terminal receipt the scenario delivered on this
//!   engine) and owns no request (no live, unsent or unresolved custody). The publication must
//!   begin a handoff (issue a ticket). Reclaiming H may not suffice alone; the owner accumulates
//!   (`fair::tests::a_large_waiter_accumulates_reclaimed_capacity_across_two_holders`), and each
//!   further completed holder is a further opportunity.
//!
//! Anything else is not an opportunity, and says why ([`NoOpportunity`]). Progress bounds are
//! stated in these opportunities under named assumptions (finite preceding work, feasible demand,
//! a backend that delivers receipts), never as a wall-clock deadline.
#![cfg(test)]

use std::collections::BTreeMap;

pub(crate) type Mind = uuid::Uuid;

/// One mind as the predicate reads her at a publication boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MindAt {
    pub need: u32,
    /// Her published grant on this engine (0 when none).
    pub published: u32,
    /// What the grant owner charges for her (reserved leases, a retiring publication).
    pub charged: u32,
    /// KV cells the engine still holds for her (the scenario's actuator).
    pub resident: u32,
    /// She owns a request: reserved, sent, or unresolved after her caller let go.
    pub custody: bool,
    /// A terminal receipt for her was delivered on this engine since her current grant.
    pub completed_since_grant: bool,
    /// The publication at which she became owed service (`None` while not owed).
    pub owed_since: Option<u64>,
    /// The owner is handing her publication off (charged until physical proof or fence).
    pub retiring: bool,
}

impl MindAt {
    /// The cells she occupies in any form: none of them can be given to someone else.
    pub fn claim(&self) -> u32 {
        self.published.max(self.charged).max(self.resident)
    }
}

/// The boundary the predicate reads.
#[derive(Debug, Clone)]
pub(crate) struct Boundary {
    pub pool: u32,
    pub ceiling: u32,
    /// In demand order (the tie-break between waiters owed since the same publication).
    pub minds: Vec<(Mind, MindAt)>,
}

/// What the publication at this boundary must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opportunity {
    Grant { waiter: Mind, need: u32, available: u32 },
    Reclaim { waiter: Mind, holder: Mind },
}

/// Why this boundary is not an opportunity for the oldest waiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoOpportunity {
    /// Nobody eligible is owed service.
    NoWaiter,
    /// A handoff is in flight: its physical receipt (or a fence) comes first.
    AwaitingPhysicalReceipt { holders: Vec<Mind> },
    /// Every holder that could be reclaimed owns a request (all pinned).
    CustodyOutstanding { waiter: Mind, holders: Vec<Mind> },
    /// No holder has completed a turn since her grant.
    NoCompletedHolder { waiter: Mind },
    /// A demand no sequence or pool can hold: an explicit placement shortfall, never a promise.
    Infeasible { mind: Mind, need: u32, limit: u32 },
}

impl Boundary {
    /// The oldest eligible waiter, and every infeasible demand seen on the way.
    fn oldest_waiter(&self) -> Result<(Mind, MindAt), NoOpportunity> {
        let limit = self.ceiling.min(self.pool);
        let mut infeasible = None;
        let mut oldest: Option<(u64, Mind, MindAt)> = None;
        for (mind, at) in &self.minds {
            if at.published >= at.need {
                continue;
            }
            if at.need > limit {
                infeasible.get_or_insert(NoOpportunity::Infeasible { mind: *mind, need: at.need, limit });
                continue;
            }
            if let Some(since) = at.owed_since {
                if oldest.is_none_or(|(o, _, _)| since < o) {
                    oldest = Some((since, *mind, *at));
                }
            }
        }
        match (oldest, infeasible) {
            (Some((_, mind, at)), _) => Ok((mind, at)),
            (None, Some(why)) => Err(why),
            (None, None) => Err(NoOpportunity::NoWaiter),
        }
    }

    /// The predicate.
    pub fn opportunity(&self) -> Result<Opportunity, NoOpportunity> {
        let (waiter, at) = self.oldest_waiter()?;
        let claimed: u64 = self.minds.iter().map(|(_, m)| u64::from(m.claim())).sum();
        let free = u64::from(self.pool).saturating_sub(claimed);
        let available = free.saturating_add(u64::from(at.claim()));
        if available >= u64::from(at.need) {
            return Ok(Opportunity::Grant { waiter, need: at.need, available: available.min(u64::from(u32::MAX)) as u32 });
        }
        let retiring: Vec<Mind> = self.minds.iter().filter(|(_, m)| m.retiring).map(|(mind, _)| *mind).collect();
        if !retiring.is_empty() {
            return Err(NoOpportunity::AwaitingPhysicalReceipt { holders: retiring });
        }
        let holders: BTreeMap<Mind, MindAt> =
            self.minds.iter().filter(|(mind, m)| *mind != waiter && m.published > 0).map(|(mind, m)| (*mind, *m)).collect();
        if let Some((holder, _)) = holders.iter().find(|(_, m)| m.completed_since_grant && !m.custody) {
            return Ok(Opportunity::Reclaim { waiter, holder: *holder });
        }
        let pinned: Vec<Mind> = holders.iter().filter(|(_, m)| m.custody).map(|(mind, _)| *mind).collect();
        match pinned.len() == holders.len() && !pinned.is_empty() {
            true => Err(NoOpportunity::CustodyOutstanding { waiter, holders: pinned }),
            false => Err(NoOpportunity::NoCompletedHolder { waiter }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(need: u32, published: u32) -> MindAt {
        MindAt { need, published, charged: published, resident: published, custody: false, completed_since_grant: false, owed_since: None, retiring: false }
    }

    // what this catches: reading a released turn as released residency (section 4). A holder's
    // completion is an opportunity only to reclaim; free cells are what can be granted; a pinned
    // holder, a handoff in flight and an infeasible need are named, not counted.
    #[test]
    fn a_completion_is_a_reclaim_opportunity_and_only_free_cells_are_a_grant() {
        let (a, b, c) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2), uuid::Uuid::from_u128(3));
        let waiter = MindAt { owed_since: Some(1), ..at(40, 0) };
        let mut boundary = Boundary { pool: 100, ceiling: 100, minds: vec![(a, at(90, 90)), (b, waiter)] };
        assert_eq!(boundary.opportunity(), Err(NoOpportunity::NoCompletedHolder { waiter: b }));
        boundary.minds[0].1.completed_since_grant = true;
        assert_eq!(boundary.opportunity(), Ok(Opportunity::Reclaim { waiter: b, holder: a }));
        boundary.minds[0].1.custody = true;
        assert_eq!(boundary.opportunity(), Err(NoOpportunity::CustodyOutstanding { waiter: b, holders: vec![a] }));
        boundary.minds[0].1.retiring = true;
        assert_eq!(boundary.opportunity(), Err(NoOpportunity::AwaitingPhysicalReceipt { holders: vec![a] }));
        // The physical receipt landed: published and resident are gone, the cells are free.
        boundary.minds[0].1 = MindAt { owed_since: Some(2), ..at(90, 0) };
        assert_eq!(boundary.opportunity(), Ok(Opportunity::Grant { waiter: b, need: 40, available: 100 }), "the older waiter first");
        boundary.minds.push((c, MindAt { owed_since: Some(0), ..at(120, 0) }));
        assert_eq!(boundary.opportunity(), Ok(Opportunity::Grant { waiter: b, need: 40, available: 100 }), "an infeasible need never blocks a feasible one");
        boundary.minds = vec![(c, MindAt { owed_since: Some(0), ..at(120, 0) })];
        assert_eq!(boundary.opportunity(), Err(NoOpportunity::Infeasible { mind: c, need: 120, limit: 100 }));
    }
}
