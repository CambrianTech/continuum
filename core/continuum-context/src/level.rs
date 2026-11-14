//! How much of an item the view renders. The view decides it; the epoch records it on each block,
//! so a render receipt says what she was actually shown.

/// Whole, its summary, its pointer, or inside its source's one fold block with the rest of the
/// oldest run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Level {
    Full,
    Summary,
    Pointer,
    Folded,
    /// An unchanged observation she already holds whole in this epoch, shown as its repeat block
    /// ("repeat of #n, outcome unchanged", CONTINUUM-ENGINE section 5 invariant 4). Never her whole: a receipt
    /// reader asking whether a result was shown whole matches `Full` only. `of` is the occurrence
    /// it repeats, the one she was shown whole.
    Repeat { of: crate::cause::ElementId },
    /// Her newest result's first page, cut to the room the act's one allocation left it
    /// (`allocation::newest`) and naming the handle she reads on through: its content from its
    /// start, never a summary. Only her act's tail arrives at it (`View::append_tail`); a start
    /// never allots it.
    Page,
}
