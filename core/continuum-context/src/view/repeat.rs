//! MIND-CONTEXT invariant 4: a cycle cannot be presented as progress.
//!
//! An act that asks a tool which only observes for exactly what an earlier act in its chain asked
//! (equal [`Fingerprint`](crate::cause::Fingerprint)), and gets the same outcome (equal content
//! hash), arrives as its source's repeat block ("repeat of #n, outcome unchanged") instead of a
//! second copy of a whole she already has. It keeps its own id, order and cause, so nothing is
//! erased. The view decides here, once, for every source; the source only words it
//! ([`Source::repeat`]). This is perception, never a gate or an instruction.
//!
//! Only where the earlier whole is actually in front of her (Codex and Fable on the design): a
//! repeat is chosen for an act's first arrival in an epoch, and only when the occurrence it
//! repeats is held WHOLE in that epoch, so it was in the context she was sent. An ancestor that is
//! folded, collapsed, summarized or gone with a turnover gives no repeat: she needs the content
//! again and it arrives whole. A turnover re-renders every item as itself, and a reopen is never a
//! repeat ([`ToolEffect::Reopens`]).

use std::collections::HashMap;

use crate::cause::{Cause, Content, ContentHash, ElementId, Fingerprint, ToolEffect};
use crate::epoch::Block;
use crate::source::{Item, Source};

/// `items` with each repeat replaced by its source's repeat block, and for each item the key of
/// the occurrence it repeats (`None` when it is not one). `whole` answers whether an earlier item
/// is held whole in this epoch; `arriving` whether an item is arriving for the first time (an
/// item already rendered keeps its rendering for the epoch). The block is the item's whole and its
/// pointer, with no summary and nothing stored: it costs what it says at every level.
pub(super) fn resolve<T: Block>(
    source: &(impl Source<T> + ?Sized),
    items: Vec<Item<T>>,
    whole: impl Fn(&Item<T>) -> bool,
    arriving: impl Fn(&Item<T>) -> bool,
) -> (Vec<Item<T>>, Vec<Option<String>>) {
    let ancestors = ancestors(&items, &whole, &arriving);
    let blocks: Vec<Option<T>> = items
        .iter()
        .zip(&ancestors)
        .map(|(item, ancestor)| ancestor.and_then(|at| source.repeat(item, &items[at])))
        .collect();
    let keys: Vec<Option<String>> = ancestors
        .iter()
        .zip(&blocks)
        .map(|(ancestor, block)| block.as_ref().and(*ancestor).map(|at| items[at].key.clone()))
        .collect();
    let repeated = items
        .into_iter()
        .zip(blocks)
        .map(|(mut item, block)| {
            if let Some(block) = block {
                item.pointer = Some(block.clone());
                item.full = block;
                item.summary = None;
                item.stored = None;
            }
            item
        })
        .collect();
    (repeated, keys)
}

/// For each arriving act, the position of the occurrence it repeats: walking its `Cause::Of` chain
/// back through earlier items of this source, the nearest act with its fingerprint and content
/// hash whose whole is held. An ancestor that matches but is not whole (folded, collapsed, itself
/// a repeat) is passed over for an older one. The walk only moves to earlier positions, so a
/// malformed chain cannot loop.
fn ancestors<T>(
    items: &[Item<T>],
    whole: &impl Fn(&Item<T>) -> bool,
    arriving: &impl Fn(&Item<T>) -> bool,
) -> Vec<Option<usize>> {
    let at: HashMap<ElementId, usize> =
        items.iter().enumerate().map(|(position, item)| (item.id, position)).collect();
    items
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let identity = observed(item).filter(|_| arriving(item))?;
            let mut cause = item.cause;
            while let Cause::Of(parent) = cause {
                let earlier = at.get(&parent).copied().filter(|&earlier| earlier < position)?;
                let ancestor = &items[earlier];
                if observed(ancestor) == Some(identity) && whole(ancestor) {
                    return Some(earlier);
                }
                cause = ancestor.cause;
            }
            None
        })
        .collect()
}

/// An item's repeat identity, only for an act whose tool declares it observes and whose content is
/// hashed. Anything else is never a repeat and never repeated.
fn observed<T>(item: &Item<T>) -> Option<(Fingerprint, ContentHash)> {
    let act = item.act?;
    match (act.effect, item.content) {
        (ToolEffect::Observes, Content::Hashed(hash)) => Some((act.fingerprint, hash)),
        _ => None,
    }
}
