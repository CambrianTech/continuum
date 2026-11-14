//! One mind's view: an epoch fed by her sources.
//!
//! Within an epoch every act only appends: the items whose rendering changed. Turnover is
//! the only rewrite. At a start the caller's one allocator grants each source a budget, in the
//! caller's cost (the price it charges the final prompt with), between its floor (its items folded into one block when the source folds, else
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
use crate::cost::Estimate;
use crate::motion::{fingerprint, Motion};
use crate::source::{Item, Source};

mod allot;
mod held;
mod log;
mod repeat;
mod tail;
pub use tail::Judged;

use allot::{allot, rank};
pub use crate::level::Level;
pub use allot::floor;

/// The item key a source's fold block is observed under: no source keys an item with it.
const FOLD_KEY: &str = "(folded)";

/// One source as the allocator sees it at an epoch start, every size priced by the caller's cost
/// (`crate::cost`), the unit its grants come back in.
#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub source: String,
    pub priority: u8,
    /// What the source costs at its least ([`floor`]): one fold block when the source folds
    /// its items, else every item collapsed to its pointer.
    pub floor: usize,
    /// What the source costs in full: its want. A folded item still wants its whole.
    pub full: usize,
    /// Its newest item in full, with every item her work in hand holds open (`Item::in_hand`): the
    /// newest end a start never collapses when it fits.
    pub newest: usize,
    /// What of its floor her collapsed log already lays down byte for byte.
    pub logged: usize,
    /// What the engine holds of it from the live epoch (the events its last sent prompt carried,
    /// every version it superseded included): computed once and resident. An act granting less turns the epoch over and re-prefills
    /// the whole view behind it (on the M5, 2026-11-10, 65% of all re-prefilled tokens, an epoch
    /// every second act: the act's other claims took back the reserve a start had left). 0 at a
    /// start, which recomputes anyway.
    pub held: usize,
    /// Its items carry acts, so a re-read of one is measured (`crate::rereads`).
    pub instrumented: bool,
    /// What a deeper start could collapse, newest first, with her measured re-read chance for each
    /// (`crate::depth`): every item but the newest not yet in her log.
    pub collapsible: Vec<crate::depth::Collapsible>,
}

impl Claim {
    /// This source as the depth choice weighs it (`crate::depth::choose`): `share` is what the
    /// caller's rule gives it, in the same cost as the claim.
    pub fn growing(&self, share: usize) -> crate::depth::Growing<'_> {
        crate::depth::Growing { share, base: self.floor + self.newest, logged: self.logged, instrumented: self.instrumented, collapsible: &self.collapsible }
    }
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
    /// Arrivals rendered as a repeat of an occurrence held whole (CONTINUUM-ENGINE section 5 invariant 4): the
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
    /// Characters of her collapsed log this start laid down again as they were.
    pub log_kept_chars: usize,
    /// The log folded at this start: a source's logged history alone outgrew its grant, so every
    /// item was re-decided from scratch (the one full re-prefill the log costs).
    pub log_folded: bool,
    /// Characters of her log this start dropped because their source was absent from it: charged
    /// to no one, so never laid down.
    pub log_dropped_chars: usize,
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
    /// Items whose element her epoch already shows whole under another key (her act's tail showed
    /// act k; her actions now offer it as `#k`): one element, one occurrence, so the item never
    /// arrives again as its source renders it until a start re-renders everything, and she read it
    /// where it was shown ([`View::renders`]). Absent in a view saved before the tail joined the
    /// view, which covered nothing.
    #[serde(default)]
    covered: HashSet<SourceId>,
    /// Items of a source the start's allocation SHED for want of its floor ([`View::withhold`]):
    /// they wait for a start that funds them, never arriving on their own mid-epoch. A shed
    /// source's backlog arriving whole on the next act appended 270k chars and turned the epoch
    /// over on every other act (Atlas, the M5, 2026-11-07: `thoughts:287/270351`). Its items
    /// that arise after the start arrive as any new item does. Absent in a view saved before it,
    /// which withheld nothing.
    #[serde(default)]
    withheld: HashSet<SourceId>,
    /// Her collapsed history, in the order it collapsed, each block as first rendered: laid down
    /// again byte for byte at every start (`view/log.rs`). Absent in a view saved before it, which
    /// starts its log at its next start.
    #[serde(default = "Vec::new")]
    log: Vec<log::Logged<T>>,
    /// Characters each act of this epoch appended, in order.
    appended: Vec<usize>,
    /// How each source moved across this view's starts; carried over every turnover and
    /// restart, it orders the next start. Absent in a view saved before it existed.
    #[serde(default)]
    motion: Motion,
    /// Per source, the key of the newest item this epoch's start folded ([`View::fold_edge`]):
    /// every item of that source up to it renders only inside its fold block, so its owner may let
    /// it leave RAM. Absent in a view saved before it, which names no edge.
    #[serde(default)]
    fold_edges: std::collections::BTreeMap<String, String>,
    /// Where this act's appends began, until its prompt is sent: what [`View::readmit`]
    /// redoes. Never saved: a restored view has no unsent act.
    #[serde(skip)]
    act: Option<Mark>,
    /// What the last sent prompt showed her, until the request that carried it is served:
    /// taken there ([`View::take_submitted`]), so a prompt the engine never accepted is never
    /// receipted. Never saved: a restored view has nothing in flight.
    #[serde(skip)]
    submitted: Option<Receipt>,
    /// How many of this epoch's events the last sent prompt carried: what the engine holds of it.
    /// 0 from a start until its first send, and in a restored view until its next: an epoch the
    /// engine has not received holds nothing a claim must keep ([`Claim::held`]).
    #[serde(default)]
    sent_events: usize,
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

