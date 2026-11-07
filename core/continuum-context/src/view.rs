//! One mind's view: an epoch fed by her sources.
//!
//! Within an epoch every act only appends: the items whose rendering changed. Turnover is
//! the only rewrite. At a start the caller's one allocator grants each source a character
//! budget between its floor (its items folded into one block when the source folds, else
//! each collapsed to its pointer) and its want (every item in full); the view spends it newest
//! first, each item at the most detail that fits: whole, its summary (a coarser level, like a
//! mipmap), its pointer, or folded with the oldest run. Those choices hold for the
//! epoch, and an item that arises mid-epoch (her next act's result) arrives in full unless
//! the act's admission ([`View::readmit`]) gives it a coarser level. So collapse happens once, at a start, never as a
//! cut in the middle of an epoch. WHEN to turn over (the window filling, a revocation, a
//! new tool surface) is the caller's policy, read from [`View::chars`] and
//! [`View::appended_per_act`].

use std::collections::HashSet;

use crate::epoch::{Block, Epoch, Mark, Observed, SourceId};
use crate::motion::{fingerprint, Motion};
use crate::source::{Item, Source};

mod allot;
mod repeat;

use allot::{allot, rank};
pub use crate::level::Level;
pub use allot::floor;

/// The item key a source's fold block is observed under: no source keys an item with it.
const FOLD_KEY: &str = "(folded)";

/// One source as the allocator sees it at an epoch start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub source: String,
    pub priority: u8,
    /// What the source costs at its least ([`floor`]): one fold block when the source folds
    /// its items, else every item collapsed to its pointer.
    pub floor_chars: usize,
    /// What the source costs in full: its want. A folded item still wants its whole.
    pub full_chars: usize,
    /// Its newest item in full: the newest end a start never collapses when it fits.
    pub newest_chars: usize,
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
    /// Arrivals rendered as a repeat of an occurrence held whole (MIND-CONTEXT invariant 4): the
    /// health line's repeats.
    pub repeats: usize,
}

/// What an epoch start rendered: the one full re-prefill an epoch costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub epoch: u64,
    pub chars: usize,
    /// Items the grants collapsed to their pointers for this epoch.
    pub collapsed: usize,
    /// Items the grants rendered at their summary for this epoch.
    pub summarized: usize,
    /// Items the grants folded into their source's one block for this epoch.
    pub folded: usize,
    /// Previously held items promoted above their old level to summary or full.
    pub restored_items: usize,
    /// Rendered characters in those promoted items, not a measure of useful learning.
    pub restored_chars: usize,
}

/// One block she was shown, as the view rendered it (`View::receipt`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rendered {
    pub source: SourceId,
    /// The occurrence it renders; `None` for a fold block, which stands for a range.
    pub element: Option<crate::cause::ElementId>,
    pub level: Level,
    pub version: u32,
    /// A superseding notice for a source that ended.
    pub retired: bool,
    pub chars: usize,
}

/// What one submitted prompt showed her, in prompt order (the view's half of the render receipt).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Receipt {
    pub epoch: u64,
    pub items: Vec<Rendered>,
}

