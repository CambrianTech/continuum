//! The composition the dyno drives: the PRODUCTION grant owner (`window_grant` fair publication,
//! compare-and-reserve, terminal settlement, handoff receipts, engine fencing) behind
//! [`OwnerUnderTest`], with a scripted actuator standing in for the engine (what it accepted, what
//! KV it still holds, which receipts it returns). Every step is applied once; after EVERY step
//! the safety invariants are checked, and at every publication the feasible-opportunity
//! obligation of [`super::opportunity`] is checked against what the owner published.
//!
//! What is production here: every grant, reservation, refusal, ticket, fence and settlement. What
//! is scripted: the engine's receipts (when, and whether, a terminal or physical receipt arrives)
//! and the mind's turn intent. The HTTP save/erase ordering behind a physical receipt is the
//! paging custodian's own test (`slots::grant_handoff::tests`).
#![cfg(test)]

use std::collections::BTreeMap;

use super::opportunity::{Boundary, MindAt, Mind, NoOpportunity, Opportunity};
use crate::capacity::window_grant::dyno_seam::{Observation, OwnerUnderTest};
use crate::capacity::window_grant::{settle, Demand, FairGrantState, GrantHandoff, GrantLease, GrantReservation, PoolGrants, ReserveRefused, TerminalReceipt};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::planning::window_allocator::test_tokens;

/// One scripted event. A step whose precondition does not hold is a no-op, so a schedule stays
/// valid under deletion (the shrinker relies on it). Minds are named by demand-order index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The serving daemon's planning tick: one fair publication.
    Publish,
    /// Her act is rendered and quoted against her published lease.
    Quote(usize),
    /// Dispatch compares-and-reserves the quoted lease.
    Reserve(usize),
    /// The POST is accepted; the engine allocates her KV.
    Send(usize),
    /// The engine's terminal receipt (a final frame with a finish reason).
    Terminal(usize),
    /// The stream ends without a terminal receipt (a cut stream, a caller that let go).
    Disconnect(usize),
    /// Cancelled after the quote, before the reserve.
    CancelQuoted(usize),
    /// Cancelled after the reserve, before the POST: the reservation is dropped.
    CancelReserved(usize),
    /// The engine's save-then-erase receipt for her handoff.
    Reclaimed(usize),
    /// The engine positively refused her handoff's save or erase.
    RefusedHandoff(usize),
    /// The engine at the same URL is replaced (a crash and relaunch): a new incarnation.
    ReplaceEngine,
}

/// Where her current request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Idle,
    Quoted,
    Reserved,
    Sent,
}

/// A broken invariant, at the step that broke it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Violation {
    /// Claims (published, charged, or physically resident) past the pool.
    OverPool { claimed: u64, pool: u32 },
    /// KV the engine still holds for her that no grant or charge covers.
    UncoveredResident { mind: Mind, resident: u32, published: u32, charged: u32 },
    /// The owner dropped a claim whose request it never saw end (no terminal receipt, no fence).
    UnresolvedReleased { mind: Mind },
    /// A ticket for a mind that still owns a request.
    TicketWithCustody { mind: Mind, phase: Phase, unresolved: u32 },
    /// A ticket for a holder with no completed turn since her grant on this engine.
    TicketWithoutCompletion { mind: Mind },
    /// A publication grants a mind the owner is retiring.
    GrantedWhileRetiring { mind: Mind },
    /// A reserve accepted a lease that is not her published one.
    ReservedStaleLease { mind: Mind },
    /// The publication did not do what its feasible opportunity required.
    MissedOpportunity { publication: u64, opportunity: Opportunity },
}

/// One mind's scripted facts.
struct MindState {
    name: String,
    need: u32,
    phase: Phase,
    quoted: Option<GrantLease>,
    reservation: Option<GrantReservation>,
    /// The engine her reservation was made on (a receipt after a replacement is late).
    reserved_on: Option<EngineIncarnation>,
    /// Requests dropped without a terminal receipt on the current engine.
    unresolved: u32,
    resident: u32,
    published: u32,
    completed_since_grant: bool,
    owed_since: Option<u64>,
    /// She held a seat across the last publication with nothing in custody: an unused seat, which
    /// yields to a waiter whether or not she completed a turn since her grant (`fair_publish`).
    idle_at_last_publish: bool,
}

impl MindState {
    /// A reservation is held (reserved or sent) or was let go unresolved. A quote holds nothing.
    fn owns_request(&self) -> bool {
        matches!(self.phase, Phase::Reserved | Phase::Sent) || self.unresolved > 0
    }
}

/// What one step did, for a scenario driver to react to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    pub tickets: Vec<usize>,
    pub newly_served: Vec<usize>,
    pub refused: Option<ReserveRefused>,
    pub opportunity: Option<Result<Opportunity, NoOpportunity>>,
}

