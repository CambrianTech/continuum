//! A context source: the results of her acts, her open views, the board, recall.
//!
//! A source renders its current state as ITEMS under stable keys (one result, one open
//! file, one notice), never as one block. The epoch appends only the items whose rendering
//! changed, so a source that grows by one item costs one item, not a re-render of all of
//! them. Even the act's tail (the clock, the ask, her call and its result) is a source, keyed by
//! what each item is: judged by the view before the send and joining the epoch once served
//! (`view::View::judge_tail`, `view::View::append_tail`), so it never moves once sent.

use crate::cause::{Act, Cause, Content, ContentHash, ElementId};
use crate::epoch::Block;

/// One item of a source's state, in both renderings the view may choose between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<T> {
    /// Stable for the item's life: the same result, file or notice keeps its key. What the item is
    /// ABOUT; the occurrence is `id`.
    pub(crate) key: String,
    /// This occurrence, named by its producer: two reads of one file share a `key` and have two ids.
    pub(crate) id: ElementId,
    /// What this occurrence follows from (`crate::cause`).
    pub(crate) cause: Cause,
    /// Its whole's hash, taken once where the whole is held or stored.
    pub(crate) content: Content,
    /// The item itself.
    pub(crate) full: T,
    /// A coarser rendering between whole and pointer (a file's outline and the lines she
    /// works in), like a mipmap level: what the view gives an item too large to keep whole
    /// before it gives it only its pointer. `None` when the item has no such level.
    pub(crate) summary: Option<T>,
    /// What it is and how to reopen it: what the item costs once collapsed. `None` when the
    /// item leaves the view entirely once collapsed (a conversation turn whose room history
    /// holds it, where a one-line pointer would cost about what the turn does).
    pub(crate) pointer: Option<T>,
    /// The whole is stored, not resident: `full` then holds what is in memory (its pointer or
    /// summary) and the view prices the whole level from `stored.chars` without reading it.
    /// Only an item the allocation grants WHOLE is loaded, once per render, through the
    /// render's loader. Residency is not visibility: evicting a body from RAM never changes
    /// what she can see (Cormac's proposal, FLOW plan §2d; the RAM storage budget is by bytes,
    /// from governed RAM, never from the window).
    pub(crate) stored: Option<StoredWhole>,
    /// What this occurrence asked for and what its tool declares it does, when it is an act
    /// (CONTINUUM-ENGINE section 5 invariant 4); `None` for an element that is not one (a message, a notice).
    pub(crate) act: Option<Act>,
}

/// Where a non-resident whole lives and what it costs, measured when it was stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredWhole {
    /// The whole's exact size in chars, known at spill time: the allocation's price for it.
    pub chars: usize,
    /// The handle the render's loader opens (a spill path, a result id).
    pub handle: String,
    /// The whole's hash, taken at spill time with its size so it is compared without reading the
    /// body back; `Absent` for a whole spilled before hashes were kept (never a minted hash).
    pub content: Content,
}

/// An item's fields are private to this crate: a source builds one through a constructor, which
/// requires its cause and takes its content hash from bytes, and adds coarser levels with the
/// builders below. There is no way to replace `full` after it is hashed (Cormac on #5611).
impl<T: Block> Item<T> {
    /// An occurrence resident whole, with no coarser levels yet. Its cause is required: a source
    /// cannot build an element without saying what it follows from.
    pub fn whole(id: ElementId, cause: Cause, key: impl Into<String>, full: T) -> Self {
        let content = Content::Hashed(ContentHash::of(full.text()));
        Self { key: key.into(), id, cause, content, full, summary: None, pointer: None, stored: None, act: None }
    }

    /// An occurrence whose `full` is a rendering of a whole held elsewhere (a result shown under
    /// its attribution label): `content` is that whole's hash, taken over its raw bytes where it
    /// is held, so two equal outcomes compare equal whatever their labels say. `Absent` when the
    /// whole is not held.
    pub fn rendered(id: ElementId, cause: Cause, key: impl Into<String>, full: T, content: Content) -> Self {
        Self { key: key.into(), id, cause, content, full, summary: None, pointer: None, stored: None, act: None }
    }

    /// Its pointer level: what it costs once collapsed, and how to reopen it. `None` when it
    /// leaves the view entirely once collapsed.
    pub fn with_pointer(mut self, pointer: impl Into<Option<T>>) -> Self {
        self.pointer = pointer.into();
        self
    }

