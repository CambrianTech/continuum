//! How often she asks again for what she already observed, by how long ago she last asked: the
//! re-read cost a turnover's collapse depth weighs (`crate::depth`).
//!
//! An occurrence is an act whose tool only observes ([`ToolEffect::Observes`]) and whose outcome is
//! hashed. At every act the view ages each one she still has (her latest occurrence of each request)
//! and counts it once as EXPOSED at its age, in acts since she last asked; when an arriving act asks
//! the same thing ([`Fingerprint`]) and gets the same outcome ([`ContentHash`]), that is a
//! RE-REQUEST at the earlier occurrence's age. Held whole, the view renders it as a repeat block
//! (MIND-CONTEXT invariant 4); held collapsed (its summary, pointer, a fold block or her log), its
//! whole arrives again. So what a collapse costs is the propensity times the whole it re-sends.
//! A re-request with a CHANGED outcome (the file was edited) is a new observation: keeping the old
//! whole would not have spared it, so it only becomes her latest occurrence.
//!
//! The propensity is counted over every occurrence whatever its level, so an age the current depth
//! never collapses is still measured (counting only collapsed ones would leave the young ages, which
//! today's start keeps whole, unmeasured forever, and the depth could never move toward them). What
//! was held collapsed is tallied beside it, the evidence on whether collapse itself raises her
//! re-requests. On the M5's captures (core ed142f14c, 2026-11-06, four residents) it does not
//! measurably: 14 re-reads of a collapsed item over 49,645 collapsed item-acts (2.8e-4) against 41
//! of a whole one over 15,243 (2.7e-3), the whole ones younger; at matched ages within a factor of
//! two either way (31-62 acts: 5.5e-4 collapsed, 3.5e-4 whole; 15-30: 6.0e-4 and 1.6e-3).
//!
//! Ages are bucketed by powers of two (0, 1-2, 3-6, 7-14, ...): resolution where her behaviour
//! changes fastest, a handful of buckets over a whole session. Each start halves every tally, so the
//! measurement has a half-life of one of her epochs, her own time scale, and follows what she does
//! now.
//!
//! Every age is read off ONE clock, her acts as this measurement counts them: an occurrence's
//! birth is the act at which her view first held it, stamped there ([`Rereads::act`] for an
//! arrival, a start for what it finds), kept with her view across restarts, and never inferred
//! from how many items stand after it (a source that sat idle for eight acts, or appended three
//! items in one, would sample the wrong bucket: Codex on #5806). The depth choice reads the same
//! clock ([`Rereads::age`]). Tallies and births survive a restart with her view; the index of
//! occurrences is rebuilt from those births at the first act after it, counting no re-request for
//! that act.

use std::collections::{BTreeMap, HashMap};

use crate::cause::{Content, ContentHash, ElementId, Fingerprint, ToolEffect};
use crate::epoch::Block;
use crate::source::{Item, Source};

/// One age bucket's counts, decayed: acts at which an occurrence of this age was exposed, and
/// re-requests of an occurrence at this age.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tally {
    pub exposure: f64,
    pub events: f64,
}

/// What one age bucket measured, for the depth choice: Unmeasured until an occurrence of that age
/// was exposed at least once, never a guessed rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hazard {
    Unmeasured,
    Measured(Tally),
}

impl Hazard {
    /// The chance per act that an occurrence of this age is asked for again: (events + 1) /
    /// (exposure + 1), one more event than seen, so a bucket that saw none on few exposures still
    /// prices a re-read (a small sample errs toward keeping her history, never toward collapsing it).
    pub fn per_act(self) -> Option<f64> {
        match self {
            Hazard::Unmeasured => None,
            Hazard::Measured(tally) => Some((tally.events + 1.0) / (tally.exposure + 1.0)),
        }
    }

    /// What was observed, without the extra event: the receipt a prediction is checked against.
    pub fn observed(self) -> Option<f64> {
        match self {
            Hazard::Unmeasured => None,
            Hazard::Measured(tally) => Some(tally.events / tally.exposure),
        }
    }
}

/// The age bucket of an occurrence `age` acts after she last asked for it.
pub fn bucket(age: u64) -> usize {
    age.saturating_add(1).ilog2() as usize
}

/// Per source, per age bucket.
pub type Curves = BTreeMap<String, Vec<Tally>>;

/// Her latest occurrence of one request.
#[derive(Clone, Debug)]
struct Held {
    source: String,
    key: String,
    fingerprint: Fingerprint,
    content: ContentHash,
    /// The act at which her view first held it ([`Rereads::births`]).
    born: u64,
}