pub(crate) struct World {
    owner: OwnerUnderTest,
    fair: FairGrantState,
    current: Option<PoolGrants>,
    pool: u32,
    ceiling: u32,
    engine: EngineIncarnation,
    order: Vec<Mind>,
    minds: BTreeMap<Mind, MindState>,
    tickets: BTreeMap<Mind, GrantHandoff>,
    publications: u64,
    steps: u64,
    /// Each mind's terminal receipts on any engine, and the publication she was first granted.
    pub completions: BTreeMap<Mind, u64>,
    pub first_served: BTreeMap<Mind, u64>,
    pub violations: Vec<(u64, Step, Violation)>,
    pub log: Vec<String>,
}

impl World {
    /// Minds in demand order, by name and need, on a pool whose one sequence holds `ceiling`.
    pub(crate) fn new(pool: u32, ceiling: u32, minds: &[(&str, u32)]) -> Self {
        let order: Vec<Mind> = (1..=minds.len() as u128).map(uuid::Uuid::from_u128).collect();
        let states = order.iter().zip(minds).map(|(id, (name, need))| {
            (*id, MindState {
                name: name.to_string(), need: *need, phase: Phase::Idle, quoted: None, reservation: None, reserved_on: None,
                unresolved: 0, resident: 0, published: 0, completed_since_grant: false, owed_since: None, idle_at_last_publish: false,
            })
        }).collect();
        Self {
            owner: OwnerUnderTest::new(), fair: FairGrantState::default(), current: None, pool, ceiling,
            engine: EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 },
            order, minds: states, tickets: BTreeMap::new(), publications: 0, steps: 0,
            completions: BTreeMap::new(), first_served: BTreeMap::new(), violations: Vec::new(), log: Vec::new(),
        }
    }

    pub(crate) fn mind(&self, index: usize) -> Mind {
        self.order[index % self.order.len()]
    }

    pub(crate) fn name(&self, index: usize) -> &str {
        &self.minds[&self.mind(index)].name
    }

    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }

    pub(crate) fn phase(&self, index: usize) -> Phase {
        self.minds[&self.mind(index)].phase
    }

    pub(crate) fn published(&self, index: usize) -> u32 {
        self.minds[&self.mind(index)].published
    }

    pub(crate) fn observe(&self) -> Observation {
        self.owner.observe(Some(self.engine))
    }

    /// Start from a publication the recorded day already held (its grants stamped as the daemon
    /// stamps them), with the holders' KV resident.
    pub(crate) fn start_published(&mut self, grants: &[(usize, u32)]) {
        let mut next = PoolGrants {
            pool_tokens: self.pool, per_sequence_cap: self.ceiling, engine: Some(self.engine),
            by_mind: grants.iter().map(|(i, tokens)| (self.mind(*i), *tokens)).collect(), revisions: BTreeMap::new(),
        };
        crate::capacity::window_grant::stamp_revisions(None, &mut next);
        for (i, tokens) in grants {
            let id = self.mind(*i);
            let state = self.minds.get_mut(&id).expect("test: a seeded holder is a mind"); // expect: the dyno's own state invariant, a seeded holder is a mind
            state.published = *tokens;
            state.resident = *tokens;
            self.first_served.insert(id, 0);
        }
        self.current = Some(next);
    }

    fn lease_now(&self, mind: Mind) -> Option<GrantLease> {
        self.current.as_ref().filter(|c| c.engine == Some(self.engine)).and_then(|c| c.lease_for(mind))
    }

    /// The boundary the opportunity predicate reads, before a publication.
    pub(crate) fn boundary(&self) -> Boundary {
        let seen = self.observe();
        let minds = self.order.iter().map(|mind| {
            let s = &self.minds[mind];
            (*mind, MindAt {
                need: s.need, published: s.published, charged: seen.charged.get(mind).copied().unwrap_or(0), // unwrap_or: nothing charged
                resident: s.resident, custody: s.owns_request(),
                completed_since_grant: s.completed_since_grant, owed_since: s.owed_since, retiring: seen.retiring.contains_key(mind),
            })
        }).collect();
        Boundary { pool: self.pool, ceiling: self.ceiling, minds }
    }

    /// Apply one step, then check every safety invariant.
    pub(crate) fn apply(&mut self, step: Step) -> Applied {
        self.steps += 1;
        let applied = match step {
            Step::Publish => self.publish(),
            Step::ReplaceEngine => {
                self.engine = EngineIncarnation { pid: self.engine.pid + 1, started_s: self.engine.started_s + 1, port: self.engine.port };
                // The old engine's KV is gone and its unresolved requests are fenced. A quote,
                // a reservation and a handoff ticket made against it stay with their holders: a
                // late reserve or receipt for them is exactly what must be refused or ignored.
                for s in self.minds.values_mut() {
                    (s.resident, s.unresolved, s.published, s.completed_since_grant, s.idle_at_last_publish) = (0, 0, 0, false, false);
                }
                Applied::default()
            }
            Step::Quote(i) => {
                let id = self.mind(i);
                let lease = self.lease_now(id);
                let s = self.minds.get_mut(&id).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                if let (Phase::Idle, Some(lease)) = (s.phase, lease) {
                    (s.phase, s.quoted) = (Phase::Quoted, Some(lease));
                }
                Applied::default()
            }
            Step::Reserve(i) => self.reserve(i),
            Step::Send(i) => {
                let s = self.minds.get_mut(&self.order[i % self.order.len()]).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                if s.phase == Phase::Reserved && s.reserved_on == Some(self.engine) {
                    s.phase = Phase::Sent;
                    s.resident = s.resident.max(s.quoted.map_or(0, |l| l.tokens));
                }
                Applied::default()
            }
            Step::Terminal(i) | Step::Disconnect(i) => {
                let engine = self.engine;
                let id = self.mind(i);
                let s = self.minds.get_mut(&id).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                if s.phase == Phase::Sent {
                    let reservation = s.reservation.take().expect("test: a sent request holds its reservation"); // expect: the dyno's own state invariant, a sent request holds its reservation
                    let current = s.reserved_on == Some(engine);
                    s.phase = Phase::Idle;
                    if matches!(step, Step::Terminal(_)) {
                        settle(reservation, Some(&TerminalReceipt { finish: "stop".into() }));
                        *self.completions.entry(id).or_insert(0) += 1;
                        s.completed_since_grant |= current;
                    } else {
                        settle(reservation, None);
                        s.unresolved += u32::from(current);
                    }
                }
                Applied::default()
            }
            Step::CancelQuoted(i) => {
                let s = self.minds.get_mut(&self.order[i % self.order.len()]).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                if s.phase == Phase::Quoted {
                    (s.phase, s.quoted) = (Phase::Idle, None);
                }
                Applied::default()
            }
            Step::CancelReserved(i) => {
                let engine = self.engine;
                let s = self.minds.get_mut(&self.order[i % self.order.len()]).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                if s.phase == Phase::Reserved {
                    drop(s.reservation.take());
                    s.unresolved += u32::from(s.reserved_on == Some(engine));
                    s.phase = Phase::Idle;
                }
                Applied::default()
            }
            Step::Reclaimed(i) | Step::RefusedHandoff(i) => {
                let id = self.mind(i);
                if let Some(ticket) = self.tickets.remove(&id) {
                    let outcome = match step {
                        Step::Reclaimed(_) => self.owner.reclaimed(ticket),
                        _ => self.owner.refused(ticket),
                    };
                    let s = self.minds.get_mut(&id).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
                    if let (Step::Reclaimed(_), Ok(())) = (step, &outcome) {
                        s.resident = 0;
                    }
                    self.log.push(format!("{:>4} {} physical receipt {step:?}: {outcome:?}", self.steps, s.name));
                }
                Applied::default()
            }
        };
        self.check_safety(step);
        applied
    }

    fn reserve(&mut self, i: usize) -> Applied {
        let id = self.mind(i);
        let now = self.lease_now(id);
        let engine = self.engine;
        let s = self.minds.get_mut(&id).expect("test: indexed mind"); // expect: the dyno's own state invariant, indexed mind
        let (Phase::Quoted, Some(quoted)) = (s.phase, s.quoted) else { return Applied::default() };
        let needed = quoted.tokens.saturating_sub(1).max(1);
        let mut slot = None;
        match self.owner.reserve(&mut slot, id, quoted, needed, now) {
            Ok(()) => {
                (s.phase, s.reservation, s.reserved_on) = (Phase::Reserved, slot, Some(engine));
                if !now.is_some_and(|n| n.same_identity(&quoted)) {
                    let step = Step::Reserve(i);
                    self.violations.push((self.steps, step, Violation::ReservedStaleLease { mind: id }));
                }
                Applied::default()
            }
            Err(refused) => {
                (s.phase, s.quoted) = (Phase::Idle, None);
                Applied { refused: Some(refused), ..Applied::default() }
            }
        }
    }

    fn publish(&mut self) -> Applied {
        let opportunity = self.boundary().opportunity();
        let demands: Vec<Demand<Mind>> = self.order.iter().map(|mind| {
            let s = &self.minds[mind];
            let need = test_tokens(s.need);
            Demand { mind: *mind, need, target: need, wish: need, turn: match (&s.phase, s.completed_since_grant) { (Phase::Idle, true) => crate::capacity::window_grant::Turn::Settled, (Phase::Idle, false) => crate::capacity::window_grant::Turn::Unsettled, _ => crate::capacity::window_grant::Turn::InFlight }, ceiling: self.ceiling, growth: crate::inference::planning::window_allocator::Growth::Unmeasured }
        }).collect();
        let (next, tickets) = self.owner.publish(&mut self.fair, &demands, self.pool, self.ceiling, Some(self.engine), self.current.clone());
        self.publications += 1;
        let publication = self.publications;
        let mut applied = Applied { opportunity: Some(opportunity.clone()), ..Applied::default() };
        for ticket in tickets {
            let mind = ticket.persona();
            let s = &self.minds[&mind];
            if s.owns_request() {
                self.violations.push((self.steps, Step::Publish, Violation::TicketWithCustody { mind, phase: s.phase, unresolved: s.unresolved }));
            }
            if !s.completed_since_grant && !(s.idle_at_last_publish && !s.owns_request()) {
                self.violations.push((self.steps, Step::Publish, Violation::TicketWithoutCompletion { mind }));
            }
            applied.tickets.push(self.order.iter().position(|m| *m == mind).expect("test: a ticket names a mind")); // expect: the dyno's own state invariant, a ticket names a mind
            self.tickets.insert(mind, ticket);
        }
        let retiring_now = self.observe().retiring;
        for (index, mind) in self.order.clone().into_iter().enumerate() {
            let granted = next.grant_for(mind).unwrap_or(0); // unwrap_or: no grant this publication
            let s = self.minds.get_mut(&mind).expect("test: ordered mind"); // expect: the dyno's own state invariant, ordered mind
            if s.published < s.need && granted >= s.need {
                s.completed_since_grant = false;
                s.owed_since = None;
                self.first_served.entry(mind).or_insert(publication);
                applied.newly_served.push(index);
            }
            s.idle_at_last_publish = s.published > 0 && granted > 0 && !s.owns_request();
            s.published = granted;
            let eligible = s.need <= self.ceiling.min(self.pool);
            // A mind whose publication is retiring still holds her cells: she is owed service
            // only once they are physically reclaimed (or fenced).
            if granted < s.need && eligible && !retiring_now.contains_key(&mind) {
                s.owed_since.get_or_insert(publication);
            }
        }
        let retiring = !retiring_now.is_empty();
        match opportunity {
            Ok(o @ Opportunity::Grant { waiter, need, .. }) if next.grant_for(waiter).unwrap_or(0) < need => { // unwrap_or: not granted
                self.violations.push((self.steps, Step::Publish, Violation::MissedOpportunity { publication, opportunity: o }));
            }
            // A reclaim is taken when the waiter is served at this publication: by a ticket to the
            // holder, or by an unused seat withdrawn without a handoff (an idle holder that never
            // served holds no KV here; `fair_publish`'s unused-seat rule).
            Ok(o @ Opportunity::Reclaim { waiter, .. })
                if applied.tickets.is_empty() && !retiring && next.grant_for(waiter).unwrap_or(0) < self.minds[&waiter].need => { // unwrap_or: not granted
                self.violations.push((self.steps, Step::Publish, Violation::MissedOpportunity { publication, opportunity: o }));
            }
            _ => {}
        }
        self.log.push(format!("{:>4} publish #{publication}: {:?} tickets {:?} ({:?})", self.steps,
            next.by_mind.iter().map(|(m, g)| (self.minds[m].name.clone(), *g)).collect::<Vec<_>>(),
            applied.tickets.iter().map(|i| self.name(*i).to_string()).collect::<Vec<_>>(), applied.opportunity));
        self.current = Some(next);
        applied
    }

    fn check_safety(&mut self, step: Step) {
        let seen = self.observe();
        let mut claimed = 0u64;
        for mind in &self.order {
            let s = &self.minds[mind];
            let charged = seen.charged.get(mind).copied().unwrap_or(0); // unwrap_or: nothing charged
            claimed += u64::from(s.published.max(charged).max(s.resident));
            if s.resident > s.published.max(charged) {
                self.violations.push((self.steps, step, Violation::UncoveredResident { mind: *mind, resident: s.resident, published: s.published, charged }));
            }
            if s.unresolved > 0 && seen.unresolved.get(mind).copied().unwrap_or(0) < s.unresolved as usize { // unwrap_or: none held
                self.violations.push((self.steps, step, Violation::UnresolvedReleased { mind: *mind }));
            }
            if seen.retiring.contains_key(mind) && s.published > 0 {
                self.violations.push((self.steps, step, Violation::GrantedWhileRetiring { mind: *mind }));
            }
        }
        if claimed > u64::from(self.pool) {
            self.violations.push((self.steps, step, Violation::OverPool { claimed, pool: self.pool }));
        }
    }
}
