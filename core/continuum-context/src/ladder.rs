//! The one contract every budget consumer answers (Joel, 2026-11-07: "The rag budgeting should
//! collapse rather than crap out"; "It should know how each budgeted item reduces or collapses").
//!
//! A consumer (her tools, the tool menu in her framing, a grounding source, her newest result, the
//! required input) is a list of ITEMS, each with its own LEVELS: most detail first, each smaller
//! than the one above, and the last its HARD LIMIT (its pointer, an expand handle; or, for input
//! she must read whole, the whole itself). Every cost is measured from the rendered item by its
//! producer, in the allocation's unit; nothing here is a size.
//!
//! [`Leveled::reduce`] answers "what is the most you can show within this target": the same rule
//! the view's start applies to its sources' items (`view::allot`), newest (most important) item
//! first, each at the most detail that still fits, and an older item never more whole than a newer
//! one, so shrinking is oldest (least important) first and never her newest.
//! [`Leveled::options`] lists every distinct answer, most detail first: the ladder the act's search
//! chooses among ([`crate::allocation::search`]).

/// One item's levels in the allocation's unit, most detail first, each strictly smaller than the
/// one above. The last is the item's hard limit: never shown smaller than it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rungs(Vec<u32>);

impl Rungs {
    /// An item's levels as its producer measured them, most detail first. A level no smaller than
    /// the one kept above it saves nothing and is not a level (`levels::is_a_level`, the one rule):
    /// it is skipped. `None` for an item with no level at all.
    pub fn new(costs: impl IntoIterator<Item = u32>) -> Option<Self> {
        let mut kept: Vec<u32> = Vec::new();
        for cost in costs {
            if kept.last().is_none_or(|&above| cost < above) {
                kept.push(cost);
            }
        }
        (!kept.is_empty()).then_some(Self(kept))
    }

    /// An item's levels from its whole down, each with what it is (`tag`, the producer's own name
    /// for that level), aligned: the tags returned are exactly one per rung, in rung order. A level
    /// no smaller than the one kept above it is not a level and leaves with its tag. It cannot
    /// fail: the whole is always a level, so a producer never needs a fallback for "no levels"
    /// (a fallback could only price a real item at a level it does not have).
    pub fn kept<T>(whole: u32, whole_tag: T, smaller: impl IntoIterator<Item = (u32, T)>) -> (Self, Vec<T>) {
        let mut costs = vec![whole];
        let mut tags = vec![whole_tag];
        for (cost, tag) in smaller {
            if costs.last().is_some_and(|&above| cost < above) {
                costs.push(cost);
                tags.push(tag);
            }
        }
        (Self(costs), tags)
    }

    /// An item with one level: shown whole or not at all, and never smaller (input she must read).
    pub fn whole(cost: u32) -> Self {
        Self(vec![cost])
    }

    pub fn costs(&self) -> &[u32] {
        &self.0
    }

    pub fn full(&self) -> u32 {
        self.0[0]
    }

    /// Its hard limit: its last level.
    pub fn limit(&self) -> u32 {
        self.0[self.0.len() - 1]
    }

    /// What its level `rung` is, as the view names levels: its first whole, its last its pointer,
    /// any between a summary.
    pub fn class(&self, rung: usize) -> crate::level::Level {
        use crate::level::Level;
        if rung == 0 {
            Level::Full
        } else if rung + 1 == self.0.len() {
            Level::Pointer
        } else {
            Level::Summary
        }
    }
}

/// What a consumer IS to her, which decides the least a CHOICE may show of it above its hard limit
/// (the time price of context, `allocation::priced`). Its hard limit itself never moves: a window
/// too small for more still shows any consumer at its hard limit, the one path to a pointer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Consumer {
    /// What she reads (her conversation, her grounding, results): a choice may show it at its
    /// hard limit.
    #[default]
    Context,
    /// What she acts through (her tools, her held card): a choice never shows an item below its
    /// summary. A verb at its bare name costs her a `help` call and an extra act to use, which no
    /// value the allocation weighs can see (the dyno on #5822: the price took her tool schemas
    /// from 8.5-10.4k tokens to 245-717, names only).
    Actionable,
}

impl Consumer {
    /// `leveled` as a choice may show it: unchanged for context; for an actionable consumer each
    /// item without its pointer (an item with a summary keeps down to it, one whole and pointer
    /// only stays whole).
    pub fn choosable(self, leveled: &Leveled) -> Leveled {
        match self {
            Consumer::Context => leveled.clone(),
            Consumer::Actionable => Leveled::new(leveled.items.iter().map(Rungs::above_pointer).collect()),
        }
    }
}

