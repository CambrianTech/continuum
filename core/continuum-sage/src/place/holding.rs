//! The parent of every holder (SAGE.md, "Intermediate parents": Resident). A leaf declares only its rows
//! and the bodies of its ways; the parent does the rest, once, for every holder: it answers asks through the
//! leaf, holds a Pending answer until its fence lands, keeps every ask with its answer as a receipt (the
//! feedback the room law learns from), and re-reads the leaf's report so its owner publishes only on change.
//!
//! An owner keeps a `Holding` around its leaf and calls in on its own thread: `shrink` and `grow` when the
//! law asks, `landed` when a fence it started completes, `refresh` after its own state changed. Nothing here
//! waits.

use std::time::Duration;

use super::resident::{Given, GrowGrant, Report, Resident, ShrinkAsk, Way};

/// What the law asked a holder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Asked<P, Q> {
    Shrink(ShrinkAsk<P, Q>),
    Grow(GrowGrant<P, Q>),
}

/// One exchange, as the law's replay keeps it: what was asked, what the holder answered, and the report
/// revision it answered from. A Pending answer gets a second receipt when its fence lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Receipt<P, Q> {
    pub asked: Asked<P, Q>,
    pub given: Given<Q>,
    pub revision: u64,
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
    /// Asks answered Pending, awaiting their fence.
    pending: Vec<Asked<P, Q>>,
    /// Exchanges since the owner last drained them.
    receipts: Vec<Receipt<P, Q>>,
}

impl<P: Copy + PartialEq, Q: Copy + PartialEq, R: Resident<P, Q>> Holding<P, Q, R> {
    /// The parent around `leaf`, its first report read.
    pub fn new(leaf: R) -> Self {
        let mut holding = Self { leaf, report: Report::default(), revision: 0, fresh: Report::default(), pending: Vec::new(), receipts: Vec::new() };
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

    /// The law asks the leaf to give back; its answer is kept as a receipt and the report read again.
    pub fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Given<Q> {
        let given = self.leaf.shrink(ask);
        self.answered(Asked::Shrink(ask), given)
    }

    /// The law grants the leaf room; its answer is kept as a receipt and the report read again.
    pub fn grow(&mut self, grant: GrowGrant<P, Q>) -> Given<Q> {
        let given = self.leaf.grow(grant);
        self.answered(Asked::Grow(grant), given)
    }

    /// A fence the leaf started for `place` has landed: `amount` moved by `way` in `took`. Closes the oldest
    /// Pending ask on that place with a Done receipt. Answers whether one was open.
    pub fn landed(&mut self, place: P, way: Way, amount: Q, took: Duration) -> bool {
        let Some(at) = self.pending.iter().position(|asked| match asked {
            Asked::Shrink(ask) => ask.place == place,
            Asked::Grow(grant) => grant.place == place,
        }) else {
            return false;
        };
        let asked = self.pending.remove(at);
        self.refresh();
        self.receipts.push(Receipt { asked, given: Given::Done { way, amount, took }, revision: self.revision });
        true
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

    fn answered(&mut self, asked: Asked<P, Q>, given: Given<Q>) -> Given<Q> {
        if matches!(given, Given::Pending { .. }) {
            self.pending.push(asked);
        }
        self.refresh();
        self.receipts.push(Receipt { asked, given, revision: self.revision });
        given
    }
}
