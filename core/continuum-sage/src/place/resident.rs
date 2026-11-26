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

/// What one of a report's rows needs beside it to run at what it holds (Joel, 2026-11-26: "ask you what
/// other parts you need"): a live call at its level needs the encoder's memory; a mind's paged KV restores
/// only with her genes attached at the same revision; a mind runs only with her base model loaded. The
/// controller shrinks a row's requirement only with the row: freeing the genes while keeping the KV
/// saves nothing, so the two are one decision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Requires<P, Q> {
    /// The index of the row in this report that needs it.
    pub row: usize,
    pub place: P,
    pub amount: Q,
    pub why: &'static str,
}

/// What a resident reported: its rows, and what each needs beside it, in buffers the placer reuses. A
/// question the controller needs answered next is a new kind of entry here, never a new protocol.
#[derive(Debug)]
pub struct Report<P, Q> {
    rows: Vec<Row<P, Q>>,
    requires: Vec<Requires<P, Q>>,
}

impl<P, Q> Default for Report<P, Q> {
    fn default() -> Self {
        Self { rows: Vec::new(), requires: Vec::new() }
    }
}

impl<P, Q> Report<P, Q> {
    /// Add a row; answers its index, for what it requires.
    pub fn row(&mut self, row: Row<P, Q>) -> usize {
        self.rows.push(row);
        self.rows.len() - 1
    }

    /// Row `row` needs `amount` on `place` beside it.
    pub fn requires(&mut self, row: usize, place: P, amount: Q, why: &'static str) {
        self.requires.push(Requires { row, place, amount, why });
    }

    pub fn rows(&self) -> &[Row<P, Q>] {
        &self.rows
    }

    /// What `row` needs beside it.
    pub fn requirements(&self, row: usize) -> impl Iterator<Item = &Requires<P, Q>> + '_ {
        self.requires.iter().filter(move |requires| requires.row == row)
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.requires.clear();
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

/// What the law asks a holder to give back: on `place`, down to `to`, so that a wait of class `for_qos`
/// fits. One struct, so the contract grows (a deadline, the wait's id) without changing any holder's
/// signature, and every ask is a record the replay log keeps beside its answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShrinkAsk<P, Q> {
    pub place: P,
    pub to: Q,
    /// The class of the wait it makes room for: why it is asked.
    pub for_qos: Qos,
}

/// Room the law grants a holder: on `place`, up to `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowGrant<P, Q> {
    pub place: P,
    pub to: Q,
}

/// A holder of what a placer places. Each method takes one params struct and returns one result struct
/// (the command shape, `Commands.execute<P, R>`): the contract every holder implements, and the record
/// of every exchange.
pub trait Resident<P, Q> {
    /// Every place it holds on, one row each, written into `into` (cleared by the caller). Required: a
    /// holder states what it holds and how it varies, or it is not a Resident.
    fn report(&self, into: &mut Report<P, Q>);

    /// Give back as asked, the cheapest way it has. Answers at once; never waits.
    fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Given<Q>;

    /// Take the room granted: expand a collapsed level, page a cached copy back in. Answers at once; a grow
    /// that could not fit is `Refused`, never silence.
    fn grow(&mut self, grant: GrowGrant<P, Q>) -> Given<Q>;
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
    /// Share `to` on `place` among `report`'s rows. A `Fixed` or `Whole` child keeps all it holds (a whole
    /// holding is asked as a whole, by the law, never split here); an elastic child runs on its floor and
    /// keeps what it holds in priority order, so the lowest give first; what a held row requires on this
    /// place stays with it, charged like a fixed holding (freeing it would leave the row useless).
    /// `priority` is the law's (QoS aged by wait, learned), never this function's. Answers each row's
    /// target in row order: `None` for a row on another place.
    pub fn shrink<P: Copy + PartialEq>(&mut self, report: &Report<P, Q>, place: P, to: Q, priority: impl Fn(&Row<P, Q>) -> u8) -> &[Option<Q>] {
        self.claims.clear();
        let rows = report.rows();
        for row in rows.iter().filter(|row| row.place == place) {
            let floor = match row.varies {
                Varies::Elastic { floor, .. } => floor,
                Varies::Fixed { .. } | Varies::Whole { .. } => row.held,
            };
            self.claims.push(super::Flex { priority: priority(row), floor, min: row.held, max: row.held, required: true });
        }
        let on_rows = self.claims.len();
        let required = report.requires.iter().filter(|requires| requires.place == place && rows.get(requires.row).is_some_and(|row| row.held.units() > 0));
        for requires in required {
            self.claims.push(super::Flex { priority: u8::MAX, floor: requires.amount, min: requires.amount, max: requires.amount, required: true });
        }
        self.flexbox.share(to, &self.claims, |a, b| a.cmp(&b), &mut self.shares);
        self.targets.clear();
        let mut on = self.shares[..on_rows].iter();
        for row in rows {
            self.targets.push(if row.place == place { on.next().map(|share| share.amount) } else { None });
        }
        &self.targets
    }
}
