//! Which level each item of one source renders at under a start's grant, and what its floor is.
//!
//! The floor of a source is what its items cost at their least: one fold block for the whole
//! run when the source folds them ([`Source::fold`]) and the block costs less than their
//! pointers, else each item's pointer. A floor that is a sum of pointers grows with every act
//! she ever takes (Aris on the M5, 2026-10-29 22:00Z: 97 results in her view, 2 whole, ~95
//! pointers at ~190 chars each, ~4.5k tokens before anything whole fit); one block does not.

use crate::epoch::Block;
use crate::source::{whole_chars, Item, Source};

/// How much of an item the view renders: whole, its summary, its pointer, or inside its
/// source's one fold block with the rest of the oldest run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Level {
    Full,
    Summary,
    Pointer,
    Folded,
}

/// How much of an item a level shows, for comparing two levels of one item.
pub(super) fn rank(level: &Level) -> u8 {
    match level {
        Level::Folded => 0,
        Level::Pointer => 1,
        Level::Summary => 2,
        Level::Full => 3,
    }
}

/// What a start gives one source: each item's level, and the fold block that renders the
/// items at [`Level::Folded`] (the contiguous oldest run), `Some` exactly when one is folded.
pub(super) struct Allotment<T> {
    pub levels: Vec<Level>,
    pub fold: Option<T>,
}

/// The oldest run `run` at its floor, priced by `cost`, given what its pointers cost: the
/// source's fold block when it has one cheaper than those pointers, else the pointers (`None`).
fn run_floor<T: Block>(
    source: &(impl Source<T> + ?Sized),
    run: &[Item<T>],
    pointers: usize,
    cost: &impl Fn(&T) -> usize,
) -> (usize, Option<T>) {
    if run.is_empty() {
        return (0, None);
    }
    match source.fold(run) {
        Some(block) if cost(&block) < pointers => (cost(&block), Some(block)),
        _ => (pointers, None),
    }
}

/// What `items` of `source` cost at their floor, in the unit `cost` prices a block in: the one
/// owner of a source's floor, for the view's own claim and any caller pricing it in tokens.
pub fn floor<T: Block>(
    source: &(impl Source<T> + ?Sized),
    items: &[Item<T>],
    cost: impl Fn(&T) -> usize,
) -> usize {
    let pointers = items
        .iter()
        .map(|item| item.pointer.as_ref().map_or(0, &cost))
        .sum(); // map_or: an item that leaves when collapsed costs nothing
    run_floor(source, items, pointers, &cost).0
}

/// The level a `grant` of characters gives each item. Every item starts at the floor (folded
/// with the rest when its source folds, else its pointer); what the grant leaves above it is
/// spent newest first, each item taking the most detail that still fits, and never more detail
/// than the item after it: shrinking is oldest first (goal 7), so an older item is never kept
/// more whole than a newer one, and the folded items are always the contiguous oldest run.
///
/// The arithmetic: `floor(k)` is what the oldest `k` items cost at their floor. With item `i`
/// still in the run, what it and every older item may cost is `spare + floor(i + 1)`; raising
/// it to a level of `size` leaves the older run at `floor(i)`, so it is affordable when
/// `size + floor(i) <= spare + floor(i + 1)`, and leaves that difference as the new spare.
/// Without a fold `floor(i + 1) - floor(i)` is the item's own pointer, the arithmetic before
/// folding; with one, the run's block is repriced as it shrinks and refunded whole when the
/// last folded item rises. The first item that cannot afford even its pointer stops the walk:
/// it and everything older stay at their floor.
///
/// One item too large to keep whole takes its summary instead of collapsing every older item
/// to a line with it (a 51,827-char file read that ended the run left Aiko nothing of it, so
/// she read it whole 9 times in 43 acts, the M5, 2026-10-25). It still caps the older ones:
/// filling them past it doubled the size of every start (median 14,238 -> 27,696 chars) and
/// the median act went 83-111 s -> 140 s under the same eleven residents (canary 28bf81fa7 vs
/// 3cd34f87c, the M5, 2026-10-25 17:00-18:06Z).
pub(super) fn allot<T: Block>(
    source: &(impl Source<T> + ?Sized),
    items: &[Item<T>],
    grant: usize,
) -> Allotment<T> {
    let cost = |block: &T| block.chars();
    let mut pointers = Vec::with_capacity(items.len() + 1);
    pointers.push(0);
    for item in items {
        let below = pointers[pointers.len() - 1];
        pointers.push(below + item.pointer.as_ref().map_or(0, Block::chars)); // map_or: an item that leaves when collapsed costs nothing
    }
    let floor = |count: usize| run_floor(source, &items[..count], pointers[count], &cost);
    let mut levels = vec![Level::Pointer; items.len()];
    let (mut paid, _) = floor(items.len());
    let mut spare = grant.saturating_sub(paid);
    let mut ceiling = Level::Full;
    let mut run = items.len();
    while run > 0 {
        let item = &items[run - 1];
        let (below, _) = floor(run - 1);
        let budget = spare + paid;
        let afford = |size: usize| size + below <= budget;
        let full = whole_chars(item);
        let summary = item.summary.as_ref().map(Block::chars);
        let rise = if ceiling == Level::Full && afford(full) {
            Some((Level::Full, full))
        } else if ceiling == Level::Summary || ceiling == Level::Full {
            match summary {
                Some(summary) if afford(summary) => Some((Level::Summary, summary)),
                // An item with no smaller level is its own summary: under a summary ceiling a
                // short result stays whole when it fits, and the ceiling holds. Without this a
                // short result older than one big read fell to its pointer and collapsed every
                // older item with it (Cormac on #5426).
                None if ceiling == Level::Summary && afford(full) => Some((Level::Full, full)),
                _ => None,
            }
        } else {
            None
        };
        let pointer = item.pointer.as_ref().map_or(0, Block::chars); // map_or: an item that leaves when collapsed costs nothing
        let Some((level, size)) =
            rise.or_else(|| afford(pointer).then_some((Level::Pointer, pointer)))
        else {
            break;
        };
        // An item with no smaller level holds the summary ceiling it rose under.
        if level != Level::Full || ceiling != Level::Summary {
            ceiling = level;
        }
        levels[run - 1] = level;
        spare = budget - below - size;
        paid = below;
        run -= 1;
    }
    let (_, fold) = floor(run);
    if fold.is_some() {
        levels[..run].fill(Level::Folded);
    }
    Allotment { levels, fold }
}
