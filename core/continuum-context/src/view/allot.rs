//! Which level each item of one source renders at under a start's grant, and what its floor is.
//!
//! The floor of a source is what its items cost at their least: one fold block for the whole
//! run when the source folds them ([`Source::fold`]) and the block costs less than their
//! pointers, else each item's pointer. A floor that is a sum of pointers grows with every act
//! she ever takes (Aris on the M5, 2026-10-29 22:00Z: 97 results in her view, 2 whole, ~95
//! pointers at ~190 chars each, ~4.5k tokens before anything whole fit); one block does not.

use crate::epoch::Block;
use crate::cost::Cost;
use crate::source::{Item, Source};

pub(super) use crate::level::Level;

/// How much of an item a level shows, for comparing two levels of one item.
pub(super) fn rank(level: &Level) -> u8 {
    match level {
        Level::Folded => 0,
        Level::Pointer => 1,
        Level::Summary => 2,
        Level::Full | Level::Repeat { .. } => 3,
    }
}

/// What a start gives one source: each item's level, and the fold block that renders the
/// items at [`Level::Folded`] (the contiguous oldest run), `Some` exactly when one is folded.
pub(super) struct Allotment<T> {
    pub levels: Vec<Level>,
    pub fold: Option<T>,
}

/// The fold block standing for the oldest run `run` when its source folds it and the block
/// costs less than the run's pointers (`pointers`), else `None`: the run floors at its pointers.
/// Decided in the caller's cost, the one the view allots by, so the view and the caller's claim
/// ([`floor`]) price the same rendering. `Source::fold` reads only the run's ends, so asking it
/// once per item a start raises is O(1) each.
fn fold_of<T: Block>(
    source: &(impl Source<T> + ?Sized),
    run: &[Item<T>],
    pointers: usize,
    cost: &dyn Cost<T>,
) -> Option<T> {
    if run.is_empty() {
        return None;
    }
    source
        .fold(run)
        .filter(|block| cost.block(block) < pointers)
}

/// How many of `items`' leading items stand as themselves: an item with no summary, nothing
/// stored and a pointer as long as its whole has nothing to give up, so folding it saves nothing
/// and only moves the front of her view (her `[actions]` banner folded into the block broke the
/// prefix a turnover keeps, `consecutive_acts_share_the_whole_view_and_a_turnover_keeps_the_settled_front`).
/// They render as themselves at the floor; the fold run is the oldest items after them.
fn standing_head<T: Block>(items: &[Item<T>]) -> usize {
    items
        .iter()
        .take_while(|item| {
            item.summary.is_none()
                && item.stored.is_none()
                && item
                    .pointer
                    .as_ref()
                    .is_some_and(|pointer| pointer.chars() == item.full.chars())
        })
        .count()
}

/// What `items` of `source` cost at their floor, priced by the caller's `cost`: the one owner of a
/// source's floor, for the view's own claim and any caller pricing it for its allocation.
/// Whether the floor is the fold block or the pointers is [`fold_of`]'s decision, the view's.
pub fn floor<T: Block>(cost: &dyn Cost<T>, source: &(impl Source<T> + ?Sized), items: &[Item<T>]) -> usize {
    let head = standing_head(items);
    let pointers = |run: &[Item<T>]| -> usize {
        run.iter()
            .map(|item| item.pointer.as_ref().map_or(0, |pointer| cost.block(pointer)))
            .sum() // map_or: an item that leaves when collapsed costs nothing
    };
    let (standing, rest) = items.split_at(head);
    let rest_pointers = pointers(rest);
    pointers(standing)
        + match fold_of(source, rest, rest_pointers, cost) {
            Some(block) => cost.block(&block),
            None => rest_pointers,
        }
}

/// The level a `grant` gives each item, every level priced by the caller's `cost` (the oracle its
/// grants are made in, never an average converting them). Every item starts at the floor (folded
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
    cost: &dyn Cost<T>,
) -> Allotment<T> {
    let mut pointers = Vec::with_capacity(items.len() + 1);
    pointers.push(0);
    for item in items {
        let below = pointers[pointers.len() - 1];
        pointers.push(below + item.pointer.as_ref().map_or(0, |pointer| cost.block(pointer))); // map_or: an item that leaves when collapsed costs nothing
    }
    let head = standing_head(items);
    let floor = |count: usize| {
        if count <= head {
            return (pointers[count], None);
        }
        match fold_of(source, &items[head..count], pointers[count] - pointers[head], cost) {
            Some(block) => (pointers[head] + cost.block(&block), Some(block)),
            None => (pointers[count], None),
        }
    };
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
        let full = crate::cost::whole(item, cost);
        let summary = item.summary.as_ref().map(|summary| cost.block(summary));
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
        let pointer = item.pointer.as_ref().map_or(0, |pointer| cost.block(pointer)); // map_or: an item that leaves when collapsed costs nothing
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
        levels[head..run].fill(Level::Folded);
    }
    Allotment { levels, fold }
}
