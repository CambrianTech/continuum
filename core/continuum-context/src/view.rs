//! One mind's view: an epoch fed by her sources.
//!
//! Within an epoch every act only appends: the items whose rendering changed. Turnover is
//! the only rewrite. At a start the caller's one allocator grants each source a character
//! budget between its floor (every item collapsed to its pointer) and its want (every item
//! in full); the view spends it on the source's NEWEST items in full and collapses the
//! older ones. Those choices hold for the epoch, and an item that arises mid-epoch (her
//! next act's result) arrives in full. So collapse happens once, at a start, never as a
//! cut in the middle of an epoch. WHEN to turn over (the window filling, a revocation, a
//! new tool surface) is the caller's policy, read from [`View::chars`] and
//! [`View::appended_per_act`].

use std::collections::HashSet;

use crate::epoch::{Block, Epoch, Observed, SourceId};
use crate::source::{Item, Source};

/// One source as the allocator sees it at an epoch start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub source: String,
    pub priority: u8,
    /// What the source costs with every item collapsed to its pointer: its floor.
    pub pointer_chars: usize,
    /// What the source costs in full: its want.
    pub full_chars: usize,
}

/// The allocator answered for a different number of sources than it was asked about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantMismatch {
    pub claims: usize,
    pub grants: usize,
}

impl std::fmt::Display for GrantMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the allocator granted {} sources' budgets for {} claims", self.grants, self.claims)
    }
}

impl std::error::Error for GrantMismatch {}

/// What one act appended to the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Advance {
    pub items: usize,
    pub chars: usize,
}

/// What an epoch start rendered: the one full re-prefill an epoch costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub epoch: u64,
    pub chars: usize,
    /// Items the grants collapsed to their pointers for this epoch.
    pub collapsed: usize,
}

pub struct View<T: Block> {
    epoch: Epoch<T>,
    /// Items rendered as their pointer for this epoch; every other item renders in full.
    collapsed: HashSet<SourceId>,
    /// Characters each act of this epoch appended, in order.
    appended: Vec<usize>,
}

fn key(source: &str, item: &str) -> SourceId {
    SourceId(format!("{source}/{item}"))
}

/// The items a `grant` of characters keeps in full: the newest run whose upgrade from
/// pointer to full fits what the grant leaves above the floor. The run stops at the first
/// item that does not fit, so what she reads is older items collapsed, newer ones whole.
fn pointer_chars<T: Block>(item: &Item<T>) -> usize {
    item.pointer.as_ref().map_or(0, Block::chars) // map_or: an item that leaves when collapsed costs nothing
}

fn kept_in_full<T: Block>(items: &[Item<T>], grant: usize) -> usize {
    let floor: usize = items.iter().map(pointer_chars).sum();
    let mut spare = grant.saturating_sub(floor);
    let mut kept = 0;
    for item in items.iter().rev() {
        let upgrade = item.full.chars().saturating_sub(pointer_chars(item));
        if upgrade > spare {
            break;
        }
        spare -= upgrade;
        kept += 1;
    }
    kept
}

impl<T: Block> View<T> {
    /// The first epoch. `allocate` answers each claim with a character grant.
    pub fn start(
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<(Self, Start), GrantMismatch> {
        Self::begin(1, head, sources, allocate)
    }

    /// Replace this epoch with a fresh one rendered from the sources' current state at the
    /// grants the allocator makes now.
    pub fn turn_over(
        &mut self,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<Start, GrantMismatch> {
        let (next, start) = Self::begin(self.epoch.id() + 1, head, sources, allocate)?;
        *self = next;
        Ok(start)
    }

    fn begin(
        id: u64,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<(Self, Start), GrantMismatch> {
        let offered: Vec<Vec<Item<T>>> = sources.iter().map(|source| source.items()).collect();
        let claims: Vec<Claim> = sources
            .iter()
            .zip(&offered)
            .map(|(source, items)| Claim {
                source: source.name().to_owned(),
                priority: source.priority(),
                pointer_chars: items.iter().map(pointer_chars).sum(),
                full_chars: items.iter().map(|item| item.full.chars()).sum(),
            })
            .collect();
        let grants = allocate(&claims);
        if grants.len() != claims.len() {
            return Err(GrantMismatch { claims: claims.len(), grants: grants.len() });
        }
        let mut view = Self { epoch: Epoch::new(id, head), collapsed: HashSet::new(), appended: Vec::new() };
        for ((source, items), grant) in sources.iter().zip(offered).zip(grants) {
            let pointers = items.len() - kept_in_full(&items, grant);
            for (index, item) in items.into_iter().enumerate() {
                let id = key(source.name(), &item.key);
                if index < pointers {
                    view.collapsed.insert(id.clone());
                    if let Some(pointer) = item.pointer {
                        view.epoch.observe(id, pointer);
                    }
                } else {
                    view.epoch.observe(id, item.full);
                }
            }
        }
        let start = Start { epoch: id, chars: view.epoch.chars(), collapsed: view.collapsed.len() };
        Ok((view, start))
    }

    /// Append what changed in each source since the last act, each item at the rendering its
    /// epoch start chose. An item first seen mid-epoch arrives in full: the allocator weighs
    /// it at the next start.
    pub fn advance(&mut self, sources: &[&dyn Source<T>]) -> Advance {
        let mut advance = Advance::default();
        let mut count = |observed: Observed, chars: usize| {
            if let Observed::Appended { .. } = observed {
                advance.items += 1;
                advance.chars += chars;
            }
        };
        for source in sources {
            for item in source.items() {
                let id = key(source.name(), &item.key);
                let text = if self.collapsed.contains(&id) { item.pointer } else { Some(item.full) };
                if let Some(text) = text {
                    let chars = text.chars();
                    count(self.epoch.observe(id, text), chars);
                }
            }
            for item in source.retired() {
                let chars = item.full.chars();
                count(self.epoch.retire(key(source.name(), &item.key), item.full), chars);
            }
        }
        self.appended.push(advance.chars);
        advance
    }

    /// The view in order: the head, then every item as it arrived.
    pub fn render(&self) -> impl Iterator<Item = &T> {
        self.epoch.render()
    }

    pub fn epoch_id(&self) -> u64 {
        self.epoch.id()
    }

    /// Characters the whole view costs, superseded versions included.
    pub fn chars(&self) -> usize {
        self.epoch.chars()
    }

    /// Characters held by versions a later version superseded: what the next turnover frees.
    pub fn superseded_chars(&self) -> usize {
        self.epoch.superseded_chars()
    }

    /// The median characters one act of this epoch appended; `None` before its first act.
    /// The caller's growth reserve when it sizes the next start.
    pub fn appended_per_act(&self) -> Option<usize> {
        let mut acts = self.appended.clone();
        acts.sort_unstable();
        acts.get(acts.len() / 2).copied()
    }
}

#[cfg(test)]
mod tests;
