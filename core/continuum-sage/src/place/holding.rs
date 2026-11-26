//! The parent of every holder (SAGE.md, "Intermediate parents": Resident). A leaf declares only its rows
//! and the bodies of its ways; the parent does the rest, once, for every holder: it answers asks through the
//! leaf, gives each exchange a ticket, holds a Pending answer until the fence of that ticket lands, keeps
//! every exchange as a receipt (the feedback the room law learns from), and re-reads the leaf's report so its
//! owner publishes only on change.
//!
//! An owner keeps a `Holding` around its leaf and calls in on its own thread: `shrink` and `grow` when the
//! law asks, `landed` with the ticket when a fence it started completes, `refresh` after its own state
//! changed. Nothing here waits.

use std::time::Duration;

use super::resident::{Given, GrowGrant, Report, Resident, ShrinkAsk, Way};

/// What the law asked a holder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Asked<P, Q> {
    Shrink(ShrinkAsk<P, Q>),
    Grow(GrowGrant<P, Q>),
}

/// One exchange's identity, issued by the parent: a fence names the ask it completes, so a late or
/// duplicate completion, or one of two asks on the same place, can never close another (Codex on the
/// delivery design, 2026-11-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

/// The leaf's answer to one ask, with the ticket its fence will name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Answered<Q> {
    pub ticket: Ticket,
    pub given: Given<Q>,
}

/// One exchange, as the law's replay keeps it: its ticket, what was asked, what the holder answered, and the
/// report revision it answered from. A Pending answer gets a second receipt when its fence lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Receipt<P, Q> {
    pub ticket: Ticket,
    pub asked: Asked<P, Q>,
    pub given: Given<Q>,
    pub revision: u64,
}

/// What a fence's landing did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landing {
    /// It closed its pending ask.
    Settled,
    /// No ask is pending under that ticket: already settled, never issued, or answered at once. Nothing changed.
    Unknown,
}

/// A leaf and everything its parent keeps for it.
#[derive(Debug)]
pub struct Holding<P, Q, R> {
    leaf: R,
    /// The report as last read from the leaf, and its revision (bumped on every change).
    report: Report<P, Q>,
    revision: u64,
    /// Scratch for the next read, so a refresh allocates nothing past its high-water mark.
    fresh: Report<P, Q>,
    next: u64,
    /// Asks answered Pending, by ticket, awaiting their fence.
    pending: Vec<(Ticket, Asked<P, Q>)>,
    /// Exchanges since the owner last drained them.
    receipts: Vec<Receipt<P, Q>>,
}

impl<P: Copy + PartialEq, Q: Copy + PartialEq, R: Resident<P, Q>> Holding<P, Q, R> {
    /// The parent around `leaf`, its first report read.
    pub fn new(leaf: R) -> Self {
        let mut holding = Self { leaf, report: Report::default(), revision: 0, fresh: Report::default(), next: 0, pending: Vec::new(), receipts: Vec::new() };
        holding.refresh();
        holding
    }

    /// The leaf, for its owner's own work; call [`Self::refresh`] after changing it.
    pub fn leaf_mut(&mut self) -> &mut R {
        &mut self.leaf
    }

    pub fn leaf(&self) -> &R {
        &self.leaf
    }

    /// The report as last read, and its revision.
    pub fn report(&self) -> (&Report<P, Q>, u64) {
        (&self.report, self.revision)
    }

    /// Read the leaf's report again; true when it changed (the owner publishes it).
    pub fn refresh(&mut self) -> bool {
        self.fresh.clear();
        self.leaf.report(&mut self.fresh);
        if self.fresh == self.report {
            return false;
        }
        self.report.copy_from(&self.fresh);
        self.revision += 1;
        true
    }

    /// The law asks the leaf to give back; its answer is kept as a receipt under a new ticket.
    pub fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Answered<Q> {
        let given = self.leaf.shrink(ask);
        self.answered(Asked::Shrink(ask), given)
    }

    /// The law grants the leaf room; its answer is kept as a receipt under a new ticket.
    pub fn grow(&mut self, grant: GrowGrant<P, Q>) -> Answered<Q> {
        let given = self.leaf.grow(grant);
        self.answered(Asked::Grow(grant), given)
    }

    /// The fence of `ticket` has landed: `amount` moved by `way` in `took`. Settles that ask once, with a Done
    /// receipt; any other ticket changes nothing.
    pub fn landed(&mut self, ticket: Ticket, way: Way, amount: Q, took: Duration) -> Landing {
        let Some(at) = self.pending.iter().position(|(pending, _)| *pending == ticket) else {
            return Landing::Unknown;
        };
        let (_, asked) = self.pending.swap_remove(at);
        self.refresh();
        self.receipts.push(Receipt { ticket, asked, given: Given::Done { way, amount, took }, revision: self.revision });
        Landing::Settled
    }

    /// Whether an ask is waiting on its fence.
    pub fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The exchanges since the last drain, into `out` (cleared first): the law's feedback.
    pub fn receipts(&mut self, out: &mut Vec<Receipt<P, Q>>) {
        out.clear();
        out.append(&mut self.receipts);
    }

    fn answered(&mut self, asked: Asked<P, Q>, given: Given<Q>) -> Answered<Q> {
        let ticket = Ticket(self.next);
        self.next += 1;
        if matches!(given, Given::Pending { .. }) {
            self.pending.push((ticket, asked));
        }
        self.refresh();
        self.receipts.push(Receipt { ticket, asked, given, revision: self.revision });
        Answered { ticket, given }
    }
}
