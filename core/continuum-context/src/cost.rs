//! What a block costs, in the unit the caller's grants are made in: the caller's oracle.
//!
//! A start decides what fits a source's grant by pricing every level it could render (whole,
//! summary, pointer, fold block) with the SAME function the caller charges the final prompt with.
//! Before this the view decided in characters, and a caller granting tokens converted each grant
//! to characters at the source's AVERAGE tokens per character; long old turns are cheap per
//! character and short new ones dear, so the average bought characters enough for short items whose
//! own cost overran the grant (Codex, reproduced on Windows: a start rendering 100 tokens against a
//! 32-token grant, `a_view_start_keeps_its_token_grant_with_uneven_message_sizes`). The oracle owns
//! its calibration (a caller's measured density is how its oracle estimates a block); the view never
//! converts a unit.

use crate::epoch::Block;
use crate::source::Item;

/// The caller's price of a block. Additive: a run costs the sum of its blocks, so a caller whose
/// own price of a whole prompt rounds (a ceiling per prompt) charges each block at least its share
/// of that whole, and what the view keeps within a grant is within it charged as one prompt too.
pub trait Cost<T: Block> {
    /// What `block` costs where it is sent.
    fn block(&self, block: &T) -> usize;

    /// What a whole that is stored, not resident, costs: `chars` is its size measured when it was
    /// stored, `resident` what stands in memory for it (its pointer or summary). The body is never
    /// loaded to price it.
    fn stored(&self, resident: &T, chars: usize) -> usize;
}

/// Characters ([`Block::chars`]): the price for a caller whose grants are characters.
#[derive(Clone, Copy, Debug, Default)]
pub struct Chars;

impl<T: Block> Cost<T> for Chars {
    fn block(&self, block: &T) -> usize {
        block.chars()
    }

    fn stored(&self, _resident: &T, chars: usize) -> usize {
        chars
    }
}

/// What `item`'s whole level costs: its stored size when the whole is not resident, else its
/// resident text. The one owner of a whole's price; the allocation never reads a stored body.
pub fn whole<T: Block>(item: &Item<T>, cost: &dyn Cost<T>) -> usize {
    item.stored.as_ref().map_or_else(|| cost.block(&item.full), |stored| cost.stored(&item.full, stored.chars)) // map_or_else: a resident item is priced by its own text
}
