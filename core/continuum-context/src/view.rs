//! One mind's view: an epoch fed by her sources.
//!
//! Within an epoch every act only appends: each source's items at the resolution granted
//! at the epoch's start, and only the items whose rendering changed. Turnover is the only
//! rewrite. It asks the caller's one allocator which resolution each source gets, so
//! collapse happens once, at a start, and never as a cut in the middle of an epoch.
//! WHEN to turn over (the window filling, a revocation, a new tool surface) is the
//! caller's policy, read from [`View::chars`] and [`View::appended_per_act`].

use std::collections::HashMap;

use crate::epoch::{Block, Epoch, Observed, SourceId};
use crate::source::{Resolution, Source};

/// One source as the allocator sees it at an epoch start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub source: String,
    pub priority: u8,
    /// What the source costs collapsed to pointers: its floor.
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
    /// Sources the allocator collapsed to pointers for this epoch.
    pub collapsed: Vec<String>,
}

pub struct View<T: Block> {
    epoch: Epoch<T>,
    granted: HashMap<String, Resolution>,
    /// Characters each act of this epoch appended, in order.
    appended: Vec<usize>,
}

fn key(source: &str, item: &str) -> SourceId {
    SourceId(format!("{source}/{item}"))
}

impl<T: Block> View<T> {
    /// The first epoch.
    pub fn start(
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<Resolution>,
    ) -> Result<(Self, Start), GrantMismatch> {
        Self::begin(1, head, sources, allocate)
    }

    /// Replace this epoch with a fresh one rendered from the sources' current state at the
    /// resolutions the allocator grants now.
    pub fn turn_over(
        &mut self,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<Resolution>,
    ) -> Result<Start, GrantMismatch> {
        let (next, start) = Self::begin(self.epoch.id() + 1, head, sources, allocate)?;
        *self = next;
        Ok(start)
    }

    fn begin(
        id: u64,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<Resolution>,
    ) -> Result<(Self, Start), GrantMismatch> {
        let chars = |source: &dyn Source<T>, resolution| -> usize {
            source.items(resolution).iter().map(|item| item.text.chars()).sum()
        };
        let claims: Vec<Claim> = sources
            .iter()
            .map(|source| Claim {
                source: source.name().to_owned(),
                priority: source.priority(),
                pointer_chars: chars(*source, Resolution::Pointer),
                full_chars: chars(*source, Resolution::Full),
            })
            .collect();
        let grants = allocate(&claims);
        if grants.len() != claims.len() {
            return Err(GrantMismatch { claims: claims.len(), grants: grants.len() });
        }
        let mut view = Self { epoch: Epoch::new(id, head), granted: HashMap::new(), appended: Vec::new() };
        for (source, resolution) in sources.iter().zip(grants) {
            view.granted.insert(source.name().to_owned(), resolution);
            for item in source.items(resolution) {
                view.epoch.observe(key(source.name(), &item.key), item.text);
            }
        }
        let start = Start {
            epoch: id,
            chars: view.epoch.chars(),
            collapsed: view
                .granted
                .iter()
                .filter(|(_, resolution)| **resolution == Resolution::Pointer)
                .map(|(source, _)| source.clone())
                .collect(),
        };
        Ok((view, start))
    }

    /// Append what changed in each source since the last act. A source first seen in the
    /// middle of an epoch is carried in full: the allocator weighs it at the next start.
    pub fn advance(&mut self, sources: &[&dyn Source<T>]) -> Advance {
        let mut advance = Advance::default();
        let mut count = |observed: Observed, chars: usize| {
            if let Observed::Appended { .. } = observed {
                advance.items += 1;
                advance.chars += chars;
            }
        };
        for source in sources {
            let resolution = *self.granted.entry(source.name().to_owned()).or_insert(Resolution::Full);
            for item in source.items(resolution) {
                let chars = item.text.chars();
                count(self.epoch.observe(key(source.name(), &item.key), item.text), chars);
            }
            for item in source.retired() {
                let chars = item.text.chars();
                count(self.epoch.retire(key(source.name(), &item.key), item.text), chars);
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