/// Serializable, so a mind's view survives her restart with the rest of her volatile state:
/// restored, it renders the same bytes, and the engine's saved page for it still matches.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct View<T: Block> {
    epoch: Epoch<T>,
    /// Items rendered as their pointer for this epoch; every other item renders in full.
    collapsed: HashSet<SourceId>,
    /// Items rendered as their summary for this epoch. Absent in a view saved before levels.
    #[serde(default)]
    summarized: HashSet<SourceId>,
    /// Items inside their source's fold block for this epoch: they never arrive on their own
    /// until a start raises them. Absent in a view saved before folding.
    #[serde(default)]
    folded: HashSet<SourceId>,
    /// Items rendered as a repeat of an occurrence held whole this epoch. A repeat shows its
    /// content only by reference, so it is never a read of a whole ([`View::renders`]). Absent in
    /// a view saved before repeats, which held none.
    #[serde(default)]
    repeats: HashSet<SourceId>,
    /// Items a sent prompt's tail already showed her this epoch ([`View::append_sent`]): the tail
    /// is in the view in the bytes it was sent, so the item never arrives again as its source
    /// renders it until a start re-renders everything. Absent in a view saved before the tail
    /// joined the view, which covered nothing.
    #[serde(default)]
    covered: HashSet<SourceId>,
    /// Characters each act of this epoch appended, in order.
    appended: Vec<usize>,
    /// How each source moved across this view's starts; carried over every turnover and
    /// restart, it orders the next start. Absent in a view saved before it existed.
    #[serde(default)]
    motion: Motion,
    /// Where this act's appends began, until its prompt is sent: what [`View::readmit`]
    /// redoes. Never saved: a restored view has no unsent act.
    #[serde(skip)]
    act: Option<Mark>,
    /// What the last sent prompt showed her, until the request that carried it is served:
    /// taken there ([`View::take_submitted`]), so a prompt the engine never accepted is never
    /// receipted. Never saved: a restored view has nothing in flight.
    #[serde(skip)]
    submitted: Option<Receipt>,
    /// What the last append added, per source, in order: which source appended how many items and
    /// characters, so a re-append of a whole history names its source (card ef0b04e4: 2,195 items,
    /// 57.9M chars on one advance after a restart). Never saved: it describes one act.
    #[serde(skip)]
    last_by_source: Vec<SourceAdvance>,
}

/// One source's share of an append.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceAdvance {
    pub source: String,
    pub items: usize,
    pub chars: usize,
}

fn key(source: &str, item: &str) -> SourceId {
    SourceId(format!("{source}/{item}"))
}

fn claim<T: Block>(source: &dyn Source<T>, items: &[Item<T>]) -> Claim {
    Claim {
        source: source.name().to_owned(),
        priority: source.priority(),
        floor_chars: floor(source, items, Block::chars),
        full_chars: items.iter().map(crate::source::whole_chars).sum(),
        newest_chars: items.last().map_or(0, crate::source::whole_chars), // map_or: a source with no items has no newest end
    }
}

/// What each source would claim at an epoch start, without starting one: the caller's one
/// allocation weighs these beside its own claims and hands its grants back to
/// [`View::start`] / [`View::turn_over`].
pub fn claims<T: Block>(sources: &[&dyn Source<T>]) -> Vec<Claim> {
    sources.iter().map(|source| claim(*source, &source.items())).collect()
}

