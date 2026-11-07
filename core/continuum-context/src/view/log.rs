//! Her collapsed history as a LOG, and where a start lays her sources down.
//!
//! An item collapses ONCE: the first start that gives it a coarser level (its summary, its
//! pointer, or a fold block over the oldest run) writes that rendering into the log, and every
//! later start lays the log down again byte for byte, appending only what collapses now. Only a
//! source's OLDEST collapsed run is logged (up to its first item kept whole), so a source's items
//! keep their order: her log holds each source's oldest history, her view its newer. A start with
//! room still gives a logged item back: it renders whole at the front of its source's items while
//! its log entry stays where it was (the restore of 2026-10-29, Aiko's fifteen handles). Before
//! this every start re-decided every item from scratch, so a fold block's range, a summary or a
//! pointer could re-render differently and a turnover missed everything after the first of them
//! (the M5, 2026-11-06: 13 turnovers, all resumed from the system head). Epoch N+1 is the head,
//! epoch N's log as it was, the newly collapsed slice, then the recent whole items: successive
//! starts share the prefix through the old log. The log folds (re-decided from scratch, one full
//! re-prefill) only when a source's logged history alone exceeds what the start grants it.
//!
//! Where each unit stands is measured (`Motion::rank`): a source that held stable leads, one
//! that changes rides toward the tail, the log stands by what it keeps.

use std::collections::{HashMap, HashSet};

use super::{claim, key, Claim, Level, SourceId, FOLD_KEY};
use crate::cause::ElementId;
use crate::epoch::Block;
use crate::motion::{Motion, Unit};
use crate::source::{Item, Source};

/// The name her collapsed log goes by in the measured order.
pub(super) const LOG: &str = "(collapsed log)";

/// One collapsed block of her history, as it was first rendered: laid down again byte for byte
/// at every start until the log folds.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Logged<T> {
    pub source: String,
    pub id: SourceId,
    pub element: Option<ElementId>,
    pub level: Level,
    pub text: T,
    /// A fold block's items: they never arrive on their own while the block stands.
    #[serde(default)]
    pub covers: Vec<SourceId>,
}

/// Every item her log already renders (its own entries and what its fold blocks cover).
pub(super) fn logged_ids<T>(log: &[Logged<T>]) -> HashSet<SourceId> {
    log.iter().flat_map(|entry| std::iter::once(entry.id.clone()).chain(entry.covers.iter().cloned())).collect()
}

/// The id of a fold block over the run `first`..`last`: unique, so the log can hold one per start
/// that folds.
pub(super) fn fold_id(source: &str, first: &str, last: &str) -> SourceId {
    key(source, &format!("{FOLD_KEY}{first}..{last}"))
}

/// A start's sources in their measured order, each with its items, its claim (the log's share of it
/// counted as fixed, her logged items' floor being their log entries), and where her log stands.
pub(super) struct Arranged<'a, T> {
    pub sources: Vec<&'a dyn Source<T>>,
    pub items: Vec<Vec<Item<T>>>,
    pub claims: Vec<Claim>,
    pub logged_chars: Vec<usize>,
    /// How many sources stand ahead of the log.
    pub log_at: usize,
    /// For each entry of her log, whether this start charges it: only an entry whose source this
    /// start supplies. A source can be absent from a start (shed for want of its floor, a stream
    /// that ended, a room she left: `fit_view`'s `shed_view_sources`), and its entries are then
    /// dropped with it, never laid down uncharged (Codex on #5805).
    pub retained: Vec<bool>,
}

/// One source as a start takes it: the source, its items, its claim and its logged characters.
type Slot<'a, T> = (&'a dyn Source<T>, Vec<Item<T>>, Claim, usize);

/// Order `sources` and her `log` for a start ([`Motion::rank`]).
pub(super) fn arrange<'a, T: Block>(motion: &Motion, sources: &[&'a dyn Source<T>], log: &[Logged<T>]) -> Arranged<'a, T> {
    let retained: Vec<bool> = log.iter().map(|entry| sources.iter().any(|source| source.name() == entry.source)).collect();
    let charged: Vec<Logged<T>> = log.iter().zip(&retained).filter(|(_, kept)| **kept).map(|(entry, _)| entry.clone()).collect();
    let ids = logged_ids(&charged);
    let mut logged: HashMap<&str, usize> = HashMap::new();
    for entry in &charged {
        *logged.entry(entry.source.as_str()).or_default() += entry.text.chars();
    }
    let items: Vec<Vec<Item<T>>> = sources.iter().map(|source| source.items()).collect();
    let logged_chars: Vec<usize> = sources.iter().map(|source| logged.get(source.name()).copied().unwrap_or(0)).collect(); // unwrap_or: a source with nothing logged
    let claims: Vec<Claim> = sources
        .iter()
        .zip(&items)
        .zip(&logged_chars)
        .map(|((source, items), &fixed)| {
            let fresh: Vec<Item<T>> = items.iter().filter(|item| !ids.contains(&key(source.name(), &item.key))).cloned().collect();
            let mut whole = claim(*source, items, motion.rereads());
            whole.floor_chars = claim(*source, &fresh, motion.rereads()).floor_chars + fixed;
            whole.full_chars += fixed;
            whole.logged_chars = fixed;
            whole.collapsible = crate::depth::collapsible(&fresh, motion.rereads());
            whole
        })
        .collect();
    let log_chars: usize = logged.values().sum();
    let mut units: Vec<Unit<'_>> = sources
        .iter()
        .zip(&claims)
        .zip(&logged_chars)
        .map(|((source, claim), &fixed)| Unit { name: source.name(), chars: claim.full_chars - fixed, log: false, after_log: fixed > 0 })
        .collect();
    units.push(Unit { name: LOG, chars: log_chars, log: true, after_log: false });
    let order = motion.rank(&units);
    let log_at = order.iter().position(|&index| index == sources.len()).unwrap_or(sources.len()); // unwrap_or: rank returns every unit
    let order: Vec<usize> = order.into_iter().filter(|&index| index < sources.len()).collect();
    let mut slots: Vec<Option<Slot<'a, T>>> =
        sources.iter().zip(items).zip(claims).zip(logged_chars).map(|(((s, f), c), l)| Some((*s, f, c, l))).collect();
    let mut arranged = Arranged { sources: Vec::new(), items: Vec::new(), claims: Vec::new(), logged_chars: Vec::new(), log_at, retained };
    for index in order {
        if let Some((source, items, claim, logged)) = slots[index].take() {
            arranged.sources.push(source);
            arranged.items.push(items);
            arranged.claims.push(claim);
            arranged.logged_chars.push(logged);
        }
    }
    arranged
}