/// Her re-request measurement, kept with her view.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Rereads {
    /// Per source, per age bucket, over every occurrence.
    any: Curves,
    /// The same, counting only acts at which the occurrence was held collapsed.
    collapsed: Curves,
    /// Her acts, counted for ages.
    acts: u64,
    /// Acts at the last start: a start after an epoch with no acts does not decay.
    #[serde(default)]
    acts_at_start: u64,
    /// Every re-request counted, undecayed, and those of an occurrence held collapsed: the receipt
    /// a replay or a probe compares policies by.
    #[serde(default)]
    total: u64,
    #[serde(default)]
    total_collapsed: u64,
    /// The act at which her view first held each occurrence her sources still offer: the one
    /// clock every age is read from. Stamped where it enters, kept across restarts (a view saved
    /// before births has none, and each occurrence is born when this clock first sees it), and
    /// drained at each start to what the sources still offer.
    #[serde(default)]
    births: BTreeMap<ElementId, u64>,
    #[serde(skip)]
    index: Vec<Held>,
    #[serde(skip)]
    primed: bool,
}

/// An item's identity as an occurrence: only an observing act with a hashed outcome is one.
fn occurrence<T: Block>(item: &Item<T>) -> Option<(Fingerprint, ContentHash)> {
    let act = item.act()?;
    match (act.effect, item.content()) {
        (ToolEffect::Observes, Content::Hashed(hash)) => Some((act.fingerprint, hash)),
        _ => None,
    }
}

fn add(tallies: &mut Curves, source: &str, age: u64, exposure: f64, events: f64) {
    let curve = tallies.entry(source.to_owned()).or_default();
    let at = bucket(age);
    if curve.len() <= at {
        curve.resize(at + 1, Tally::default());
    }
    curve[at].exposure += exposure;
    curve[at].events += events;
}

impl Rereads {
    /// A start: halve what was measured (when she acted since the last start) and index her latest
    /// occurrence of each request among the sources' items, each at its recorded birth.
    pub(crate) fn start<T: Block>(&mut self, sources: &[(&str, &[Item<T>])]) {
        if self.acts > self.acts_at_start {
            for tally in self.any.values_mut().chain(self.collapsed.values_mut()).flatten() {
                tally.exposure /= 2.0;
                tally.events /= 2.0;
            }
        }
        self.acts_at_start = self.acts;
        self.prime(sources);
    }

    fn prime<T: Block>(&mut self, sources: &[(&str, &[Item<T>])]) {
        let mut latest: HashMap<Fingerprint, Held> = HashMap::new();
        let mut births = BTreeMap::new();
        for (source, items) in sources {
            for item in *items {
                // Born when this clock first held it: recorded, or now for what it never saw.
                let born = *births.entry(item.id()).or_insert_with(|| self.births.get(&item.id()).copied().unwrap_or(self.acts)); // unwrap_or: an occurrence this clock never held enters her view at this act
                let Some((fingerprint, content)) = occurrence(item) else { continue };
                let held = Held { source: (*source).to_owned(), key: item.key().to_owned(), fingerprint, content, born };
                if latest.get(&fingerprint).is_none_or(|was| was.born <= born) {
                    latest.insert(fingerprint, held);
                }
            }
        }
        self.index = latest.into_values().collect();
        self.births = births;
        self.primed = true;
    }

