//! A holder of what a placer places, as an object (UNIFIED-PAGING.md, "Holders are objects"; Joel,
//! 2026-11-26: "you do this with objects. The interface like the lifecycle one, allows feedback and
//! control, both directions"; "You can force it to report by its contract. That's why good OOP works").
//!
//! The contract forces the report: [`Resident::report`] has no default, so nothing is placed without
//! stating, for every place it holds on, what it holds, what it wants, how it can vary and why not, its
//! QoS class and how long it has waited. The controller is never blind and never guesses: a holding is
//! [`Varies::Fixed`] only because its holder said so and why, never because it said nothing.
//!
//! Control comes down synchronously and never waits ([`Resident::shrink`], [`Resident::grow`]): a holder
//! answers at once with what it did, or `Pending` while its fence is in flight, and the fence's landing is
//! a row of its next report (Cormac on e7b8b9ff9: the placer never awaits a holder, and the trait stays
//! object safe so a composite holds mixed children). Every answer and every report is a measurement the
//! law's replay log keeps.
//!
//! Like `persona::Lifecycle`, a parent is a Resident of its children: its report folds theirs, and a
//! shrink it receives is shared among them by the flexbox at their QoS. That is how one interface reaches
//! from a mind's KV to a node and on to the grid.

use std::time::Duration;

use crate::decide::Measured;
use crate::Qos;

/// What a way of giving back costs, measured: how long it takes, and what quality or speed it loses
/// (in the holder's own unit, comparable across its ways). Unmeasured until it has been done once.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cost {
    pub takes: Measured<Duration>,
    pub loses: Measured<f64>,
}

/// The ways a holding can give back on a place. A way the holding does not have is `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ways<P> {
    /// Run on less where it is (a workspace re-reserved smaller, a stream sized to its occupant).
    pub resize: Option<Cost>,
    /// Move its bytes down a tier, to `.0`: a destination claim for the copy and its staging peak first,
    /// the source charged until the copy's fence and native release (Codex on e7b8b9ff9).
    pub cache_out: Option<(P, Cost)>,
    /// Keep a cheaper level (older context summarized behind a handle; a lesson at fewer rows).
    pub collapse: Option<Cost>,
}

/// How a holding on one place can vary, from its holder's own knowledge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Varies<P, Q> {
    /// It cannot run on less while it serves: never asked, and the place's ceiling is short of it.
    Fixed { why: &'static str },
    /// Anything from `floor` up runs; `ways` are how it gives back.
    Elastic { floor: Q, ways: Ways<P> },
    /// All or nothing: only a wait that outranks every one of its `dependents` (minds on a base model) may
    /// ask it, and the answer is the whole holding moved or cached out, never a fraction.
    Whole { dependents: u32, ways: Ways<P> },
}

/// One row of a report: a holding on one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row<P, Q> {
    pub place: P,
    /// Measured from its allocator, never planned.
    pub held: Q,
    /// What its measured use would take.
    pub want: Q,
    pub varies: Varies<P, Q>,
    pub qos: Qos,
    /// How long it has waited for room in its class (zero when it waits for nothing).
    pub waited: Duration,
}

/// What a resident reported: its rows, in a buffer the placer reuses.
#[derive(Debug)]
pub struct Report<P, Q> {
    rows: Vec<Row<P, Q>>,
}

impl<P, Q> Default for Report<P, Q> {
    fn default() -> Self {
        Self { rows: Vec::new() }
    }
}

impl<P, Q> Report<P, Q> {
    pub fn row(&mut self, row: Row<P, Q>) {
        self.rows.push(row);
    }

    pub fn rows(&self) -> &[Row<P, Q>] {
        &self.rows
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

/// Which way a holder gave back or took room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Resize,
    CacheOut,
    Collapse,
    /// Grow only: a collapsed level expanded or a cached copy paged back in.
    Restore,
}

/// A holder's answer to a shrink or a grow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Given<Q> {
    /// Done and fenced: `amount` released (a shrink) or taken (a grow), by `way`, in `took`.
    Done { way: Way, amount: Q, took: Duration },
    /// Started; its fence lands as a row of the holder's next report.
    Pending { way: Way },
    /// It could not, and why: the law asks the next holder.
    Refused { why: &'static str },
}

/// A holder of what a placer places.
pub trait Resident<P, Q> {
    /// Every place it holds on, one row each, written into `into` (cleared by the caller). Required: a
    /// holder states what it holds and how it varies, or it is not a Resident.
    fn report(&self, into: &mut Report<P, Q>);

    /// Give back on `place` down to `to`, the cheapest way it has. Answers at once; never waits.
    fn shrink(&mut self, place: P, to: Q) -> Given<Q>;

    /// Room granted on `place` up to `to`: expand a collapsed level, page a cached copy back in. Answers
    /// at once; a grow that could not fit is `Refused`, never silence.
    fn grow(&mut self, place: P, to: Q) -> Given<Q>;
}

/// How a parent shares a shrink it received among its children: the one flexbox over their rows on the
/// place. Kept by the parent and reused, so a sharing allocates nothing past its high-water mark.
#[derive(Debug, Default)]
pub struct Sharing<Q> {
    flexbox: super::Flexbox,
    claims: Vec<super::Flex<Q>>,
    shares: Vec<super::Share<Q>>,
    targets: Vec<Option<Q>>,
}

impl<Q: super::Quantity> Sharing<Q> {
    /// Share `to` on `place` among `rows`. A `Fixed` or `Whole` child keeps all it holds (a whole holding is
    /// asked as a whole, by the law, never split here); an elastic child runs on its floor and keeps what it
    /// holds in priority order, so the lowest give first. `priority` is the law's (QoS aged by wait,
    /// learned), never this function's. Answers each row's target in row order: `None` for a row on
    /// another place.
    pub fn shrink<P: Copy + PartialEq>(&mut self, rows: &[Row<P, Q>], place: P, to: Q, priority: impl Fn(&Row<P, Q>) -> u8) -> &[Option<Q>] {
        self.claims.clear();
        for row in rows.iter().filter(|row| row.place == place) {
            let floor = match row.varies {
                Varies::Elastic { floor, .. } => floor,
                Varies::Fixed { .. } | Varies::Whole { .. } => row.held,
            };
            self.claims.push(super::Flex { priority: priority(row), floor, min: row.held, max: row.held, required: true });
        }
        self.flexbox.share(to, &self.claims, |a, b| a.cmp(&b), &mut self.shares);
        self.targets.clear();
        let mut on = self.shares.iter();
        for row in rows {
            self.targets.push(if row.place == place { on.next().map(|share| share.amount) } else { None });
        }
        &self.targets
    }
}
