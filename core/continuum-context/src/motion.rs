//! How each source of her view moves between epoch starts, measured from the starts themselves.
//!
//! A turnover re-renders every source, and the engine reuses the previous start's prompt up
//! to the first block that differs. So the order an epoch start renders its sources in decides
//! what a turnover costs, and the order that keeps the most is a measurement, not a label:
//! a hand table once called the roster "board churn" and led with it, while a resident acting
//! in three rooms changed it at most starts (Aiko on the M5, 2026-10-25: 5 of 12 turnovers
//! broke at the roster, 3-6% of the prompt kept).
//!
//! For each source the view records, at every start, the fingerprints of the items it
//! rendered, and compares them with the previous start's (did it render identically, how many
//! leading characters it kept), and at every act whether the source appended a new version (its
//! change rate over her last epoch). A start orders sources by the exchange argument over the
//! prefix it shares with the epoch before (`Motion::rank`): a unit of `s` characters that differs
//! at the next start with chance `q`, keeping its first `k` characters when it does, goes ahead of
//! another when `s (1 - q) / q + k` is larger. `q` blends what her starts measured with her
//! per-act rate over her measured epoch length (`1 - (1 - p)^E`); a unit that never changed
//! leads; one never measured (Unmeasured) goes to the tail; two units whose `q` are within their
//! measurement error keep the order the last start laid them in (hysteresis), so the order never
//! flaps. A source that turns volatile drifts tailward on its own, one that settles drifts back.
//! Recomputed only at a start: within an epoch the view only appends. Measured on the M5's
//! captures (2026-11-06): a per-act reorder broke each mind's own append order (85% -> 54% token
//! hits); cross-mind sharing was ~2% of fresh tokens, so it carries no weight here.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::epoch::Block;

/// One source's movement across the starts this view has seen.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Track {
    /// Starts compared with the one before them.
    compared: u32,
    /// Compared starts at which the source rendered exactly as before.
    unchanged: u32,
    /// Leading characters each compared start kept from the one before, summed.
    kept_chars: u64,
    /// The last start's items: fingerprint and characters, in render order; `None` before the
    /// source's first start.
    last: Option<Vec<(u64, usize)>>,
    /// Acts of the current epoch that saw the source, and those at which it appended a new
    /// version. Absent in a view saved before acts were counted.
    #[serde(default)]
    acts: u32,
    #[serde(default)]
    changed: u32,
    /// The same two counts over her last closed epoch: the rate a start ranks by, decayed to her
    /// recent behaviour (the epoch before replaces it).
    #[serde(default)]
    last_acts: u32,
    #[serde(default)]
    last_changed: u32,
}

/// Every source's [`Track`], kept across turnovers and restarts with her view.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Motion {
    sources: BTreeMap<String, Track>,
    /// The order the last start laid its sources down in (first block of each): what a tie keeps.
    /// Absent in a view saved before it, which breaks ties by the caller's order.
    #[serde(default)]
    last_order: Vec<String>,
    /// Acts in the current epoch, and in her last closed one: her measured epoch length.
    #[serde(default)]
    epoch_acts: u32,
    #[serde(default)]
    last_epoch_acts: u32,
    /// How often she asks again for what she observed, by its age: what a start's collapse depth
    /// weighs (`crate::rereads`, `crate::depth`). Absent in a view saved before it, which starts
    /// measuring at its next act.
    #[serde(default)]
    rereads: crate::rereads::Rereads,
}

/// One unit a start places: a source, or her collapsed log.
#[derive(Clone, Debug)]
pub(crate) struct Unit<'a> {
    pub name: &'a str,
    /// What it renders at this start, in characters.
    pub chars: usize,
    /// Her collapsed log: it grows at every start (q = 1) and keeps all it held (k = its size).
    pub log: bool,
    /// The source has items in her collapsed log: it never goes ahead of it, or her newer items
    /// would stand before her older ones.
    pub after_log: bool,
}

/// One source's measured movement, for a probe or a caller's policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measured {
    pub compared: u32,
    pub unchanged: u32,
    pub kept_chars: u64,
}

/// A rendered item's identity within a start: its key and what it rendered as. `DefaultHasher`
/// is fixed-keyed, so a restored view compares with what it saved; a toolchain that changes the
/// hash costs one mismatched comparison, never a wrong order for longer.
pub(crate) fn fingerprint<T: Block>(key: &str, rendered: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    rendered.hash(&mut hasher);
    hasher.finish()
}

