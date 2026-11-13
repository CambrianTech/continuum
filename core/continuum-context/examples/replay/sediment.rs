//! Generational view (`--generational`, card f746143e): make a turnover cheap by where its first
//! changed byte lands, since KV shift is infeasible on our hybrid models.
//!
//! Two generations follow the head. The SEDIMENT holds her oldest items already collapsed to
//! their handles (pointer lines), frozen right after the head: a turnover never re-renders it.
//! The YOUNG generation is the view as the codec runs it today (`View`, its starts, admission).
//! A turnover settles the young generation's oldest items into the sediment, appended to it as
//! one more block, so the engine keeps everything up to the young boundary and re-prefills only
//! from there. The codec's rules hold: shrinking is oldest first; an item in the sediment is a
//! pointer, the least detail, and every item in it is older than every young item of its source,
//! so no item is more detailed than a newer one; her newest stays whole (it is young).
//!
//! How much settles is the start's own allocation, no constant of its own: each growing source
//! keeps, newest first, the items whose whole text fits its grant under `start_grants`
//! (`Rule::Reserved`), and its older items settle. The sediment's share is what that start leaves
//! the growing sources, divided by the same growing share (`--share`); only a sediment past it
//! re-renders, the one full re-prefill from the head: it keeps the items her sources still offer,
//! newest first, within its share less the last batch it took (room for one more turnover, as
//! `start_grants` leaves room for one more act), and drops the rest.

use std::collections::{HashMap, HashSet};

use continuum_context::source::Source;
use continuum_context::view::Claim;

use super::Captured;

const HEADER: &str = "[older items, collapsed to their handles; expand any by its handle]";

/// A source whose items are her history in time: what settles.
fn settles(source: &str, key: &str) -> bool {
    matches!(source, "conversation" | "actions" | "working-memory") && key != "banner" && !key.ends_with("/header")
}

#[derive(Default)]
pub struct Sediment {
    blocks: Vec<String>,
    /// Every settled item, oldest first, with its pointer line.
    settled: Vec<((String, String), String)>,
    keys: HashSet<(String, String)>,
    last_batch: usize,
    /// Full re-renders: the sediment's own turnovers.
    pub rerenders: usize,
    /// Items a re-render dropped because it could not hold their handles.
    pub dropped: usize,
}

impl Sediment {
    /// A new head: nothing after it is reusable, the sediment included. Its counts carry on.
    pub fn restart(&mut self) {
        *self = Sediment { rerenders: self.rerenders, dropped: self.dropped, ..Sediment::default() };
    }

    /// The head blocks of the young epoch: the head, then the sediment.
    pub fn heads(&self, head: &str) -> Vec<String> {
        std::iter::once(head.to_owned()).chain(self.blocks.iter().cloned()).collect()
    }

    pub fn chars(&self) -> usize {
        self.blocks.iter().map(|block| block.chars().count()).sum()
    }

    /// Remove the settled items from the young sources.
    pub fn filter(&self, sources: &mut [Captured]) {
        if self.keys.is_empty() {
            return;
        }
        for source in sources {
            let name = source.name.clone();
            source.items.retain(|item| !self.keys.contains(&(name.clone(), item.key().to_owned())));
        }
    }

    /// Settle each growing source's oldest items that its `grants` (one per source, in order)
    /// cannot keep whole. Returns how many items settled.
    pub fn settle(&mut self, sources: &mut [Captured], grants: &[usize]) -> usize {
        let mut batch = Vec::new();
        for (source, grant) in sources.iter_mut().zip(grants) {
            let standing: usize = source.items.iter().filter(|item| !settles(&source.name, item.key())).map(|item| item.full().chars().count()).sum();
            let mut room = grant.saturating_sub(standing);
            // Newest first: the items that still fit whole stay young; from the first that does
            // not, every older item settles (a contiguous oldest run). Her newest item of each
            // source stays young whatever its grant: the newest is never collapsed.
            let mut boundary = 0;
            let mut newest = true;
            for (index, item) in source.items.iter().enumerate().rev().filter(|(_, item)| settles(&source.name, item.key())) {
                let chars = item.full().chars().count();
                if std::mem::take(&mut newest) {
                    room = room.saturating_sub(chars);
                    continue;
                }
                if chars > room {
                    boundary = index + 1;
                    break;
                }
                room -= chars;
            }
            let name = source.name.clone();
            let mut index = 0;
            source.items.retain(|item| {
                let settle = index < boundary && settles(&name, item.key());
                index += 1;
                if settle {
                    let key = (name.clone(), item.key().to_owned());
                    let pointer = item.pointer().cloned().unwrap_or_else(|| item.full().clone()); // unwrap_or_else: an item without a pointer is its own handle
                    self.keys.insert(key.clone());
                    batch.push((key, pointer));
                }
                !settle
            });
        }
        if batch.is_empty() {
            return 0;
        }
        let mut block: Vec<&str> = if self.blocks.is_empty() { vec![HEADER] } else { Vec::new() };
        block.extend(batch.iter().map(|(_, pointer)| pointer.as_str()));
        let block = block.join("\n");
        self.last_batch = block.chars().count();
        self.blocks.push(block);
        let settled = batch.len();
        self.settled.extend(batch);
        settled
    }

