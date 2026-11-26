//! The levels between the Resident contract and a leaf (Joel, 2026-11-26: "you reduce the difficulty of the
//! contracts (burden) with multi level inheritance. This part is different so only this part needs the
//! abstract method, which need to pass this to, and it return this").
//!
//! - [`Resident`] is the whole contract: report, shrink, grow. Few holders implement it directly (a composite,
//!   the kernel's adapter).
//! - [`Fixed`] is a holder with no code of its own: its bytes cannot vary while it serves, and it says why.
//! - [`Resizing`] owns a row's facts and implements the whole contract, so its leaf implements one method:
//!   [`Resize::resize`], told where it is and where to go, answering what moved. The level checks every ask
//!   against the floor and the want, builds the row, and turns the answer into the contract's.
//!
//! On every level the parent [`super::Holding`] adds tickets, fences and receipts, so a leaf never sees them.

use std::time::Duration;

use super::quantity::Quantity;
use super::resident::{Cost, Given, GrowGrant, Report, Resident, Row, ShrinkAsk, Varies, Way, Ways};
use crate::Qos;

/// Where a row stands: the facts every level reports the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Facts<P, Q> {
    pub place: P,
    /// Measured from its allocator, never planned.
    pub held: Q,
    pub qos: Qos,
    /// How long it has waited for room in its class.
    pub waited: Duration,
}

/// A holding nothing can shrink while it serves (a base model's weights under live minds). No code of its
/// own: the level reports it Fixed and answers every ask with its reason.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fixed<P, Q> {
    pub facts: Facts<P, Q>,
    pub why: &'static str,
}

impl<P: Copy, Q: Quantity> Resident<P, Q> for Fixed<P, Q> {
    fn report(&self, into: &mut Report<P, Q>) {
        let Facts { place, held, qos, waited } = self.facts;
        into.row(Row { place, held, want: held, varies: Varies::Fixed { why: self.why }, qos, waited });
    }
    fn shrink(&mut self, _: ShrinkAsk<P, Q>) -> Given<Q> {
        Given::Refused { why: self.why }
    }
    fn grow(&mut self, _: GrowGrant<P, Q>) -> Given<Q> {
        Given::Refused { why: self.why }
    }
}

/// Where a resize starts and where it is asked to end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeTo<Q> {
    pub from: Q,
    pub to: Q,
}

/// What a resize did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Moved<Q> {
    /// It now holds `now`, in `took`.
    Done { now: Q, took: Duration },
    /// Started; its owner reports the fence to the parent (`Holding::landed`) with the bytes it moved.
    Pending,
    /// It could not, and why.
    Refused { why: &'static str },
}

/// The one thing a resizable holder does differently: change how much it holds. Its owner keeps the
/// allocation; this method moves it.
pub trait Resize<Q> {
    fn resize(&mut self, ask: ResizeTo<Q>) -> Moved<Q>;
}

/// A holding that runs on anything from its floor up to its want, by resizing: the whole contract on one
/// level, its leaf the one method of [`Resize`].
#[derive(Debug, Clone, PartialEq)]
pub struct Resizing<P, Q, W> {
    pub facts: Facts<P, Q>,
    pub floor: Q,
    /// What its measured use would take: the most a grow takes it to.
    pub want: Q,
    /// What a resize costs, measured (unmeasured until it has run once).
    pub cost: Cost,
    pub leaf: W,
}

impl<P: Copy + PartialEq, Q: Quantity, W: Resize<Q>> Resizing<P, Q, W> {
    fn moved(&mut self, to: Q) -> Given<Q> {
        let from = self.facts.held;
        match self.leaf.resize(ResizeTo { from, to }) {
            Moved::Done { now, took } => {
                self.facts.held = now;
                let amount = Q::of(from.units().abs_diff(now.units()));
                Given::Done { way: Way::Resize, amount, took }
            }
            Moved::Pending => Given::Pending { way: Way::Resize },
            Moved::Refused { why } => Given::Refused { why },
        }
    }
}

impl<P: Copy + PartialEq, Q: Quantity, W: Resize<Q>> Resident<P, Q> for Resizing<P, Q, W> {
    fn report(&self, into: &mut Report<P, Q>) {
        let Facts { place, held, qos, waited } = self.facts;
        let ways = Ways { resize: Some(self.cost), cache_out: None, collapse: None };
        into.row(Row { place, held, want: self.want, varies: Varies::Elastic { floor: self.floor, ways }, qos, waited });
    }

    /// Down to the ask, never below its floor; an ask for another place is not its own.
    fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Given<Q> {
        if ask.place != self.facts.place {
            return Given::Refused { why: "it holds nothing on that place" };
        }
        let to = ask.to.max(self.floor);
        if to >= self.facts.held {
            return Given::Refused { why: "it holds no more than its floor" };
        }
        self.moved(to)
    }

    /// Up to the grant, never past its want.
    fn grow(&mut self, grant: GrowGrant<P, Q>) -> Given<Q> {
        if grant.place != self.facts.place {
            return Given::Refused { why: "it holds nothing on that place" };
        }
        let to = grant.to.min(self.want);
        if to <= self.facts.held {
            return Given::Refused { why: "it already holds its want" };
        }
        self.moved(to)
    }
}