impl<T: Block> View<T> {
    /// The first epoch. `allocate` answers each claim with a character grant.
    pub fn start(
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<(Self, Start), GrantMismatch> {
        Self::begin(1, head, sources, allocate, Motion::default(), None).map_err(|(mismatch, _)| mismatch)
    }

    /// Replace this epoch with a fresh one rendered from the sources' current state at the
    /// grants the allocator makes now.
    pub fn turn_over(
        &mut self,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<Start, GrantMismatch> {
        let motion = std::mem::take(&mut self.motion);
        let (next, mut start) = match Self::begin(self.epoch.id() + 1, head, sources, allocate, motion, Some(&self.epoch)) {
            Ok(begun) => begun,
            Err((mismatch, motion)) => {
                self.motion = motion;
                return Err(mismatch);
            }
        };
        for event in next.epoch.events() {
            if !event.retired && event.element.is_some() && self.restores(&event.source, &event.level) {
                start.restored_items += 1;
                start.restored_chars += event.text.chars();
            }
        }
        *self = next;
        Ok(start)
    }

    /// Whether a start under the grants `allocate` answers would show her more than this epoch
    /// holds: an item it already holds that the start would render at a higher level (whole, or
    /// a summary of what it holds as a pointer). An epoch only appends, so after a squeezed start
    /// her items stayed collapsed while the window had room for them (Aiko, the M5, 2026-10-29:
    /// her last fifteen results as handles in a 42k-char prompt on a 55k-token window). This is
    /// the turnover that gives them back.
    /// Only a rise to a summary or whole earns it: a folded item that would rise only to its
    /// pointer is not a restore, or every act that could afford one more pointer would turn the
    /// view over and lose the prefix for a line the fold block already covers (Cormac).
    /// An allocator that answers for a different number of sources is the same refusal a start
    /// gives ([`GrantMismatch`]), never a quiet "nothing to restore" (Codex on #5564).
    pub fn start_would_restore(&self, sources: &[&dyn Source<T>], allocate: impl FnOnce(&[Claim]) -> Vec<usize>) -> Result<bool, GrantMismatch> {
        let sources = self.motion.order(sources);
        let offered: Vec<Vec<Item<T>>> = sources.iter().map(|source| source.items()).collect();
        let claims: Vec<Claim> = sources.iter().zip(&offered).map(|(source, items)| claim(*source, items)).collect();
        let grants = allocate(&claims);
        if grants.len() != claims.len() {
            return Err(GrantMismatch { claims: claims.len(), grants: grants.len() });
        }
        Ok(sources.iter().zip(&offered).zip(&grants).any(|((source, items), grant)| {
            allot(&**source, items, *grant).levels.iter().zip(items).any(|(level, item)| {
                let id = key(source.name(), &item.key);
                self.restores(&id, level)
            })
        }))
    }

    fn restores(&self, id: &SourceId, level: &Level) -> bool {
        (self.epoch.holds(id) || self.folded.contains(id))
            && rank(level) > rank(&self.held_level(id))
            && rank(level) > rank(&Level::Pointer)
    }

    fn held_level(&self, id: &SourceId) -> Level {
        if self.folded.contains(id) {
            Level::Folded
        } else if self.collapsed.contains(id) {
            Level::Pointer
        } else if self.summarized.contains(id) {
            Level::Summary
        } else {
            Level::Full
        }
    }

    /// Render a start with `sources` in the order `motion` measured as most stable first, and
    /// record what each rendered. The blocks are laid down so the start shares the longest prefix
    /// with the epoch it replaces (`previous`): [`lead_with_what_held`]. A refusal hands `motion`
    /// back.
    fn begin(
        id: u64,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
        mut motion: Motion,
        previous: Option<&Epoch<T>>,
    ) -> Result<(Self, Start), (GrantMismatch, Motion)> {
        let sources = motion.order(sources);
        let offered: Vec<Vec<Item<T>>> = sources.iter().map(|source| source.items()).collect();
        let claims: Vec<Claim> = sources.iter().zip(&offered).map(|(source, items)| claim(*source, items)).collect();
        let grants = allocate(&claims);
        if grants.len() != claims.len() {
            return Err((GrantMismatch { claims: claims.len(), grants: grants.len() }, motion));
        }
        let mut view = Self {
            epoch: Epoch::new(id, head),
            collapsed: HashSet::new(),
            summarized: HashSet::new(),
            folded: HashSet::new(),
            repeats: HashSet::new(),
            covered: HashSet::new(),
            appended: Vec::new(),
            motion: Motion::default(),
            act: None,
            submitted: None,
            last_by_source: Vec::new(),
        };
        let mut planned: Vec<Planned<T>> = Vec::new();
        for (index, ((source, items), grant)) in sources.iter().zip(offered).zip(grants).enumerate() {
            let allot::Allotment { levels, mut fold } = allot(&**source, &items, grant);
            let mut rendered = Vec::with_capacity(items.len());
            for (item, level) in items.into_iter().zip(levels) {
                let id = key(source.name(), &item.key);
                let element = item.id();
                let item_key = item.key.clone(); // its fingerprint after the level consumed it; once per item per start
                let text = match level {
                    // A start re-renders every item as itself: no repeat is allotted here.
                    Level::Full | Level::Repeat { .. } => Some(crate::source::whole(&**source, item)),
                    Level::Summary => {
                        view.summarized.insert(id.clone());
                        item.summary
                    }
                    Level::Pointer => {
                        view.collapsed.insert(id.clone());
                        item.pointer
                    }
                    // The run is the contiguous oldest: its block renders once, where its oldest
                    // item stood, naming the range it held at this start for the whole epoch.
                    Level::Folded => {
                        view.folded.insert(id);
                        if let Some(block) = fold.take() {
                            rendered.push((fingerprint(FOLD_KEY, &block), block.chars()));
                            planned.push(Planned { source: index, id: key(source.name(), FOLD_KEY), element: None, level: Level::Folded, text: block });
                        }
                        continue;
                    }
                };
                if let Some(text) = text {
                    rendered.push((fingerprint(&item_key, &text), text.chars()));
                    planned.push(Planned { source: index, id, element: Some(element), level, text });
                }
            }
            motion.record(source.name(), rendered);
        }
        let laid = lead_with_what_held(planned, previous);
        let mut order: Vec<String> = Vec::with_capacity(sources.len());
        for block in laid {
            let name = sources[block.source].name();
            if !order.iter().any(|seen| seen == name) {
                order.push(name.to_owned());
            }
            view.epoch.observe(block.id, block.element, block.level, block.text);
        }
        // Sources that rendered nothing keep their place after the ones that did.
        for source in &sources {
            if !order.iter().any(|seen| seen == source.name()) {
                order.push(source.name().to_owned());
            }
        }
        motion.started_in(order);
        view.motion = motion;
        let start = Start {
            epoch: id,
            chars: view.epoch.chars(),
            collapsed: view.collapsed.len(),
            summarized: view.summarized.len(),
            folded: view.folded.len(),
            restored_items: 0,
            restored_chars: 0,
        };
        Ok((view, start))
    }

    /// Append what changed in each source since the last act, each item at the rendering its
    /// epoch start chose. An item first seen mid-epoch arrives in full: the allocator weighs
    /// it at the next start.
    /// Whether her view shows `source`'s item `item` this epoch, whole or as its summary. A
    /// collapsed item renders at most its pointer, which is not reading it: the read receipt
    /// (her room cursor) only advances through what this answers true for.
    pub fn renders(&self, source: &str, item: &str) -> bool {
        let id = key(source, item);
        (self.epoch.holds(&id) && !self.collapsed.contains(&id) && !self.repeats.contains(&id)) || self.covered.contains(&id)
    }

    /// Whether a sent tail already showed her `source`'s item `item` this epoch
    /// ([`View::append_sent`]): the caller does not send it in a tail again.
    pub fn covers(&self, source: &str, item: &str) -> bool {
        self.covered.contains(&key(source, item))
    }

    /// Whether this epoch holds `block` exactly, appended under `source` by a sent tail and not
    /// retired: a tail piece she was already sent (the same clock line, the same ask) is not sent
    /// again, or every act would re-append it.
    pub fn holds_sent(&self, source: &str, block: &T) -> bool {
        let prefix = format!("{source}/");
        self.epoch.events().iter().any(|event| !event.retired && event.source.0.starts_with(&prefix) && &event.text == block)
    }

    /// The tail of a prompt that was SENT after this view (the clock line, the ask, her own call
    /// and its result), appended in the exact blocks it was sent under `source`, so the next act's
    /// prompt is this prompt plus what is new and the engine resumes at its end. Before this the
    /// tail was re-rendered every act: her newest result as native protocol at the end of one
    /// prompt and as a `[result #n]` item of her actions in the next, so every act re-prefilled the
    /// previous one's tail (the M5, 2026-11-06: 2,822 s over 78 acts). `covers` names the items
    /// (`(source, key)`) these blocks already show her: they do not arrive again from their source
    /// this epoch; a start re-renders everything from the sources and drops the tail blocks.
    pub fn append_sent(&mut self, source: &str, blocks: Vec<T>, covers: &[(&str, &str)]) -> Advance {
        let mut advance = Advance::default();
        for block in blocks {
            // Unique for the epoch: events only grow once a prompt is sent (rewind never passes it).
            let id = key(source, &self.epoch.events().len().to_string());
            let chars = block.chars();
            if let Observed::Appended { .. } = self.epoch.observe(id, None, Level::Full, block) {
                advance.items += 1;
                advance.chars += chars;
            }
        }
        for (covered_source, item) in covers {
            self.covered.insert(key(covered_source, item));
        }
        // The tail is this act's growth too: the next start reserves room for it.
        match self.appended.last_mut() {
            Some(last) => *last += advance.chars,
            None => self.appended.push(advance.chars),
        }
        advance
    }

    /// Whether `id` is held whole this epoch: rendered at its whole level, not collapsed,
    /// summarized, folded or a repeat. What a repeat may point at.
    fn holds_whole(&self, id: &SourceId) -> bool {
        let held = self.epoch.holds(id)
            && !self.collapsed.contains(id)
            && !self.summarized.contains(id)
            && !self.folded.contains(id)
            && !self.repeats.contains(id);
        // A sent tail showed it in the bytes she read it in: whole.
        held || self.covered.contains(id)
    }

    pub fn advance(&mut self, sources: &[&dyn Source<T>]) -> Advance {
        self.act = Some(self.epoch.mark());
        self.append(sources, None)
    }

    /// Redo this act's append within `limit` characters for the whole view, reducing only an
    /// arrival that is itself `oversized`: larger than a start would grant it, so a turnover
    /// would reduce it too. It arrives at its summary if that fits, else its pointer, and keeps
    /// that level for the epoch. Every other arrival stays whole even when the view is then
    /// still over `limit`: the caller turns over, which shrinks the OLDEST items, never her
    /// newest (goal 7; Cormac on #5422: a 3k read into a nearly full view must not come back as
    /// an outline she reads again). An item without a pointer arrives whole regardless.
    /// `None` when no unsent act is held (a start, a restored view, or an act already
    /// [`sent`](View::sent)).
    ///
    /// This is admission, the alternative to a turnover: an arrival larger than what the act
    /// grants her view (a whole 760-line file read, 51,827 chars) used to arrive in full and
    /// start a new epoch, the one full re-prefill (Aiko, the M5, 2026-10-25: 11 turnovers in
    /// 43 acts). Reduced, it costs its outline or pointer, she can reopen it by its key, and
    /// the epoch keeps appending.
    pub fn readmit(&mut self, sources: &[&dyn Source<T>], limit: usize, oversized: usize) -> Option<Advance> {
        let mark = self.act?;
        self.epoch.rewind(mark);
        self.appended.pop();
        Some(self.append(sources, Some((limit, oversized))))
    }

    /// The prompt rendered from this act was sent: the engine may now hold every token of it,
    /// so nothing may rewind past it and [`View::readmit`] answers `None` until the next act.
    pub fn sent(&mut self) {
        self.act = None;
        self.submitted = Some(self.receipt());
    }

    /// The receipt of the last sent prompt, once: the caller takes it where the request that
    /// carried it was served, and drops it when the request failed.
    pub fn take_submitted(&mut self) -> Option<Receipt> {
        self.submitted.take()
    }

    fn append(&mut self, sources: &[&dyn Source<T>], limit: Option<(usize, usize)>) -> Advance {
        let mut advance = Advance::default();
        self.last_by_source.clear();
        for source in sources {
            let name = source.name();
            let before = (advance.items, advance.chars);
            let (items, repeats) = repeat::resolve(
                &**source,
                source.items(),
                |earlier| self.holds_whole(&key(name, &earlier.key)),
                |item| !self.epoch.holds(&key(name, &item.key)),
            );
            for (item, repeat_of) in items.into_iter().zip(repeats) {
                let id = key(source.name(), &item.key);
                let repeat = repeat_of.is_some();
                if repeat {
                    self.repeats.insert(id.clone());
                }
                // A folded item is held by its source's fold block, fixed at the start: it
                // arrives on its own only when a later start raises it (the prefix never moves).
                // A covered item is in her view already, in the bytes a sent tail showed her.
                if self.folded.contains(&id) || self.covered.contains(&id) {
                    continue;
                }
                // An item with no pointer would leave her view entirely if collapsed (a peer's
                // new turn), so it always arrives whole: admission never drops what she has
                // not seen.
                let fits = |chars: usize| limit.is_none_or(|(limit, _)| self.epoch.chars() + chars <= limit); // no limit admits every arrival whole
                let oversized = limit.is_some_and(|(_, oversized)| crate::source::whole_chars(&item) > oversized);
                let level = if self.collapsed.contains(&id) {
                    Level::Pointer
                } else if self.summarized.contains(&id) {
                    Level::Summary
                } else if self.epoch.holds(&id) || item.pointer.is_none() || !oversized || fits(crate::source::whole_chars(&item)) {
                    Level::Full
                } else if item.summary.as_ref().is_some_and(|summary| fits(summary.chars())) {
                    self.summarized.insert(id.clone());
                    Level::Summary
                } else {
                    self.collapsed.insert(id.clone());
                    Level::Pointer
                };
                // A repeat renders its block where its whole would go; the receipt says which.
                let level = match (level, repeat_of) {
                    (Level::Full, Some(of)) => Level::Repeat { of },
                    (level, _) => level,
                };
                let element = item.id();
                let text = match level {
                    Level::Full | Level::Repeat { .. } => Some(crate::source::whole(&**source, item)),
                    Level::Summary => item.summary.or(item.pointer),
                    Level::Pointer | Level::Folded => item.pointer,
                };
                if let Some(text) = text {
                    let chars = text.chars();
                    if let Observed::Appended { .. } = self.epoch.observe(id, Some(element), level, text) {
                        advance.items += 1;
                        advance.chars += chars;
                        advance.repeats += usize::from(repeat);
                    }
                }
            }
            for item in source.retired() {
                let chars = item.full.chars();
                if let Observed::Appended { .. } = self.epoch.retire(key(source.name(), &item.key), item.full) {
                    advance.items += 1;
                    advance.chars += chars;
                }
            }
            for (id, notice) in if source.states() { self.ended(&**source) } else { Vec::new() } {
                let chars = notice.chars();
                if let Observed::Appended { .. } = self.epoch.retire(id, notice) {
                    advance.items += 1;
                    advance.chars += chars;
                }
            }
            if advance.items > before.0 {
                self.last_by_source.push(SourceAdvance {
                    source: name.to_owned(),
                    items: advance.items - before.0,
                    chars: advance.chars - before.1,
                });
            }
        }
        self.appended.push(advance.chars);
        advance
    }

    /// The items of `source` this epoch holds (their latest version not retired) that the source no
    /// longer offers, each with the notice that ends it, for a source of states ([`Source::ended`]).
    /// Its fold block and anything a sent tail covers are not its items to end.
    fn ended(&self, source: &dyn Source<T>) -> Vec<(SourceId, T)> {
        let prefix = format!("{}/", source.name());
        let offered: HashSet<String> = source.items().into_iter().map(|item| item.key).collect();
        let mut latest: std::collections::BTreeMap<&SourceId, bool> = std::collections::BTreeMap::new();
        for event in self.epoch.events().iter().filter(|event| event.source.0.starts_with(&prefix)) {
            latest.insert(&event.source, event.retired);
        }
        latest
            .into_iter()
            .filter(|(id, retired)| !retired && !self.covered.contains(*id))
            .filter_map(|(id, _)| {
                let item = &id.0[prefix.len()..];
                (item != FOLD_KEY && !offered.contains(item)).then(|| source.ended(item).map(|notice| (id.clone(), notice)))?
            })
            .collect()
    }

    /// The view in order: the head, then every item as it arrived.
    pub fn render(&self) -> impl Iterator<Item = &T> {
        self.epoch.render()
    }

    /// What she is shown, in prompt order: each block after the head with its occurrence, the
    /// level the view rendered it at, its version and its size. Taken where the prompt is
    /// SUBMITTED (the caller binds owner, memory incarnation and request there), never at
    /// rendering: a prompt built but not accepted was not seen (HER-TOOLS, the render receipt).
    pub fn receipt(&self) -> Receipt {
        Receipt {
            epoch: self.epoch.id(),
            items: self
                .epoch
                .events()
                .iter()
                .map(|event| Rendered {
                    source: event.source.clone(),
                    element: event.element,
                    level: event.level,
                    version: event.version,
                    retired: event.retired,
                    chars: event.text.chars(),
                })
                .collect(),
        }
    }

    /// How each source moved across this view's starts.
    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    pub fn epoch_id(&self) -> u64 {
        self.epoch.id()
    }

    /// What the last append added, per source (sources that appended nothing are absent).
    pub fn last_advance_by_source(&self) -> &[SourceAdvance] {
        &self.last_by_source
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

/// One block a start renders: which source (its index in the start's order), its id, its level
/// and its bytes.
struct Planned<T> {
    source: usize,
    id: SourceId,
    element: Option<crate::cause::ElementId>,
    level: Level,
    text: T,
}

/// A start's blocks in the order that shares the most with the epoch it replaces: the previous
/// epoch's blocks, in its order, for as long as this start renders each of them byte for byte,
/// then every other block in the start's order (its sources most stable first, each source's
/// items in their own order). The engine reuses a prompt only up to its first difference, so
/// every unchanged block the previous prompt led with is reused in place, however the measured
/// order would have ranked it, and the first block it holds in new bytes follows them, so its
/// unchanged front is reused too (the M5, 2026-11-06: 13 turnovers, every one resumed from the
/// system head alone because the start's front moved). A source's items never pass one another:
/// a block of the previous epoch is reused only when everything its source lays down before it
/// already was. `previous` is `None` at a view's first start.
fn lead_with_what_held<T: Block>(planned: Vec<Planned<T>>, previous: Option<&Epoch<T>>) -> Vec<Planned<T>> {
    let Some(previous) = previous else {
        return planned;
    };
    let at: std::collections::HashMap<&SourceId, usize> = planned.iter().enumerate().map(|(index, block)| (&block.id, index)).collect();
    // The next block each source may lay down: its blocks are contiguous in `planned`, in order.
    let mut next: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (index, block) in planned.iter().enumerate() {
        next.entry(block.source).or_insert(index);
    }
    let mut lead = Vec::new();
    for event in previous.events() {
        let Some(&index) = at.get(&event.source) else {
            break; // the previous prompt held a block this start does not render: the prefix ends
        };
        let block = &planned[index];
        if next.get(&block.source) != Some(&index) {
            break;
        }
        lead.push(index);
        next.insert(block.source, index + 1);
        // The same block in new bytes ends the reuse, laid right where it stood: the engine still
        // reuses its bytes up to their first difference (a card appended at a list's end keeps
        // the whole list before it).
        if block.text != event.text {
            break;
        }
    }
    let led: HashSet<usize> = lead.iter().copied().collect();
    let mut slots: Vec<Option<Planned<T>>> = planned.into_iter().map(Some).collect();
    let mut laid: Vec<Planned<T>> = lead.iter().filter_map(|&index| slots[index].take()).collect();
    laid.extend(slots.into_iter().enumerate().filter(|(index, _)| !led.contains(index)).filter_map(|(_, block)| block));
    laid
}

#[cfg(test)]
mod tests;
