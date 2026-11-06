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
//! now. Tallies survive a restart with her view; the index of occurrences is rebuilt at the first
//! act after it, counting nothing for that act.

use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// The act at which she asked for it.
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
    #[serde(skip)]
    index: Vec<Held>,
    #[serde(skip)]
    seen: HashSet<ElementId>,
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
    /// occurrence of each request among the sources' items, each aged by how many newer items its
    /// source holds.
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
        self.seen.clear();
        for (source, items) in sources {
            for (position, item) in items.iter().enumerate() {
                self.seen.insert(item.id());
                let Some((fingerprint, content)) = occurrence(item) else { continue };
                let newer = (items.len() - 1 - position) as u64;
                let born = self.acts.saturating_sub(newer);
                let held = Held { source: (*source).to_owned(), key: item.key().to_owned(), fingerprint, content, born };
                if latest.get(&fingerprint).is_none_or(|was| was.born <= born) {
                    latest.insert(fingerprint, held);
                }
            }
        }
        self.index = latest.into_values().collect();
        self.primed = true;
    }

    /// One act: every occurrence she still has is exposed at its age (`whole` answers whether her
    /// view holds it whole), then each arriving occurrence that asks an earlier one's request with its
    /// outcome is a re-request at that one's age, and becomes her latest occurrence of it.
    pub(crate) fn act<T: Block>(&mut self, sources: &[&dyn Source<T>], whole: impl Fn(&str, &str) -> bool) {
        let items: Vec<(&str, Vec<Item<T>>)> = sources.iter().map(|source| (source.name(), source.items())).collect();
        if !self.primed {
            let views: Vec<(&str, &[Item<T>])> = items.iter().map(|(name, items)| (*name, items.as_slice())).collect();
            self.prime(&views);
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
                if !self.seen.insert(item.id()) {
                    continue;
                }
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
}