    /// Its summary level, between whole and pointer.
    pub fn with_summary(mut self, summary: Option<T>) -> Self {
        self.summary = summary;
        self
    }

    /// Its whole is stored, not resident (`StoredWhole`).
    pub fn with_stored(mut self, stored: Option<StoredWhole>) -> Self {
        self.stored = stored;
        self
    }

    /// It is an act: what it asked for and what its tool declares it does.
    pub fn with_act(mut self, act: Act) -> Self {
        self.act = Some(act);
        self
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn id(&self) -> ElementId {
        self.id
    }

    pub fn cause(&self) -> Cause {
        self.cause
    }

    pub fn content(&self) -> Content {
        self.content
    }

    pub fn full(&self) -> &T {
        &self.full
    }

    pub fn summary(&self) -> Option<&T> {
        self.summary.as_ref()
    }

    pub fn pointer(&self) -> Option<&T> {
        self.pointer.as_ref()
    }

    pub fn stored(&self) -> Option<&StoredWhole> {
        self.stored.as_ref()
    }

    pub fn act(&self) -> Option<Act> {
        self.act
    }
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

    /// What it is to her ([`crate::ladder::Consumer`]): context she reads unless she acts through
    /// it (her held card), which a choice never shows below its whole.
    fn consumer(&self) -> crate::ladder::Consumer {
        crate::ladder::Consumer::Context
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

    /// Whether this source's items are STATES, true only while offered (a perception fact: "my
    /// last two acts repeated" stops holding when she stops repeating), rather than events that
    /// stay what happened (a turn, a result). The view ends a state the source no longer offers
    /// with its [`Source::ended`] notice. `false` (the default): an item that left the source
    /// stays until the next start, as [`Source::items`] says.
    fn states(&self) -> bool {
        false
    }

    /// For a source of [`states`](Source::states): the notice that supersedes its item `key`,
    /// which her view holds and the source no longer offers. The view appends it once, so the
    /// item's last word in her view is that it ended, and the same state returning later appends
    /// anew at the newest end instead of reading as unchanged where it first fired. `None` keeps
    /// that item (an event among the states) until the next start.
    fn ended(&self, _key: &str) -> Option<T> {
        None
    }

    /// Load the whole of one of this source's stored items (its `stored` handle), called only
    /// for an item the allocation granted WHOLE. The default stores nothing; a source that
    /// spills bodies (her results) overrides it. `None` renders what is resident.
    fn load(&self, _item: &Item<T>) -> Option<T> {
        None
    }

    /// One block standing for `folded`, the contiguous oldest run of this source's items at a
    /// start: the range it covers and how to reopen one, its length independent of the run's
    /// (never a list of ids). It reads only the run's ends, O(1) in the run: a start asks once
    /// per item it raises. The view folds the run only when this block is smaller than the
    /// run's pointers, so a source's floor stops growing with every item it ever held. Fixed
    /// for the epoch it starts. REQUIRED, never defaulted: a source that does not decide floors
    /// every item it ever held at its pointer, and a growing one then outgrows any window (her
    /// thoughts had no fold: Benchy's floor was 2,536 thought pointers, 78,755 tokens, and every
    /// act was refused, the M5, 2026-11-09). `None` is a decision a source states and justifies.
    fn fold(&self, folded: &[Item<T>]) -> Option<T>;

    /// The block `item` renders as when the view finds it repeats `ancestor` (CONTINUUM-ENGINE section 5
    /// invariant 4: the same request to a tool that only observes, with the same outcome). The
    /// view decides; the source words it, naming the ancestor and how to reopen it even when the
    /// ancestor is folded or paged out, never "as above". The default words none, so its items
    /// always render as themselves.
    fn repeat(&self, _item: &Item<T>, _ancestor: &Item<T>) -> Option<T> {
        None
    }
}

/// The item at its whole level: a stored body loaded through its source, else what is resident.
pub fn whole<T: Block>(source: &(impl Source<T> + ?Sized), item: Item<T>) -> T {
    let loaded = item.stored.as_ref().and_then(|_| source.load(&item));
    loaded.unwrap_or(item.full) // unwrap_or: an unloadable body renders its resident level (pointer or summary), never nothing
}
