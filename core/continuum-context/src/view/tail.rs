//! Her act's tail joins her view after the act's one allocation and before it is sent (MIND-CONTEXT,
//! "Revision, 2026-11-11: her act's tail is a source"). The tail (her call, its result, the ask, the
//! act's framing and working context) is a [`Source`] for its rendering, keys, element identity and
//! repeat judgement, but never a start source: its claims are the act's own in the one allocation
//! (REQUIRED, NEWEST, LAST_MOVE), so a start grants it nothing, and a turnover starts from her
//! view's sources and then appends this act's tail.
//!
//! Every tail item is judged on arrival as every arriving item is: a repeat of an act whose whole
//! she holds arrives as its repeat block (invariant 4), and an item whose element her epoch already
//! shows is shown (one element, one occurrence). Its level is the act's: whole, or the page the
//! allocation chose for her newest result (`Level::Page`).

use super::{key, repeat::resolve, Advance, View};
use crate::epoch::{Block, Observed};
use crate::level::Level;
use crate::source::{Item, Source};

impl<T: Block> View<T> {
    /// Append `tail`'s items that arrive for the first time this epoch, in its order. `history` is
    /// the source her earlier acts arrive through (her actions): its items are what a repeat may
    /// point at, never appended here. `level` is each tail item's level as the act's allocation
    /// decided it (`Level::Page` for a paged result, its page carried as the item's summary;
    /// `Level::Full` otherwise); a repeat overrides it.
    pub fn append_tail(&mut self, tail: &dyn Source<T>, history: &dyn Source<T>, level: impl Fn(&Item<T>) -> Level) -> Advance {
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
        let mut advance = Advance::default();
        for (item, repeat_of) in items.into_iter().zip(repeats).skip(earlier_n) {
            let id = key(&name, &item.key);
            let element = item.id();
            if self.epoch.holds(&id) || self.epoch.holds_element(element) {
                continue;
            }
            let level = match repeat_of {
                Some(of) => {
                    self.repeats.insert(id.clone());
                    advance.repeats += 1;
                    Level::Repeat { of }
                }
                None => level(&item),
            };
            let text = match level {
                Level::Page => item.summary.clone().unwrap_or_else(|| crate::source::whole(tail, item.clone())), // JUSTIFIED: a page the source did not render is her whole result: the newest is never cut by a missing page
                _ => crate::source::whole(tail, item),
            };
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
