//! Her act's tail, judged and rendered by the view before it is sent, and joining her view once it
//! is served (MIND-CONTEXT, "Revision, 2026-11-11: her act's tail is a source"). The tail (her call,
//! its result, the ask, the act's framing and working context) is a [`Source`] for its rendering,
//! keys, element identity and repeat judgement, but never a start source: its claims are the act's
//! own in the one allocation (REQUIRED, NEWEST, LAST_MOVE), so a start grants it nothing, and a
//! turnover starts from her view's sources, its tail judged after.
//!
//! Two steps, because an act can be rendered several times before one is served (a capacity
//! refusal's refit, a grant that moved, an engine that cannot count it): [`View::judge_tail`] is
//! pure, so every re-render judges afresh and nothing unsent ever enters her epoch;
//! [`View::append_tail`] appends exactly what the served prompt carried.
//!
//! Every tail item is judged as every arriving item is: a repeat of an act whose whole she holds
//! arrives as its repeat block (invariant 4), and an item whose element her epoch already shows is
//! shown (one element, one occurrence). Its level is the act's: whole, or the page the allocation
//! chose for her newest result (`Level::Page`). Once it is held, her sources' items for the same
//! elements are covered: never shown a second time, and read as she read them in her tail
//! (`View::renders`).

use super::{key, repeat::resolve, Advance, View};
use crate::cause::ElementId;
use crate::epoch::{Block, Observed, SourceId};
use crate::level::Level;
use crate::source::{Item, Source};

/// One tail item as the view judged it for this act: where it goes, its level, what it renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Judged<T> {
    id: SourceId,
    element: ElementId,
    level: Level,
    /// What her prompt shows of it.
    pub text: T,
}

impl<T: Block> Judged<T> {
    /// Her tail with no view to judge it against (none could start, or her view is not placed
    /// yet): every item at `level`, as a view that holds nothing judges it (nothing is held, so
    /// nothing is a repeat or already shown).
    pub fn unheld(tail: &dyn Source<T>, level: impl Fn(&Item<T>) -> Level) -> Vec<Self> {
        tail.items().into_iter().map(|item| {
            let level = level(&item);
            Self::at(tail, item, level)
        }).collect()
    }

    /// `item` of `tail` at `level`: a page is the item's summary (the page the act's allocation cut).
    fn at(tail: &dyn Source<T>, item: Item<T>, level: Level) -> Self {
        let id = key(tail.name(), &item.key);
        let element = item.id();
        let text = match level {
            Level::Page => item.summary.clone().unwrap_or_else(|| crate::source::whole(tail, item.clone())), // JUSTIFIED: a page the source did not render is her whole result: the newest is never cut by a missing page
            _ => crate::source::whole(tail, item),
        };
        Self { id, element, level, text }
    }

    /// The occurrence it shows: which item of her tail it is.
    pub fn element(&self) -> ElementId {
        self.element
    }

    /// The level her view judged it at.
    pub fn level(&self) -> Level {
        self.level
    }
}

impl<T: Block> View<T> {
    /// Judge `tail`'s items that arrive for the first time this epoch, in its order, without
    /// touching her epoch. `history` is the source her earlier acts arrive through (her actions):
    /// its items are what a repeat may point at, never judged here. `level` is each item's level as
    /// the act's allocation decided it (`Level::Page` for a paged result, its page carried as the
    /// item's summary; `Level::Full` otherwise); a repeat overrides it.
    pub fn judge_tail(&self, tail: &dyn Source<T>, history: &dyn Source<T>, level: impl Fn(&Item<T>) -> Level) -> Vec<Judged<T>> {
        let name = tail.name().to_owned();
        let arriving: Vec<Item<T>> = tail.items();
        let arriving_keys: Vec<String> = arriving.iter().map(|item| item.key.clone()).collect();
        let earlier = history.items();
        let earlier_n = earlier.len();
        let mut items = earlier;
        items.extend(arriving);
        // Whole is the element's, whichever source's key it arrived under: an act her tail showed
        // whole is held under the tail's key, never under her actions'.
        let (items, repeats) = resolve(
            tail,
            items,
            |earlier| self.epoch.holds_element_whole(earlier.id()),
            |item| arriving_keys.contains(&item.key) && !self.epoch.holds(&key(&name, &item.key)),
        );
        let mut judged = Vec::new();
        for (item, repeat_of) in items.into_iter().zip(repeats).skip(earlier_n) {
            if self.epoch.holds(&key(&name, &item.key)) || self.epoch.holds_element(item.id()) {
                continue;
            }
            let level = match repeat_of {
                Some(of) => Level::Repeat { of },
                None => level(&item),
            };
            judged.push(Judged::at(tail, item, level));
        }
        judged
    }

    /// Append what the served prompt's tail carried ([`View::judge_tail`]), in order: from here on
    /// it is held, so her next prompt extends this one.
    pub fn append_tail(&mut self, judged: Vec<Judged<T>>) -> Advance {
        let mut advance = Advance::default();
        for Judged { id, element, level, text } in judged {
            if matches!(level, Level::Repeat { .. }) {
                self.repeats.insert(id.clone());
                advance.repeats += 1;
            }
            let chars = text.chars();
            if let Observed::Appended { .. } = self.epoch.observe(id, Some(element), level, text) {
                advance.items += 1;
                advance.chars += chars;
            }
        }
        // The tail is this act's growth too: the next start reserves room for it.
        match self.appended.last_mut() {
            Some(last) => *last += advance.chars,
            None => self.appended.push(advance.chars),
        }
        advance
    }
}
