//! A holder made of holders (UNIFIED-PAGING.md, "Holders are objects": a mind over her parts, the Core over
//! its minds and workspace, a node over its engines). It is itself a [`Resident`]: its report is its parts'
//! reports folded, and a shrink or a grow it receives is shared among its parts by the one flexbox, at the
//! law's priority, the lowest giving first and the highest growing first. No holder writes this again.

use std::ops::Range;
use std::time::Duration;

use super::quantity::Quantity;
use super::resident::{Given, GrowGrant, Report, Resident, Row, Sharing, ShrinkAsk, Way};

/// The law's priority for a row (its QoS class, aged by its wait: learned, never a constant here).
pub type Priority<P, Q> = fn(&Row<P, Q>) -> u8;

/// Parts of any kind, held as one.
pub struct Composite<P, Q> {
    parts: Vec<Box<dyn Resident<P, Q> + Send>>,
    priority: Priority<P, Q>,
    /// The last fold, and the rows each part wrote into it.
    rows: Report<P, Q>,
    spans: Vec<Range<usize>>,
    sharing: Sharing<Q>,
    targets: Vec<Option<Q>>,
}

impl<P: Copy + PartialEq, Q: Quantity> Composite<P, Q> {
    pub fn new(priority: Priority<P, Q>) -> Self {
        Self { parts: Vec::new(), priority, rows: Report::default(), spans: Vec::new(), sharing: Sharing::default(), targets: Vec::new() }
    }

    pub fn push(&mut self, part: Box<dyn Resident<P, Q> + Send>) {
        self.parts.push(part);
    }

    pub fn parts(&self) -> usize {
        self.parts.len()
    }

    /// Fold every part's report into `rows`, noting the rows each wrote.
    fn fold(&mut self) {
        self.rows.clear();
        self.spans.clear();
        for part in &self.parts {
            let start = self.rows.rows().len();
            part.report(&mut self.rows);
            self.spans.push(start..self.rows.rows().len());
        }
    }

    /// Each part's (held now, target) on `place`, from the last fold and `targets`.
    fn per_part(&self, place: P, part: usize) -> (u64, u64) {
        let rows = &self.rows.rows()[self.spans[part].clone()];
        let targets = &self.targets[self.spans[part].clone()];
        rows.iter().zip(targets).filter(|(row, _)| row.place == place).fold((0, 0), |(held, to), (row, target)| {
            (held + row.held.units(), to + target.map_or(row.held.units(), Quantity::units)) // map_or: a row the sharing left alone keeps what it holds
        })
    }

    /// Ask each part whose target differs from what it holds, and fold their answers into one.
    fn ask(&mut self, place: P, mut each: impl FnMut(&mut (dyn Resident<P, Q> + Send), P, Q) -> Given<Q>, grows: bool) -> Given<Q> {
        let mut answer = Answers::default();
        for part in 0..self.parts.len() {
            let (held, to) = self.per_part(place, part);
            if (grows && to > held) || (!grows && to < held) {
                answer.add(each(self.parts[part].as_mut(), place, Q::of(to)));
            }
        }
        answer.into_given()
    }
}

impl<P: Copy + PartialEq, Q: Quantity> Resident<P, Q> for Composite<P, Q> {
    fn report(&self, into: &mut Report<P, Q>) {
        for part in &self.parts {
            part.report(into);
        }
    }

    fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Given<Q> {
        self.fold();
        let targets = self.sharing.shrink(&self.rows, ask.place, ask.to, self.priority);
        self.targets.clear();
        self.targets.extend_from_slice(targets);
        let for_qos = ask.for_qos;
        self.ask(ask.place, |part, place, to| part.shrink(ShrinkAsk { place, to, for_qos }), false)
    }

    fn grow(&mut self, grant: GrowGrant<P, Q>) -> Given<Q> {
        self.fold();
        let targets = self.sharing.grow(&self.rows, grant.place, grant.to, self.priority);
        self.targets.clear();
        self.targets.extend_from_slice(targets);
        self.ask(grant.place, |part, place, to| part.grow(GrowGrant { place, to }), true)
    }
}

/// The parts' answers, folded: what was done in all, the longest it took, and whether any is still pending.
#[derive(Default)]
struct Answers {
    amount: u64,
    took: Duration,
    way: Option<Way>,
    pending: Option<Way>,
    asked: usize,
    refused: Option<&'static str>,
}

impl Answers {
    fn add<Q: Quantity>(&mut self, given: Given<Q>) {
        self.asked += 1;
        match given {
            Given::Done { way, amount, took } => {
                self.amount += amount.units();
                self.took = self.took.max(took);
                self.way.get_or_insert(way);
            }
            Given::Pending { way } => {
                self.pending.get_or_insert(way);
            }
            Given::Refused { why } => {
                self.refused.get_or_insert(why);
            }
        }
    }

    fn into_given<Q: Quantity>(self) -> Given<Q> {
        match (self.pending, self.way, self.refused) {
            (Some(way), _, _) => Given::Pending { way },
            (None, Some(way), _) => Given::Done { way, amount: Q::of(self.amount), took: self.took },
            (None, None, Some(why)) => Given::Refused { why },
            // nothing differed from what the parts hold: done, with nothing moved
            (None, None, None) => Given::Done { way: Way::Resize, amount: Q::of(0), took: Duration::ZERO },
        }
    }
}