impl Rungs {
    /// Its levels without its pointer: down to its summary, or whole when it has none.
    fn above_pointer(&self) -> Rungs {
        let keep = self.0.len().saturating_sub(1).max(1);
        Rungs(self.0[..keep].to_vec())
    }
}

/// A consumer: its items, least important first (the oldest, or the least used), so the most
/// important item is the last and keeps detail longest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Leveled {
    items: Vec<Rungs>,
}

/// What a consumer shows under one target: each item's level (an index into its [`Rungs`]) and
/// what they cost together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    pub rungs: Vec<usize>,
    pub cost: u32,
}

impl Leveled {
    pub fn new(items: Vec<Rungs>) -> Self {
        Self { items }
    }

    /// One item: required input, her last move.
    pub fn one(item: Rungs) -> Self {
        Self { items: vec![item] }
    }

    pub fn items(&self) -> &[Rungs] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Every item whole: what it wants.
    pub fn want(&self) -> u32 {
        self.items.iter().map(Rungs::full).fold(0, u32::saturating_add)
    }

    /// Every item at its hard limit: the least it can be. Derived, never declared: a consumer
    /// cannot claim a floor its items do not have.
    pub fn hard_limit(&self) -> u32 {
        self.items.iter().map(Rungs::limit).fold(0, u32::saturating_add)
    }

    /// The cost of `rungs` (one level per item).
    pub fn cost_of(&self, rungs: &[usize]) -> u32 {
        self.items.iter().zip(rungs).map(|(item, &rung)| item.costs()[rung]).fold(0, u32::saturating_add)
    }

    /// The most it can show within `target`: every item starts at its hard limit; what the target
    /// leaves above them is spent most important item first, each taking the most detail that
    /// still fits and never a level of more detail than the item after it holds (whole above
    /// summary above pointer, `Rungs::class`). An item with one level is its own hard limit and
    /// holds no ceiling. A target below the hard limit answers the hard limit: the caller compares.
    pub fn reduce(&self, target: u32) -> Choice {
        use crate::level::Level;
        let rank = |level: Level| match level {
            Level::Full | Level::Repeat { .. } => 2u8,
            Level::Summary | Level::Page => 1,
            Level::Pointer | Level::Folded => 0,
        };
        let mut rungs: Vec<usize> = self.items.iter().map(|item| item.costs().len() - 1).collect();
        let mut spare = target.saturating_sub(self.hard_limit());
        let mut ceiling = rank(Level::Full);
        for (index, item) in self.items.iter().enumerate().rev() {
            if item.costs().len() == 1 {
                continue;
            }
            let floor = item.limit();
            let budget = spare.saturating_add(floor);
            // An item with no level between its whole and its pointer is its own summary: under a
            // summary ceiling it stays whole when it fits, and the ceiling holds (the view's rule,
            // `view::allot`; Cormac on #5426: a short result older than one big read fell to its
            // pointer and collapsed every older item with it).
            let own_summary = item.costs().len() <= 2;
            let allowed = |rung: usize| {
                let rank_of = rank(item.class(rung));
                rank_of <= ceiling || (rung == 0 && own_summary && ceiling == rank(Level::Summary))
            };
            let rise = item.costs().iter().enumerate().find(|&(rung, &cost)| allowed(rung) && cost <= budget);
            let (rung, cost) = match rise {
                Some((rung, &cost)) => (rung, cost),
                None => (item.costs().len() - 1, floor),
            };
            rungs[index] = rung;
            spare = budget - cost;
            if !(rung == 0 && own_summary && ceiling == rank(Level::Summary)) {
                ceiling = rank(item.class(rung));
            }
        }
        let cost = self.cost_of(&rungs);
        Choice { rungs, cost }
    }

    /// Every distinct answer [`Self::reduce`] gives, most detail first, from every item whole to
    /// every item at its hard limit: the ladder the act's search chooses among. Each is the most it
    /// can show within the next smaller cost, so no answer is dominated by another of equal cost.
    pub fn options(&self) -> Vec<Choice> {
        let mut options = vec![self.reduce(self.want())];
        let limit = self.hard_limit();
        while let Some(last) = options.last() {
            if last.cost <= limit {
                break;
            }
            let next = self.reduce(last.cost - 1);
            if next.cost >= last.cost {
                break;
            }
            options.push(next);
        }
        options
    }

    /// The answer at its hard limit.
    pub fn at_limit(&self) -> Choice {
        let rungs: Vec<usize> = self.items.iter().map(|item| item.costs().len() - 1).collect();
        Choice { cost: self.cost_of(&rungs), rungs }
    }
}

#[cfg(test)]
mod tests;
