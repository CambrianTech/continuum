//! A context source: the results of her acts, her open views, the board, recall.
//!
//! A source renders its current state as ITEMS under stable keys (one result, one open
//! file, one notice), never as one block. The epoch appends only the items whose rendering
//! changed, so a source that grows by one item costs one item, not a re-render of all of
//! them. A source that cannot key its state (the clock, this act's ask) is not a source:
//! it rides the fresh tail after the epoch.

use crate::epoch::Block;

/// One item of a source's state, in both renderings the view may choose between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<T> {
    /// Stable for the item's life: the same result, file or notice keeps its key.
    pub key: String,
    /// The item itself.
    pub full: T,
    /// A coarser rendering between whole and pointer (a file's outline and the lines she
    /// works in), like a mipmap level: what the view gives an item too large to keep whole
    /// before it gives it only its pointer. `None` when the item has no such level.
    pub summary: Option<T>,
    /// What it is and how to reopen it: what the item costs once collapsed. `None` when the
    /// item leaves the view entirely once collapsed (a conversation turn whose room history
    /// holds it, where a one-line pointer would cost about what the turn does).
    pub pointer: Option<T>,
}

/// A source of a mind's context.
pub trait Source<T: Block> {
    /// Namespaces the source's item keys in the epoch; unique among her sources.
    fn name(&self) -> &str;

    /// What the allocator weighs this source by at an epoch start; higher keeps detail longer.
    fn priority(&self) -> u8;

    /// Whether its items are input she must read (a notice from her substrate, which lines of
    /// a file she has open). The allocator grants a required source its floor before any
    /// optional one, and a window that cannot is a loud under-provision, never a shed.
    fn required(&self) -> bool {
        false
    }

    /// The source's current items, oldest first. An item that left the source's state (a
    /// result that aged out) is simply absent: what the view already holds stays until the
    /// next turnover.
    fn items(&self) -> Vec<Item<T>>;

    /// Items that must leave the view now (a closed view, a revoked result), each as the
    /// short notice that supersedes it (its `full`). Their tokens leave at the next
    /// turnover; a privacy revocation that cannot wait is the caller's reason to turn over
    /// immediately.
    fn retired(&self) -> Vec<Item<T>> {
        Vec::new()
    }
}