    /// One act: every occurrence she still has is exposed at its age (`whole` answers whether her
    /// view holds it whole), then each arriving occurrence is born at this act, and one that asks an
    /// earlier one's request with its outcome is a re-request at that one's age, and becomes her
    /// latest occurrence of it. Every act ticks the clock, the first after a restart too, which
    /// rebuilds the index from the recorded births and counts no re-request.
    pub(crate) fn act<T: Block>(&mut self, sources: &[&dyn Source<T>], whole: impl Fn(&str, &str) -> bool) {
        let items: Vec<(&str, Vec<Item<T>>)> = sources.iter().map(|source| (source.name(), source.items())).collect();
        if !self.primed {
            let views: Vec<(&str, &[Item<T>])> = items.iter().map(|(name, items)| (*name, items.as_slice())).collect();
            self.prime(&views);
            self.acts += 1;
            return;
        }
        for held in &self.index {
            let age = self.acts.saturating_sub(held.born);
            add(&mut self.any, &held.source, age, 1.0, 0.0);
            if !whole(&held.source, &held.key) {
                add(&mut self.collapsed, &held.source, age, 1.0, 0.0);
            }
        }
        for (source, items) in &items {
            for item in items {
                if self.births.contains_key(&item.id()) {
                    continue;
                }
                self.births.insert(item.id(), self.acts);
                let Some((fingerprint, content)) = occurrence(item) else { continue };
                if let Some(at) = self.index.iter().position(|held| held.fingerprint == fingerprint) {
                    let earlier = self.index.swap_remove(at);
                    if earlier.content == content {
                        let age = self.acts.saturating_sub(earlier.born);
                        add(&mut self.any, &earlier.source, age, 0.0, 1.0);
                        self.total += 1;
                        if !whole(&earlier.source, &earlier.key) {
                            add(&mut self.collapsed, &earlier.source, age, 0.0, 1.0);
                            self.total_collapsed += 1;
                        }
                    }
                }
                self.index.push(Held { source: (*source).to_owned(), key: item.key().to_owned(), fingerprint, content, born: self.acts });
            }
        }
        self.acts += 1;
    }

    /// Acts since the last start on her clock: her current epoch's length, what a start closes
    /// (`crate::motion::Motion::close_epoch`).
    pub(crate) fn epoch_acts(&self) -> u64 {
        self.acts.saturating_sub(self.acts_at_start)
    }

    /// How many acts ago her view first held the occurrence `id`: the age its hazard is read at.
    /// An occurrence this clock has not held yet enters her view at this act.
    pub(crate) fn age(&self, id: ElementId) -> u64 {
        self.births.get(&id).map_or(0, |born| self.acts.saturating_sub(*born)) // map_or: one this clock has not held is born now
    }

    /// Her re-request chance at `age`, over every source: the propensity is hers, whichever source
    /// renders the occurrence.
    pub fn hazard(&self, age: u64) -> Hazard {
        let at = bucket(age);
        let tally = self.any.values().filter_map(|curve| curve.get(at)).fold(Tally::default(), |sum, t| Tally { exposure: sum.exposure + t.exposure, events: sum.events + t.events });
        if tally.exposure > 0.0 {
            Hazard::Measured(tally)
        } else {
            Hazard::Unmeasured
        }
    }

    /// Every source's curve, over every occurrence and over the collapsed ones: the receipt.
    pub fn curves(&self) -> (&Curves, &Curves) {
        (&self.any, &self.collapsed)
    }

