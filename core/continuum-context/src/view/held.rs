//! What her view holds, at full detail: the measure a window is requested by.
//!
//! A start collapses items to fit the grant it was given (their pointer or summary). Measured from
//! the render, her demand is then the grant's echo: the next grant is sized from it, the next start
//! collapses more, and the window ratchets down on an idle pool (Aris on the M5, 2026-11-13, canary
//! 2b17c41df: every grant of 109,745 -> 78,241 -> 68,642 -> 47,890 -> 43,807 was 1.25 x a demand
//! measured from her collapsed view, alone on a 247,040-token pool; she re-read one file six times).
//! The codec knows each item's whole level, so it says what its collapses took away.

use super::{key, View};
use crate::cost::Estimate;
use crate::epoch::Block;
use crate::source::Source;

impl<T: Block> View<T> {
    /// What the items this epoch's start collapsed or summarized would add to the view at their
    /// whole level, priced by `cost`: each item's whole less what the view renders for it now
    /// (nothing, for an item that left on collapse). Added to what the view renders, it is what she
    /// holds at full detail. Folded items (the oldest run, reached by navigation), a shed source's
    /// withheld backlog and repeats (whole elsewhere in this epoch) add nothing: they are not what
    /// her view holds, and counting them inflated a need to 902,158 tokens on a 262,144-token model
    /// (Aiko, the M5, 2026-11-07).
    pub fn collapsed_growth(&self, cost: &dyn Estimate<T>, sources: &[&dyn Source<T>]) -> usize {
        sources
            .iter()
            .flat_map(|source| {
                let name = source.name().to_owned();
                source.items().into_iter().map(move |item| (key(&name, &item.key), item))
            })
            .filter(|(id, _)| (self.collapsed.contains(id) || self.summarized.contains(id)) && !self.folded.contains(id))
            .map(|(id, item)| {
                let rendered = self.epoch.events().iter().rev().find(|event| event.source == id).map_or(0, |event| cost.block(&event.text)); // map_or: an item that left on collapse renders nothing
                crate::cost::whole(&item, cost).saturating_sub(rendered)
            })
            .sum()
    }
}