    /// Whether the sediment outgrew `share` characters.
    pub fn overgrown(&self, share: usize) -> bool {
        self.chars() > share
    }

    /// The sediment's own turnover: one block of the handles her sources still `offered`, newest
    /// first within `share` less the last batch.
    pub fn rerender(&mut self, offered: &HashSet<(String, String)>, share: usize) {
        let budget = share.saturating_sub(self.last_batch);
        let mut kept = Vec::new();
        let mut chars = HEADER.chars().count();
        for (key, pointer) in self.settled.iter().rev() {
            let cost = pointer.chars().count() + 1;
            if offered.contains(key) && chars + cost <= budget {
                chars += cost;
                kept.push((key.clone(), pointer.clone()));
            } else {
                self.dropped += 1;
            }
        }
        kept.reverse();
        let block: Vec<&str> = std::iter::once(HEADER).chain(kept.iter().map(|(_, pointer)| pointer.as_str())).collect();
        self.blocks = vec![block.join("\n")];
        self.settled = kept;
        self.rerenders += 1;
    }
}

/// Every (source, key) the sources offer this act, before the sediment filters them.
pub fn offered(sources: &[Captured]) -> HashSet<(String, String)> {
    sources.iter().flat_map(|source| source.items.iter().map(|item| (source.name.clone(), item.key().to_owned()))).collect()
}

/// The grants each source gets under `rule`'s start, mapped back to `sources` by name: the
/// view's own `claims` order.
pub fn grants_by_source(sources: &[Captured], claims: &[Claim], grants: &[usize]) -> Vec<usize> {
    let by_name: HashMap<&str, usize> = claims.iter().zip(grants).map(|(claim, grant)| (claim.source.as_str(), *grant)).collect();
    sources.iter().map(|source| by_name.get(source.name()).copied().unwrap_or(0)).collect() // unwrap_or: a source with no claim gets no grant
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, sizes: &[usize]) -> Captured {
        let items = sizes
            .iter()
            .enumerate()
            .map(|(n, size)| {
                crate::captured(n.to_string(), "x".repeat(*size)).with_pointer(format!("[result #{n}]"))
            })
            .collect();
        Captured { name: name.to_owned(), items }
    }

    // what this catches: a turnover that settles a NEWER item before an older one (an item more
    // detailed than a newer one), settles her newest, or settles a source that is not history.
    #[test]
    fn a_turnover_settles_the_oldest_run_and_keeps_the_newest_young() {
        let mut sources = vec![source("actions", &[100, 100, 10, 100, 50]), source("room-doctrine", &[500])];
        let mut sediment = Sediment::default();
        assert_eq!(sediment.settle(&mut sources, &[150, 0]), 3);
        let young: Vec<&str> = sources[0].items.iter().map(|item| item.key()).collect();
        assert_eq!(young, ["3", "4"], "from the first item that does not fit, the whole oldest run settles");
        assert_eq!(sources[1].items.len(), 1, "a standing block never settles");
        assert!(sediment.heads("head")[1].starts_with(HEADER) && sediment.heads("head")[1].ends_with("[result #2]"));
        let mut again = vec![source("actions", &[100, 100, 10, 100, 50, 30])];
        sediment.filter(&mut again);
        assert_eq!(again[0].items.len(), 3, "settled items do not come back young");
        assert_eq!(sediment.settle(&mut again, &[0]), 2, "her newest stays young even with no grant");
        assert_eq!(sediment.heads("head").len(), 3, "a turnover appends to the sediment, never rewrites it");
    }

    // what this catches: a re-render keeping handles her sources no longer offer, or keeping the
    // oldest handles over the newest.
    #[test]
    fn a_rerender_keeps_the_newest_offered_handles_within_its_share() {
        let mut sources = vec![source("actions", &[100; 6])];
        let mut sediment = Sediment::default();
        sediment.settle(&mut sources, &[0]);
        let offered: HashSet<(String, String)> = ["1", "3", "4"].iter().map(|key| ("actions".to_owned(), key.to_string())).collect();
        sediment.last_batch = 0;
        sediment.rerender(&offered, HEADER.len() + 2 * "[result #5]\n".len());
        assert_eq!(sediment.heads("h")[1], format!("{HEADER}\n[result #3]\n[result #4]"));
        assert_eq!((sediment.rerenders, sediment.dropped), (1, 3));
    }
}