impl Motion {
    /// Record what a start rendered for `source`: its items' fingerprints and characters.
    pub(crate) fn record(&mut self, source: &str, rendered: Vec<(u64, usize)>) {
        let track = self.sources.entry(source.to_owned()).or_default();
        if let Some(last) = &track.last {
            let kept = last.iter().zip(&rendered).take_while(|(a, b)| a.0 == b.0);
            track.kept_chars += kept.map(|(_, b)| b.1 as u64).sum::<u64>();
            track.unchanged += u32::from(*last == rendered);
            track.compared += 1;
        }
        track.last = Some(rendered);
    }

    pub fn of(&self, source: &str) -> Option<Measured> {
        self.sources
            .get(source)
            .filter(|t| t.compared > 0)
            .map(|t| Measured {
                compared: t.compared,
                unchanged: t.unchanged,
                kept_chars: t.kept_chars,
            })
    }

    /// One act saw `source`; `changed` when it appended a new version.
    pub(crate) fn act_saw(&mut self, source: &str, changed: bool) {
        let track = self.sources.entry(source.to_owned()).or_default();
        track.acts += 1;
        track.changed += u32::from(changed);
    }

    /// One act of her epoch.
    pub(crate) fn act(&mut self) {
        self.epoch_acts += 1;
    }

    /// A start closes her epoch: its counts become the rates the next start ranks by. An epoch
    /// with no acts keeps the rates before it.
    pub(crate) fn close_epoch(&mut self) {
        for track in self.sources.values_mut() {
            if track.acts > 0 {
                (track.last_acts, track.last_changed) = (track.acts, track.changed);
            }
            (track.acts, track.changed) = (0, 0);
        }
        if self.epoch_acts > 0 {
            self.last_epoch_acts = self.epoch_acts;
        }
        self.epoch_acts = 0;
    }

    /// `q`, the chance `source` differs at the next start, and how many observations measured it;
    /// `None` when nothing has (Unmeasured).
    fn change(&self, source: &str) -> Option<(f64, u32)> {
        let track = self.sources.get(source)?;
        let (acts, changed) = if track.last_acts > 0 { (track.last_acts, track.last_changed) } else { (track.acts, track.changed) };
        if acts == 0 && track.compared == 0 {
            return None;
        }
        // Her per-act rate over her measured epoch length, counted as one observation beside what
        // her starts measured directly.
        let epoch = f64::from(self.last_epoch_acts.max(self.epoch_acts).max(1));
        let per_act = if acts == 0 { 0.0 } else { f64::from(changed) / f64::from(acts) };
        let from_acts = 1.0 - (1.0 - per_act).powf(epoch);
        let at_starts = f64::from(track.compared - track.unchanged);
        let observed = if acts == 0 { 0 } else { 1 } + track.compared;
        let q = (at_starts + if acts == 0 { 0.0 } else { from_acts }) / f64::from(observed.max(1));
        Some((q, acts + track.compared))
    }

    /// The order a start lays `units` down in, as indices: by the exchange argument over the
    /// prefix the next start shares (see the module doc), Unmeasured units last, a unit with
    /// collapsed history after the log, and two units whose change rates are within their
    /// measurement error in the order the last start laid them in.
    pub(crate) fn rank(&self, units: &[Unit<'_>]) -> Vec<usize> {
        #[derive(Clone, Copy, PartialEq, PartialOrd)]
        enum Key {
            Unmeasured,
            Value(f64),
            Never,
        }
        let measured: Vec<Option<(f64, u32)>> = units.iter().map(|unit| if unit.log { Some((1.0, u32::MAX)) } else { self.change(unit.name) }).collect();
        let key = |index: usize| -> Key {
            let unit = &units[index];
            match measured[index] {
                None => Key::Unmeasured,
                Some(_) if unit.log => Key::Value(unit.chars as f64),
                Some((q, _)) if q <= 0.0 => Key::Never,
                Some((q, _)) => {
                    let kept = self.sources.get(unit.name).filter(|t| t.compared > 0).map_or(0.0, |t| t.kept_chars as f64 / f64::from(t.compared)); // map_or: a source no start compared kept nothing measured
                    Key::Value(unit.chars as f64 * (1.0 - q) / q + kept)
                }
            }
        };
        let place = |index: usize| self.last_order.iter().position(|name| name == units[index].name).unwrap_or(usize::MAX); // unwrap_or: a unit the last start did not lay down ties after those it did
        // One standard error of a rate measured over `n` observations (at least the error of a
        // single coin flip at that count): the band inside which two units are not told apart.
        let error = |q: f64, n: u32| ((q * (1.0 - q)).max(0.25 / f64::from(n.max(1))) / f64::from(n.max(1))).sqrt();
        let apart = |a: usize, b: usize| match (measured[a], measured[b]) {
            (Some((qa, na)), Some((qb, nb))) if !units[a].log && !units[b].log => (qa - qb).abs() > error(qa, na) + error(qb, nb),
            _ => true,
        };
        let mut ranked: Vec<usize> = (0..units.len()).collect();
        ranked.sort_by(|&a, &b| key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| place(a).cmp(&place(b)))); // unwrap_or: a NaN value cannot arise from finite counts; equal keeps the last start's order
        // Hysteresis: a unit moves ahead of one the last start laid before it only when their rates
        // are apart.
        let mut order: Vec<usize> = Vec::with_capacity(ranked.len());
        for index in ranked {
            let mut at = order.len();
            while at > 0 && !apart(order[at - 1], index) && place(order[at - 1]) > place(index) {
                at -= 1;
            }
            order.insert(at, index);
        }
        // Her older collapsed items never stand after her newer whole ones of the same source.
        if let Some(log) = order.iter().position(|&index| units[index].log) {
            let early: Vec<usize> = order[..log].iter().copied().filter(|&index| units[index].after_log).collect();
            if !early.is_empty() {
                order.retain(|index| !early.contains(index));
                let log = order.iter().position(|&index| units[index].log).unwrap_or(0); // unwrap_or: the log is still in the order it was found in
                for (offset, index) in early.into_iter().enumerate() {
                    order.insert(log + 1 + offset, index);
                }
            }
        }
        order
    }

