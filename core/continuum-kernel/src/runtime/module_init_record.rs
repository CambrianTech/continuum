//! The slowest healthy module initialize per boot, folded into an estimate on disk: what the next
//! boot's [`InitDeadline`] is derived from (`boot.module_init` measured it every boot; nothing
//! kept it).
//!
//! The estimator is RFC 6298's retransmission timeout, the standard answer to "how long to wait
//! for something whose duration is measured each time": a smoothed duration plus four times its
//! mean deviation. It follows the node both ways: a slow boot raises the bound, steady boots
//! shrink the deviation term. An init that never returns never writes: a wedge is not a sample.
//!
//! One saved-state file per node (`state/module-init.json`).

use std::path::Path;

use continuum_saved_state::{OwnedFile, Retention, SavedState, Singleton, Store};
use serde::{Deserialize, Serialize};

use super::module_readiness::InitDeadline;

/// The slowest initialize that returned Ok on one boot: one sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowestHealthyInit {
    pub module: String,
    pub ms: u64,
}

/// RFC 6298 §2 gains: the smoothed value moves 1/8 of the way to each sample, the deviation 1/4,
/// and the bound is the smoothed value plus 4 deviations. A first sample R starts at
/// smoothed = R, deviation = R/2 (bound 3R).
const RFC6298_ALPHA: f64 = 1.0 / 8.0;
const RFC6298_BETA: f64 = 1.0 / 4.0;
const RFC6298_K: f64 = 4.0;

/// The estimate the record keeps: smoothed slowest healthy init, its mean deviation, and the
/// boot's slowest module for the operator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitEstimate {
    pub smoothed_ms: f64,
    pub deviation_ms: f64,
    pub last: SlowestHealthyInit,
}

impl InitEstimate {
    fn first(sample: SlowestHealthyInit) -> Self {
        let r = sample.ms as f64;
        Self { smoothed_ms: r, deviation_ms: r / 2.0, last: sample }
    }

    fn update(&self, sample: SlowestHealthyInit) -> Self {
        let r = sample.ms as f64;
        let deviation_ms = (1.0 - RFC6298_BETA) * self.deviation_ms + RFC6298_BETA * (self.smoothed_ms - r).abs();
        let smoothed_ms = (1.0 - RFC6298_ALPHA) * self.smoothed_ms + RFC6298_ALPHA * r;
        Self { smoothed_ms, deviation_ms, last: sample }
    }

    fn deadline(&self) -> InitDeadline {
        InitDeadline::Measured {
            ms: (self.smoothed_ms + RFC6298_K * self.deviation_ms).ceil() as u64,
            smoothed_ms: self.smoothed_ms.round() as u64,
            deviation_ms: self.deviation_ms.round() as u64,
            slowest_module: self.last.module.clone(),
        }
    }
}

struct ModuleInitRecord;
impl SavedState for ModuleInitRecord {
    const KIND: &'static str = "module-init";
    const FORMAT: u32 = 1;
    const RETENTION: Retention =
        Retention::PerKey { bound: "one file per node, rewritten by every boot" };
    type Key = Singleton;
    type Contract = ();
    type Value = InitEstimate;
    type Medium = OwnedFile;
}

/// The record as a value the boot holds: built under the continuum home in production and under
/// a tempdir in a test, never resolved from a global at the moment of the write. It keeps the
/// estimate it loaded, so every save in one boot folds that boot's sample into the SAME prior.
#[derive(Clone)]
pub struct InitRecordStore {
    store: Store<ModuleInitRecord>,
    prior: Option<InitEstimate>,
}

impl InitRecordStore {
    pub fn under_home(home: &Path) -> Self {
        let store = Store::at(home.join("state"));
        // Anything but a matched record is no prior (the saved-state trunk probed which).
        let prior = store.load(&Singleton, &()).matched().ok();
        Self { store, prior }
    }

    /// The deadline this boot uses: from the estimate, or the typed backstop when this node has
    /// no record yet.
    pub fn deadline(&self) -> InitDeadline {
        self.prior.as_ref().map_or_else(InitDeadline::unmeasured, InitEstimate::deadline)
    }

    /// Fold this boot's slowest healthy init into the prior and write it. Called again when a
    /// degraded module finishes later with a slower init; it re-folds into the same prior, so a
    /// boot counts once. A failed save is probed by the trunk and costs only the next boot's
    /// bound.
    pub fn save(&self, slowest: &SlowestHealthyInit) {
        let estimate = match &self.prior {
            Some(prior) => prior.update(slowest.clone()),
            None => InitEstimate::first(slowest.clone()),
        };
        if self.store.save(&Singleton, &(), &estimate).is_ok() {
            continuum_base::probe!(
                class = "boot.module_init.recorded",
                module = %slowest.module,
                ms = slowest.ms,
                next_deadline = %estimate.deadline(),
                "slowest healthy init this boot folded into the record; the next boot's init deadline"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot(dir: &Path, ms: u64) -> InitDeadline {
        InitRecordStore::under_home(dir).save(&SlowestHealthyInit { module: "serving-daemon".into(), ms });
        InitRecordStore::under_home(dir).deadline()
    }

    // what this catches: the deadline drifting back to a constant. Each boot's sample must move the
    // next boot's bound (RFC 6298 over the M5's recorded serving-daemon inits: 1,495 / 1,537 /
    // 1,843 ms), steady boots must LOWER it, a second save in one boot must not count twice, and a
    // missing or corrupt record must be the typed Unmeasured backstop, never a guessed number.
    #[test]
    fn each_boot_moves_the_next_deadline_both_ways_and_none_is_typed() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(InitRecordStore::under_home(dir.path()).deadline(), InitDeadline::unmeasured());

        assert_eq!(boot(dir.path(), 1495).duration().as_millis(), 4485, "first sample: 3R");
        assert_eq!(boot(dir.path(), 1537).duration().as_millis(), 3785);
        let after_slow = boot(dir.path(), 1843).duration().as_millis();
        assert_eq!(after_slow, 3600);
        let mut steady = after_slow;
        for _ in 0..8 {
            steady = boot(dir.path(), 1843).duration().as_millis();
        }
        assert!(steady < after_slow, "steady boots shrink the deviation term: {steady} vs {after_slow}");
        assert!(steady > 1843, "and never below the samples themselves: {steady}");

        // Two saves in one boot fold into the same prior: the second replaces the first.
        let store = InitRecordStore::under_home(dir.path());
        store.save(&SlowestHealthyInit { module: "a".into(), ms: 1843 });
        let once = InitRecordStore::under_home(dir.path()).deadline();
        store.save(&SlowestHealthyInit { module: "a".into(), ms: 1843 });
        assert_eq!(InitRecordStore::under_home(dir.path()).deadline(), once);

        std::fs::write(dir.path().join("state/module-init.json"), b"{ not json").expect("corrupt");
        assert_eq!(InitRecordStore::under_home(dir.path()).deadline(), InitDeadline::unmeasured());
    }
}
