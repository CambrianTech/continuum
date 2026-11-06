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
//! rendered, and compares them with the previous start's: whether the source rendered
//! identically (`p`, how often its content held from one start to the next) and how many
//! leading characters it kept. Sources order by `p`, most stable first, and a tie keeps the
//! order the previous start laid them in, then the caller's. A source not yet measured counts
//! as changing every start.
//!
//! Before 2026-11-06 the order was `kept / (1 - p)`, an exchange argument over the characters a
//! changed source still keeps at its front. On the M5 that ratio moved with every start's sizes,
//! so adjacent blocks swapped places between starts (`[my work]` and `[room-kanban]`, the notices
//! and `[tasks]`) and each swap ended the shared prefix where it happened; and on the hybrid
//! model a changed source's kept front is reused only up to a checkpoint. A stable order, with
//! the start reusing every unchanged block the previous epoch led with
//! (`view::lead_with_what_held`), keeps more.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::epoch::Block;
use crate::source::Source;

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
}

/// Every source's [`Track`], kept across turnovers and restarts with her view.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Motion {
    sources: BTreeMap<String, Track>,
    /// The order the last start laid its sources down in (first block of each): what a tie keeps.
    /// Absent in a view saved before it, which breaks ties by the caller's order.
    #[serde(default)]
    last_order: Vec<String>,
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

    /// The order a start laid its sources down in: what the next start's ties keep.
    pub(crate) fn started_in(&mut self, order: Vec<String>) {
        self.last_order = order;
    }

    /// `sources` most stable first (`p`, how often each rendered as at the start before), a tie in
    /// the order the last start laid them in, then in the caller's.
    pub(crate) fn order<'a, T: Block>(
        &self,
        sources: &[&'a dyn Source<T>],
    ) -> Vec<&'a dyn Source<T>> {
        let mut ordered = sources.to_vec();
        // As the fraction unchanged / compared, compared by cross-multiplying so no float rounds a
        // tie; a source not yet measured is 0 of 1.
        let stability = |source: &&dyn Source<T>| -> (u64, u64) {
            self.of(source.name()).map_or((0, 1), |m| (u64::from(m.unchanged), u64::from(m.compared))) // map_or: unmeasured counts as changing every start
        };
        let place = |source: &&dyn Source<T>| self.last_order.iter().position(|name| name == source.name()).unwrap_or(usize::MAX); // unwrap_or: a source the last start did not lay down ties after those it did
        ordered.sort_by(|a, b| {
            let ((ua, ca), (ub, cb)) = (stability(a), stability(b));
            (u128::from(ub) * u128::from(ca)).cmp(&(u128::from(ua) * u128::from(cb))).then_with(|| place(a).cmp(&place(b)))
        });
        ordered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Item;

    struct Named(&'static str);
    impl Source<String> for Named {
        fn name(&self) -> &str {
            self.0
        }
        fn priority(&self) -> u8 {
            0
        }
        fn items(&self) -> Vec<Item<String>> {
            Vec::new()
        }
    }

    fn items(texts: &[&str]) -> Vec<(u64, usize)> {
        texts
            .iter()
            .map(|t| (fingerprint("k", &t.to_string()), t.len()))
            .collect()
    }

    // what this catches: a hand label instead of the measurement, and an order that swaps
    // neighbours between starts. A block that never changes leads, one that held half the time
    // follows, and the ones that change every start keep the order the last start laid them in,
    // never re-ranked by their sizes (the M5, 2026-11-06: `[my work]`/`[room-kanban]` and the
    // notices/`[tasks]` traded places between turnovers, and the prefix ended at each swap).
    #[test]
    fn sources_order_most_stable_first_and_ties_keep_the_last_starts_order() {
        let mut motion = Motion::default();
        let rosters: [&[&str]; 3] = [&["a"], &["a"], &["b"]];
        for (n, roster) in rosters.iter().enumerate() {
            motion.record("roster", items(roster));
            let log: Vec<&str> = ["one", "two", "three", "four"][..n + 2].to_vec();
            motion.record("actions", items(&log));
            motion.record("doctrine", items(&["rules"]));
            motion.record("tasks", items(&[["t1", "t2", "t3"][n]]));
        }
        motion.started_in(["doctrine", "tasks", "roster", "actions"].map(String::from).to_vec());
        let (roster, actions, doctrine, tasks, fresh) = (Named("roster"), Named("actions"), Named("doctrine"), Named("tasks"), Named("fresh"));
        let caller: Vec<&dyn Source<String>> = vec![&fresh, &roster, &actions, &tasks, &doctrine];
        let names: Vec<&str> = motion.order(&caller).iter().map(|s| s.name()).collect();
        assert_eq!(names, ["doctrine", "roster", "tasks", "actions", "fresh"]);
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