fn claim<T: Block>(cost: &dyn Estimate<T>, source: &dyn Source<T>, items: &[Item<T>], rereads: &crate::rereads::Rereads) -> Claim {
    Claim {
        source: source.name().to_owned(),
        priority: source.priority(),
        floor: floor(cost, source, items),
        full: items.iter().map(|item| crate::cost::whole(item, cost)).sum(),
        newest: newest_end(cost, items),
        logged: 0,
        held: 0,
        instrumented: items.iter().any(|item| item.act().is_some()),
        collapsible: crate::depth::collapsible(items, rereads, cost),
    }
}

/// What a start keeps whole when it fits: the newest item, and every item her work in hand holds
/// open (`Item::in_hand`), each at its whole.
fn newest_end<T: Block>(cost: &dyn Estimate<T>, items: &[Item<T>]) -> usize {
    let Some((newest, older)) = items.split_last() else {
        return 0; // a source with no items has no newest end
    };
    crate::cost::whole(newest, cost) + older.iter().filter(|item| item.in_hand).map(|item| crate::cost::whole(item, cost)).sum::<usize>()
}

impl Claim {
    /// Its weight in the window's one allocation: its source's priority scaled by how often she
    /// asks again for what it would collapse, `priority x (1 + re-reads per act)`, the sum of her
    /// measured re-read rate over its collapsible items. A source she keeps re-reading from wins
    /// room from one she does not (the M5, 2026-11-09: her acts and her 300k-token conversation
    /// split the room evenly at priority 5 while 70-90% of her file reads re-read one 3-5 acts
    /// old). Unmeasured ages add nothing, so an unmeasured source keeps its priority.
    pub fn weight(&self) -> u8 {
        let rereads: f64 = self.collapsible.iter().filter_map(|item| item.hazard.per_act()).sum();
        (f64::from(self.priority) * (1.0 + rereads)).round().min(f64::from(u8::MAX)) as u8
    }
}

/// What each source would claim at an epoch start, without starting one, priced by `cost`: the
/// caller's one allocation weighs these beside its own claims and hands its grants back to
/// [`View::start`] / [`View::turn_over`]. With no view there is no measured re-read: a fresh
/// clock's, every age Unmeasured.
pub fn claims<T: Block>(cost: &dyn Estimate<T>, sources: &[&dyn Source<T>]) -> Vec<Claim> {
    let fresh = crate::rereads::Rereads::default();
    sources.iter().map(|source| claim(cost, *source, &source.items(), &fresh)).collect()
}