    /// Her measured re-requests by age.
    pub fn rereads(&self) -> &crate::rereads::Rereads {
        &self.rereads
    }

    pub(crate) fn rereads_mut(&mut self) -> &mut crate::rereads::Rereads {
        &mut self.rereads
    }

    /// The order a start laid its sources down in: what the next start's ties keep.
    pub(crate) fn started_in(&mut self, order: Vec<String>) {
        self.last_order = order;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(texts: &[&str]) -> Vec<(u64, usize)> {
        texts
            .iter()
            .map(|t| (fingerprint("k", &t.to_string()), t.len()))
            .collect()
    }

    fn unit(name: &str, chars: usize) -> Unit<'_> {
        Unit { name, chars, log: false, after_log: false }
    }

    fn names<'a>(units: &'a [Unit<'a>], order: &[usize]) -> Vec<&'a str> {
        order.iter().map(|&index| units[index].name).collect()
    }

    // what this catches: a hand label instead of the measurement, and an order that flaps. A unit
    // that never changed leads; then by what a start keeps (s(1-q)/q + k); a unit never measured
    // goes last, never a guessed rank; two units within their measurement error keep the order the
    // last start laid them in (the M5, 2026-11-06: `[my work]`/`[room-kanban]` and notices/`[tasks]`
    // traded places between turnovers, and the prefix ended at each swap).
    #[test]
    fn units_rank_by_what_a_start_keeps_and_ties_keep_the_last_order() {
        let mut motion = Motion::default();
        for act in 0..20 {
            motion.act();
            motion.act_saw("doctrine", false);
            motion.act_saw("wall", act % 10 == 0);
            motion.act_saw("kanban", act % 2 == 0);
            motion.act_saw("tasks", act % 2 == 1);
        }
        motion.close_epoch();
        motion.started_in(["tasks", "kanban", "wall", "doctrine"].map(String::from).to_vec());
        let units = [unit("fresh", 900), unit("kanban", 3_000), unit("tasks", 3_000), unit("wall", 3_000), unit("doctrine", 100)];
        let order = motion.rank(&units);
        assert_eq!(names(&units, &order), ["doctrine", "wall", "tasks", "kanban", "fresh"]);
    }

    // what this catches: her collapsed log ranked like a source that changes, so it never leads,
    // or a source with logged history laid ahead of the log (her newer items before her older).
    #[test]
    fn the_log_stands_by_what_it_keeps_and_its_sources_follow_it() {
        let mut motion = Motion::default();
        for _ in 0..10 {
            motion.act();
            motion.act_saw("thoughts", true);
            motion.act_saw("roster", false);
        }
        motion.close_epoch();
        let units = [
            Unit { name: "thoughts", chars: 9_000, log: false, after_log: true },
            unit("roster", 500),
            Unit { name: "(collapsed log)", chars: 20_000, log: true, after_log: false },
        ];
        let order = motion.rank(&units);
        assert_eq!(names(&units, &order), ["roster", "(collapsed log)", "thoughts"]);
    }

    // what this catches: a view restored after a restart losing what it measured.
    #[test]
    fn motion_survives_serialization() -> Result<(), serde_json::Error> {
        let mut motion = Motion::default();
        motion.record("roster", items(&["a"]));
        motion.record("roster", items(&["a"]));
        let restored: Motion = serde_json::from_str(&serde_json::to_string(&motion)?)?;
        assert_eq!(
            restored.of("roster"),
            Some(Measured {
                compared: 1,
                unchanged: 1,
                kept_chars: 1
            })
        );
        Ok(())
    }
}