    /// Every re-request counted, and those of an occurrence she held collapsed, undecayed.
    pub fn totals(&self) -> (u64, u64) {
        (self.total, self.total_collapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cause::{Act, Cause, ContentHash, ElementId, Fingerprint, ToolEffect};

    struct Reads(Vec<Item<String>>);

    impl Source<String> for Reads {
        fn name(&self) -> &str {
            "actions"
        }
        fn priority(&self) -> u8 {
            5
        }
        fn items(&self) -> Vec<Item<String>> {
            self.0.clone()
        }
    }

    fn read(seq: u128, path: &str, body: &str) -> Item<String> {
        let act = Act { fingerprint: Fingerprint::of([("code/read", path.as_bytes())]), effect: ToolEffect::Observes };
        Item::rendered(ElementId::from_u128(seq), Cause::Unrecorded, seq.to_string(), body.to_owned(), Content::Hashed(ContentHash::of(body))).with_act(act)
    }

    // what this catches: a re-read counted at the wrong age or against the wrong level, an edited
    // file's new read counted as a re-read, and the young ages left unmeasured because only
    // collapsed occurrences were exposed (the depth could then never move toward them).
    #[test]
    fn a_re_request_counts_at_the_earlier_ones_age_and_a_changed_outcome_does_not() {
        let mut rereads = Rereads::default();
        let mut items = vec![read(1, "a.rs", "fn a() {}")];
        rereads.start::<String>(&[("actions", &items)]);
        for seq in 2..=4 {
            items.push(read(seq, &format!("{seq}.rs"), "x"));
            rereads.act(&[&Reads(items.clone()) as &dyn Source<String>], |_, key| key != "1");
        }
        // Three acts later she reads a.rs again, unchanged, while it is held collapsed.
        items.push(read(5, "a.rs", "fn a() {}"));
        rereads.act(&[&Reads(items.clone()) as &dyn Source<String>], |_, key| key != "1");
        let (any, collapsed) = rereads.curves();
        assert_eq!(any["actions"][bucket(3)].events, 1.0, "re-requested at age 3");
        assert_eq!(collapsed["actions"][bucket(3)].events, 1.0, "and it was held collapsed");
        assert_eq!(rereads.totals(), (1, 1));
        assert!(matches!(rereads.hazard(0), Hazard::Measured(_)), "the newest age is measured although none collapsed");
        // Edited: the same request with a new outcome is a new observation, not a re-read.
        items.push(read(6, "a.rs", "fn a() { edited }"));
        rereads.act(&[&Reads(items.clone()) as &dyn Source<String>], |_, _| true);
        assert_eq!(rereads.curves().0["actions"].iter().map(|t| t.events).sum::<f64>(), 1.0);
        // A start halves what was measured.
        rereads.start::<String>(&[("actions", &items)]);
        assert_eq!(rereads.curves().0["actions"][bucket(3)].events, 0.5);
    }
    fn acts(rereads: &mut Rereads, items: &[Item<String>], n: usize) {
        for _ in 0..n {
            rereads.act(&[&Reads(items.to_vec()) as &dyn Source<String>], |_, _| true);
        }
    }

    // what this catches (Codex on #5806, his reproduction): two clocks. The depth aged an item by
    // how many newer items its source held while the measurement aged it by acts, so a source that
    // sat idle sampled the wrong hazard bucket: one item, eight acts with nothing new, a second item
    // and one act make the first one 9 acts old on both, never 1.
    #[test]
    fn an_idle_source_ages_its_items_by_acts_on_both_clocks() {
        let mut rereads = Rereads::default();
        let mut items = vec![read(1, "a.rs", "fn a() {}")];
        rereads.start::<String>(&[("actions", &items)]);
        acts(&mut rereads, &items, 8);
        items.push(read(2, "b.rs", "fn b() {}"));
        acts(&mut rereads, &items, 1);
        let depth = crate::depth::collapsible(&items, &rereads, &crate::cost::Chars);
        assert_eq!(depth[0].age, 9, "the depth's age");
        assert_eq!(rereads.age(items[0].id()), 9, "the measurement's age");
        assert_eq!(depth[0].hazard, rereads.hazard(9));
    }

    // what this catches: a batched source (three results in one act) read as three ages, its
    // oldest priced as if it were two acts older than the act that brought all three.
    #[test]
    fn items_that_arrive_in_one_act_share_one_birth() {
        let mut rereads = Rereads::default();
        rereads.start::<String>(&[("actions", &[])]);
        let mut items = vec![read(1, "a.rs", "a"), read(2, "b.rs", "b"), read(3, "c.rs", "c")];
        acts(&mut rereads, &items, 1);
        items.push(read(4, "d.rs", "d"));
        acts(&mut rereads, &items, 1);
        let ages: Vec<u64> = crate::depth::collapsible(&items, &rereads, &crate::cost::Chars).iter().map(|item| item.age).collect();
        assert_eq!(ages, vec![2, 2, 2]);
    }

    // what this catches: a restart re-deriving births from position (born = acts - newer items),
    // so after the idle source above a restored view priced its first item at age 1. Births are
    // kept with her view: the first act after the restart rebuilds the index from them and ticks
    // the clock, and the next exposes the first item at its true age.
    #[test]
    fn a_restart_keeps_true_ages() {
        let mut rereads = Rereads::default();
        let mut items = vec![read(1, "a.rs", "fn a() {}")];
        rereads.start::<String>(&[("actions", &items)]);
        acts(&mut rereads, &items, 8);
        items.push(read(2, "b.rs", "fn b() {}"));
        acts(&mut rereads, &items, 1);
        let saved = serde_json::to_string(&rereads).expect("test: the measurement serializes"); // JUSTIFIED: test fixture
        let mut restored: Rereads = serde_json::from_str(&saved).expect("test: and restores"); // JUSTIFIED: test fixture
        let mut started = restored.clone();
        started.start::<String>(&[("actions", &items)]);
        assert_eq!(crate::depth::collapsible(&items, &started, &crate::cost::Chars)[0].age, 9, "a start after the restart");
        acts(&mut restored, &items, 1);
        assert_eq!(crate::depth::collapsible(&items, &restored, &crate::cost::Chars)[0].age, 10, "the first act after it ticks the clock");
        let before = restored.curves().0["actions"][bucket(10)].exposure;
        acts(&mut restored, &items, 1);
        assert_eq!(restored.curves().0["actions"][bucket(10)].exposure - before, 1.0, "exposed at its true age");
    }
}
