//! A context source: the results of her acts, her open views, the board, recall.
//!
//! A source renders its current state as ITEMS under stable keys (one result, one open
//! file, one notice), never as one block. The epoch appends only the items whose rendering
//! changed, so a source that grows by one item costs one item, not a re-render of all of
//! them. A source that cannot key its state (the clock, this act's ask) is not a source:
//! it rides the fresh tail after the epoch.

use crate::epoch::Block;

/// How much of a source an epoch carries, granted once at the epoch's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resolution {
    /// Every item in full.
    Full,
    /// Every item as its pointer: what it is and how to reopen it.
    Pointer,
}

/// One item of a source's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<T> {
    /// Stable for the item's life: the same result, file or notice keeps its key.
    pub key: String,
    pub text: T,
}

/// A source of a mind's context.
pub trait Source<T: Block> {
    /// Namespaces the source's item keys in the epoch; unique among her sources.
    fn name(&self) -> &str;

    /// What the allocator weighs this source by at an epoch start; higher keeps detail longer.
    fn priority(&self) -> u8;

    /// The source's current items at `resolution`, in the order they arose. An item that
    /// left the source's state (a result that aged out) is simply absent: what the view
    /// already holds stays until the next turnover.
    fn items(&self, resolution: Resolution) -> Vec<Item<T>>;

    /// Items that must leave the view now (a closed view, a revoked result), each with the
    /// short notice that supersedes it. Their tokens leave at the next turnover; a privacy
    /// revocation that cannot wait is the caller's reason to turn over immediately.
    fn retired(&self) -> Vec<Item<T>> {
        Vec::new()
    }
}