impl<T: Block> View<T> {
    /// What each source would claim at a start, priced by `cost`, with HER measured re-read rate
    /// on each collapsible item (`Claim::collapsible`); [`claims`] has no view and so no measured
    /// re-read.
    pub fn claims(&self, cost: &dyn Estimate<T>, sources: &[&dyn Source<T>]) -> Vec<Claim> {
        sources
            .iter()
            .map(|source| Claim { held: self.held(cost, source.name()), ..claim(cost, *source, &source.items(), self.motion.rereads()) })
            .collect()
    }

    /// What the engine holds of `source` from this epoch: its events the last sent prompt carried,
    /// every superseded version included, priced by `cost`. An unsent epoch holds nothing.
    fn held(&self, cost: &dyn Estimate<T>, source: &str) -> usize {
        let prefix = format!("{source}/");
        self.epoch.events()[..self.sent_events.min(self.epoch.events().len())]
            .iter()
            .filter(|event| event.source.0.starts_with(&prefix))
            .map(|event| cost.block(&event.text))
            .sum()
    }

    /// The first epoch. `allocate` answers each claim with a grant in `cost`, the caller's price
    /// of a block (the one it charges the final prompt with): the start keeps within each grant
    /// as `cost` prices what it renders, never through a converted unit.
    pub fn start(
        cost: &dyn Estimate<T>,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<(Self, Start), GrantMismatch> {
        Self::begin(cost, head, sources, allocate, Carried { id: 1, motion: Motion::default(), previous: None, log: Vec::new() }).map_err(|refused| refused.0)
    }

    /// Replace this epoch with a fresh one rendered from the sources' current state at the
    /// grants the allocator makes now, in `cost`.
    pub fn turn_over(
        &mut self,
        cost: &dyn Estimate<T>,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
    ) -> Result<Start, GrantMismatch> {
        let motion = std::mem::take(&mut self.motion);
        let log = std::mem::take(&mut self.log);
        let (next, mut start) = match Self::begin(cost, head, sources, allocate, Carried { id: self.epoch.id() + 1, motion, previous: Some(&self.epoch), log }) {
            Ok(begun) => begun,
            Err(refused) => {
                let (mismatch, motion, log) = *refused;
                self.motion = motion;
                self.log = log;
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
    /// An item her collapsed log holds rises too: whole at the front of its source's items, its log
    /// entry kept where it was (`view/log.rs`).
    pub fn start_would_restore(&self, cost: &dyn Estimate<T>, sources: &[&dyn Source<T>], allocate: impl FnOnce(&[Claim]) -> Vec<usize>) -> Result<bool, GrantMismatch> {
        let arranged = log::arrange(cost, &self.motion, sources, &self.log);
        let grants = allocate(&arranged.claims);
        if grants.len() != arranged.claims.len() {
            return Err(GrantMismatch { claims: arranged.claims.len(), grants: grants.len() });
        }
        Ok(arranged.sources.iter().zip(&arranged.items).zip(grants.iter().zip(&arranged.logged)).any(|((source, items), (grant, logged))| {
            allot(&**source, items, grant.saturating_sub(*logged), cost).levels.iter().zip(items).any(|(level, item)| {
                let id = key(source.name(), &item.key);
                self.restores(&id, level)
            })
        }))
    }

    fn restores(&self, id: &SourceId, level: &Level) -> bool {
        // A withheld item holds nothing: any rise past a pointer gives her back what the shed took.
        if self.withheld.contains(id) {
            return rank(level) > rank(&Level::Pointer);
        }
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

    /// Render a start: her sources in the order `motion` measured (`Motion::rank`), her collapsed
    /// log laid down again as it was with what collapses now appended to it (`view/log.rs`), and
    /// every block placed so the start shares the longest prefix with the epoch it replaces
    /// (`previous`, [`lead_with_what_held`]). A refusal hands `motion` and the log back.
    #[allow(clippy::type_complexity)] // the refusal returns what it was handed: the mismatch, the measurements and the log
    fn begin(
        cost: &dyn Estimate<T>,
        head: Vec<T>,
        sources: &[&dyn Source<T>],
        allocate: impl FnOnce(&[Claim]) -> Vec<usize>,
        carried: Carried<'_, T>,
    ) -> Result<(Self, Start), Box<(GrantMismatch, Motion, Vec<log::Logged<T>>)>> {
        let Carried { id, mut motion, previous, log } = carried;
        motion.close_epoch();
        let arranged = log::arrange(cost, &motion, sources, &log);
        {
            let views: Vec<(&str, &[Item<T>])> = arranged.sources.iter().zip(&arranged.items).map(|(source, items)| (source.name(), items.as_slice())).collect();
            motion.rereads_mut().start(&views);
        }
        let grants = allocate(&arranged.claims);
        if grants.len() != arranged.claims.len() {
            return Err(Box::new((GrantMismatch { claims: arranged.claims.len(), grants: grants.len() }, motion, log)));
        }
        let granted: std::collections::HashMap<&str, usize> = arranged.sources.iter().zip(&grants).map(|(source, grant)| (source.name(), *grant)).collect();
        // The log folds when a source's logged history alone outgrows what this start grants it:
        // everything is re-decided from scratch once, and the log starts again from what this
        // start collapses.
        // Her log as this start charged it: an absent source's entries leave with it.
        let mut log_dropped_chars = 0;
        let log: Vec<log::Logged<T>> = log
            .into_iter()
            .zip(&arranged.retained)
            .filter_map(|(entry, kept)| {
                if !kept {
                    log_dropped_chars += entry.text.chars();
                }
                kept.then_some(entry)
            })
            .collect();
        let folds = arranged.logged.iter().zip(&grants).any(|(logged, grant)| logged > grant);
        let (log, arranged) = if folds { (Vec::new(), log::arrange(cost, &motion, sources, &[])) } else { (log, arranged) };
        let mut view = Self {
            epoch: Epoch::new(id, head),
            collapsed: HashSet::new(),
            summarized: HashSet::new(),
            folded: HashSet::new(),
            repeats: HashSet::new(),
            covered: HashSet::new(),
            withheld: HashSet::new(),
            log: Vec::new(),
            appended: Vec::new(),
            motion: Motion::default(),
            fold_edges: std::collections::BTreeMap::new(),
            act: None,
            submitted: None,
            sent_events: 0,
            last_by_source: Vec::new(),
        };
        for entry in &log {
            match entry.level {
                Level::Summary => view.summarized.insert(entry.id.clone()),
                Level::Folded => {
                    view.folded.extend(entry.covers.iter().cloned());
                    true
                }
                _ => view.collapsed.insert(entry.id.clone()),
            };
        }
        let sources_n = arranged.sources.len();
        let logged_level: std::collections::HashMap<SourceId, Level> =
            log.iter().flat_map(|entry| std::iter::once((entry.id.clone(), entry.level)).chain(entry.covers.iter().map(|id| (id.clone(), Level::Folded)))).collect();
        let mut fresh_log: Vec<log::Logged<T>> = Vec::new();
        let mut planned_by_source: Vec<Vec<Planned<T>>> = Vec::with_capacity(sources_n);
        for (index, (source, items)) in arranged.sources.iter().zip(arranged.items).enumerate() {
            let name = source.name();
            let grant = granted.get(name).copied().unwrap_or(0).saturating_sub(arranged.logged[index]); // unwrap_or: every arranged source was granted (the allocator answered for each)
            let allot::Allotment { levels, mut fold } = allot(&**source, &items, grant, cost);
            let fold_run: Vec<String> = items.iter().zip(&levels).filter(|(_, level)| **level == Level::Folded).map(|(item, _)| item.key.clone()).collect();
            if let Some(newest) = fold_run.last() {
                view.fold_edges.insert(name.to_owned(), newest.clone());
            }
            let mut rendered: Vec<(u64, usize)> = log.iter().filter(|entry| entry.source == name).map(|entry| (fingerprint(&entry.id.0, &entry.text), entry.text.chars())).collect();
            let mut planned = Vec::new();
            // Her oldest collapsed run is logged, up to the first item this start keeps whole.
            let mut oldest = true;
            let mut fold_at: Option<(bool, usize)> = None;
            for (item, level) in items.into_iter().zip(levels) {
                let id = key(name, &item.key);
                let element = item.id();
                let item_key = item.key.clone(); // its fingerprint after the level consumed it; once per item per start
                if let Some(held) = logged_level.get(&id) {
                    // Logged: it stands in her log; only a rise past what it holds there renders it
                    // again, at that level, its log entry kept.
                    if !(rank(&level) > rank(held) && rank(&level) > rank(&Level::Pointer)) {
                        continue;
                    }
                    view.collapsed.remove(&id);
                    view.folded.remove(&id);
                    view.summarized.remove(&id);
                }
                let text = match level {
                    // A start re-renders every item as itself: no repeat or page is allotted here.
                    Level::Full | Level::Repeat { .. } | Level::Page => {
                        oldest = false;
                        Some(crate::source::whole(&**source, item))
                    }
                    Level::Summary | Level::Pointer => {
                        if level == Level::Summary { view.summarized.insert(id.clone()) } else { view.collapsed.insert(id.clone()) };
                        let text = if level == Level::Summary { item.summary } else { item.pointer };
                        match text {
                            // Written to the log once, as rendered now.
                            Some(text) if oldest && !logged_level.contains_key(&id) => {
                                rendered.push((fingerprint(&item_key, &text), text.chars()));
                                fresh_log.push(log::Logged { source: name.to_owned(), id: id.clone(), element: Some(element), level, text, covers: Vec::new() });
                                None
                            }
                            text => text,
                        }
                    }
                    // The run is the contiguous oldest: its block renders once, where its oldest
                    // item stood, naming the range it held at this start; logged when it is her
                    // oldest history and holds an item her log did not.
                    Level::Folded => {
                        view.folded.insert(id.clone());
                        match fold_at {
                            Some((true, at)) => fresh_log[at].covers.push(id),
                            Some((false, _)) => {}
                            None => {
                                if let Some(block) = fold.take() {
                                    rendered.push((fingerprint(FOLD_KEY, &block), block.chars()));
                                    let block_id = log::fold_id(name, fold_run.first().map_or("", String::as_str), fold_run.last().map_or("", String::as_str)); // map_or: a fold block always has a run
                                    if oldest {
                                        fold_at = Some((true, fresh_log.len()));
                                        fresh_log.push(log::Logged { source: name.to_owned(), id: block_id, element: None, level, text: block, covers: vec![id] });
                                    } else {
                                        fold_at = Some((false, 0));
                                        planned.push(Planned { source: index, id: block_id, element: None, level, text: block });
                                    }
                                }
                            }
                        }
                        continue;
                    }
                };
                if let Some(text) = text {
                    rendered.push((fingerprint(&item_key, &text), text.chars()));
                    planned.push(Planned { source: index, id, element: Some(element), level, text });
                }
            }
            motion.record(name, rendered);
            planned_by_source.push(planned);
        }
        let log_kept_chars: usize = log.iter().map(|entry| entry.text.chars()).sum();
        let mut all_log = log;
        all_log.extend(fresh_log);
        // The log stands where the measured order put it: as one unit, its blocks in the order
        // they collapsed.
        let log_blocks: Vec<Planned<T>> = all_log
            .iter()
            .map(|entry| Planned { source: sources_n, id: entry.id.clone(), element: entry.element, level: entry.level, text: entry.text.clone() })
            .collect();
        let mut planned: Vec<Planned<T>> = Vec::new();
        let mut log_blocks = Some(log_blocks);
        for (index, blocks) in planned_by_source.into_iter().enumerate() {
            if index == arranged.log_at {
                planned.extend(log_blocks.take().unwrap_or_default()); // unwrap_or_default: laid once
            }
            planned.extend(blocks);
        }
        planned.extend(log_blocks.take().unwrap_or_default()); // unwrap_or_default: the log stands last when no source follows it
        let laid = lead_with_what_held(planned, previous);
        let mut order: Vec<String> = Vec::with_capacity(sources_n + 1);
        for block in laid {
            let name = arranged.sources.get(block.source).map_or(log::LOG, |source| source.name()); // map_or: the index past the sources is her log
            if !order.iter().any(|seen| seen == name) {
                order.push(name.to_owned());
            }
            view.epoch.observe(block.id, block.element, block.level, block.text);
        }
        // Sources that rendered nothing keep their place after the ones that did.
        for name in arranged.sources.iter().map(|source| source.name()).chain([log::LOG]) {
            if !order.iter().any(|seen| seen == name) {
                order.push(name.to_owned());
            }
        }
        motion.started_in(order);
        view.motion = motion;
        view.log = all_log;
        let start = Start {
            epoch: id,
            chars: view.epoch.chars(),
            collapsed: view.collapsed.len(),
            summarized: view.summarized.len(),
            folded: view.folded.len(),
            restored_items: 0,
            restored_chars: 0,
            log_kept_chars,
            log_folded: folds,
            log_dropped_chars,
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

    /// Whether `id` is held whole this epoch: rendered at its whole level, not collapsed,
    /// summarized, folded or a repeat. What a repeat may point at.
    fn holds_whole(&self, id: &SourceId) -> bool {
        let held = self.epoch.holds(id)
            && !self.collapsed.contains(id)
            && !self.summarized.contains(id)
            && !self.folded.contains(id)
            && !self.repeats.contains(id);
        // Her tail showed its element whole, under the tail's key.
        held || self.covered.contains(id)
    }

    /// The sources this epoch's start SHED (the allocation could not grant their floor): every
    /// item each holds now waits for a later start that funds it, as a folded item waits, instead
    /// of arriving whole at the next [`advance`](View::advance), which turned the epoch over at once
    /// and measured the backlog as one act's growth, so the start after it reserved nothing.
    /// [`start_would_restore`](View::start_would_restore) counts a start that would show one as a
    /// restore. Called right after the start that shed them; a later start re-decides each.
    pub fn withhold(&mut self, shed: &[&dyn Source<T>]) {
        for source in shed {
            let name = source.name();
            self.withheld.extend(source.items().iter().map(|item| key(name, &item.key)));
        }
    }

    pub fn advance(&mut self, sources: &[&dyn Source<T>]) -> Advance {
        self.act = Some(self.epoch.mark());
        let advance = self.append(sources, None);
        // Her re-requests, aged against what this act shows her (`crate::rereads`).
        let mut rereads = std::mem::take(self.motion.rereads_mut());
        rereads.act(sources, |source, item| self.holds_whole(&key(source, item)));
        *self.motion.rereads_mut() = rereads;
        advance
    }

    /// Redo this act's append within `limit` for the whole view, priced by the caller's `cost` (the
    /// unit `limit` and `oversized` are in), reducing only an
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
    pub fn readmit(&mut self, cost: &dyn Estimate<T>, sources: &[&dyn Source<T>], limit: usize, oversized: usize) -> Option<Advance> {
        let mark = self.act?;
        self.epoch.rewind(mark);
        self.appended.pop();
        Some(self.append(sources, Some(Admission { limit, oversized, cost })))
    }

    /// What the whole view costs, head included, priced by `cost`.
    fn cost(&self, cost: &dyn Estimate<T>) -> usize {
        self.epoch.render().map(|block| cost.block(block)).sum()
    }

    /// The prompt rendered from this act was sent: the engine may now hold every token of it,
    /// so nothing may rewind past it and [`View::readmit`] answers `None` until the next act.
    pub fn sent(&mut self) {
        self.sent_events = self.epoch.events().len();
        self.act = None;
        self.submitted = Some(self.receipt());
    }

    /// The receipt of the last sent prompt, once: the caller takes it where the request that
    /// carried it was served, and drops it when the request failed.
    pub fn take_submitted(&mut self) -> Option<Receipt> {
        self.submitted.take()
    }

    fn append(&mut self, sources: &[&dyn Source<T>], limit: Option<Admission<'_, T>>) -> Advance {
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
                // A covered item is in her view already, where her tail showed its element whole.
                // A withheld item waits for a start that funds its source (`View::withhold`).
                if self.folded.contains(&id) || self.covered.contains(&id) || self.withheld.contains(&id) {
                    continue;
                }
                // One element, one occurrence: an item whose element her epoch already shows under
                // another key (her act's tail showed act k; her actions now offer it as `#k`). Shown
                // whole there, she read it there: covered for the epoch.
                if !self.epoch.holds(&id) && self.epoch.holds_element(item.id()) {
                    if self.epoch.holds_element_whole(item.id()) {
                        self.covered.insert(id);
                    }
                    continue;
                }
                // An item with no pointer would leave her view entirely if collapsed (a peer's
                // new turn), so it always arrives whole: admission never drops what she has
                // not seen.
                // Admission prices in the caller's cost; with no limit every arrival is whole.
                let (oversized, whole_fits, summary_fits) = match &limit {
                    None => (false, true, false),
                    Some(admit) => {
                        let held = self.cost(admit.cost);
                        let whole = crate::cost::whole(&item, admit.cost);
                        let fits = |price: usize| held + price <= admit.limit;
                        (whole > admit.oversized, fits(whole), item.summary.as_ref().is_some_and(|summary| fits(admit.cost.block(summary))))
                    }
                };
                let level = if self.collapsed.contains(&id) {
                    Level::Pointer
                } else if self.summarized.contains(&id) {
                    Level::Summary
                } else if self.epoch.holds(&id) || item.pointer.is_none() || !oversized || whole_fits {
                    Level::Full
                } else if summary_fits {
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
                    Level::Summary | Level::Page => item.summary.or(item.pointer),
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
            // Her per-act change rate, what the next start ranks by (an act's redo is not an act).
            if limit.is_none() {
                self.motion.act_saw(name, advance.items > before.0);
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
    /// Its fold block and anything her tail covers are not its items to end.
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
                (!item.starts_with(FOLD_KEY) && !offered.contains(item)).then(|| source.ended(item).map(|notice| (id.clone(), notice)))?
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

    /// The key of `source`'s newest item this epoch's start folded, `None` when it folded none:
    /// every item of `source` up to it (its items are oldest first, the fold run their contiguous
    /// oldest) renders only inside a fold block until a later start, so its owner may let it leave
    /// RAM. Measured by the start that allotted it, never a count. O(1).
    pub fn fold_edge(&self, source: &str) -> Option<&str> {
        self.fold_edges.get(source).map(String::as_str)
    }

    /// `source`'s items that `left` names have left their owner for good (archived): forget them as
    /// folded and as covered by her log's fold blocks. The blocks' text stays as it was rendered, so
    /// the prefix does not move; only the ids go, or the view would hold one id per item she ever
    /// had. O(folded + covered), paid by the start whose fold edge released them.
    pub fn release(&mut self, source: &str, left: impl Fn(&str) -> bool) {
        let prefix = format!("{source}/");
        let gone = |id: &SourceId| id.0.strip_prefix(&prefix).is_some_and(&left);
        self.folded.retain(|id| !gone(id));
        for entry in &mut self.log {
            entry.covers.retain(|id| !gone(id));
        }
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

/// An act's admission ([`View::readmit`]): the whole view within `limit`, reducing only an arrival
/// whose whole is over `oversized`, both in the caller's `cost`.
struct Admission<'a, T> {
    limit: usize,
    oversized: usize,
    cost: &'a dyn Estimate<T>,
}

/// What a start carries over from the view it replaces: its epoch id, her measured motion, the
/// epoch it shares a prefix with (`None` at a view's first start) and her collapsed log.
struct Carried<'a, T> {
    id: u64,
    motion: Motion,
    previous: Option<&'a Epoch<T>>,
    log: Vec<log::Logged<T>>,
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
        // The same block in new bytes ends the reuse. It is laid right where it stood when the
        // measured order puts it next anyway: the engine still reuses its bytes up to their first
        // difference (a card appended at a list's end keeps the whole list before it). A block
        // the order has moved behind others is not pulled back to the front.
        if block.text != event.text {
            if (0..index).all(|earlier| lead.contains(&earlier)) {
                lead.push(index);
            }
            break;
        }
        lead.push(index);
        next.insert(block.source, index + 1);
    }
    let led: HashSet<usize> = lead.iter().copied().collect();
    let mut slots: Vec<Option<Planned<T>>> = planned.into_iter().map(Some).collect();
    let mut laid: Vec<Planned<T>> = lead.iter().filter_map(|&index| slots[index].take()).collect();
    laid.extend(slots.into_iter().enumerate().filter(|(index, _)| !led.contains(index)).filter_map(|(_, block)| block));
    laid
}

#[cfg(test)]
mod tests;
