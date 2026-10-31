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
//! identically (`p`, the chance the prefix continues past it) and how many leading characters
//! it kept (`kept`, what it saves even when it changed: a growing log keeps its old front).
//! Placing a source with an intact prefix saves its `kept` and passes the prefix on with
//! chance `p`, so an exchange argument orders sources by `kept / (1 - p)`, largest first; a
//! source that never changed is infinite and leads. A source not yet measured counts as
//! changing every start and keeping nothing: it ties with measured sources that keep nothing,
//! and ties keep the caller's order.

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

    /// `sources` in the order that keeps the most of a turnover's prefix: by `kept / (1 - p)`,
    /// largest first, ties and unmeasured sources in the caller's order.
    pub(crate) fn order<'a, T: Block>(
        &self,
        sources: &[&'a dyn Source<T>],
    ) -> Vec<&'a dyn Source<T>> {
        let mut ordered = sources.to_vec();
        // As a fraction kept_chars / (compared - unchanged); a source that never changed has a
        // zero denominator and leads. Compared by cross-multiplying, so no float rounds a tie.
        let score = |source: &&dyn Source<T>| -> (u64, u64) {
            match self.of(source.name()) {
                None => (0, 1),
                Some(m) => (m.kept_chars, u64::from(m.compared - m.unchanged)),
            }
        };
        ordered.sort_by(|a, b| {
            let ((ka, da), (kb, db)) = (score(a), score(b));
            match (da, db) {
                (0, 0) => std::cmp::Ordering::Equal,
                (0, _) => std::cmp::Ordering::Less,
                (_, 0) => std::cmp::Ordering::Greater,
                _ => (u128::from(kb) * u128::from(da)).cmp(&(u128::from(ka) * u128::from(db))),
            }
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

    // what this catches: a hand label instead of the measurement. A roster that changes at most
    // starts must ride behind a log whose front holds, and a block that never changes leads
    // (Aiko, the M5, 2026-10-25: the roster led and 5 of 12 turnovers kept 3-6%).
    #[test]
    fn sources_order_by_what_a_turnover_keeps() {
        let mut motion = Motion::default();
        let starts: [(&str, &[&str]); 3] =
            [("room-a", &["a"]), ("room-b", &["b"]), ("room-a", &["a"])];
        for (n, (_, roster)) in starts.iter().enumerate() {
            motion.record("roster", items(roster));
            let log: Vec<&str> = ["one", "two", "three", "four"][..n + 2].to_vec();
            motion.record("actions", items(&log));
            motion.record("doctrine", items(&["rules"]));
        }
        let (roster, actions, doctrine, fresh) = (
            Named("roster"),
            Named("actions"),
            Named("doctrine"),
            Named("fresh"),
        );
        let caller: Vec<&dyn Source<String>> = vec![&fresh, &roster, &actions, &doctrine];
        let names: Vec<&str> = motion.order(&caller).iter().map(|s| s.name()).collect();
        assert_eq!(names, ["doctrine", "actions", "fresh", "roster"]);
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
