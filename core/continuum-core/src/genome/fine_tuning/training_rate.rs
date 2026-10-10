//! What training costs, measured per base model: wall seconds per character of example payload
//! per epoch, from each finished run's own wall clock. The trigger sizes a dispatch by it, so a
//! run trains what fits [`TRAINING_RUN_BUDGET`] and the rest stays pending for the next run.
//!
//! Why a time budget: measured on the 5090 2026-10-10, a bucket blocked all day reached 311 of
//! Kimi's lived conversations (1,601,530 trainable tokens) while her runs trained ~12.8 trainable
//! tokens a second beside serving, ~100 hours for 3 epochs. Joel: training is continuous while
//! inferencing, or on/off fast: small frequent runs, never one that outlives the day.
//!
//! The cost is per CHARACTER of the serialized example, the whole payload the engine reads: it
//! is what the core can count before dispatch (no tokenizer here), and the wall clock it is
//! divided by includes context decode, training and the time serving took back, which is the
//! cost a budget is about. An example longer than the window (fit:"middle" trims its oldest
//! history) is over-counted, so a run takes fewer examples than would fit: the safe direction.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::types::TrainingExample;

/// The longest one training run should take before its gene can be judged and the next run
/// starts. derived-or-floor: a floor on the learning loop's cadence; an hour lets a run land
/// within a working session, and the 5090's measured rate decides how many examples that is.
pub(crate) const TRAINING_RUN_BUDGET: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct RateRow {
    secs_per_char_epoch: f64,
    measured_at_ms: i64,
    job: String,
}

/// Measured training rates, one row per base model, in one small JSON file beside the
/// footprints (bounded by the number of base models ever trained, not by runs).
pub(crate) struct TrainingRates {
    path: PathBuf,
}

impl TrainingRates {
    pub(crate) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// The store in the continuum home, `None` when the home cannot be resolved.
    pub(crate) fn in_home() -> Option<Self> {
        crate::paths::continuum_home()
            .ok()
            .map(|home| Self::at(home.join("genome").join("engine-rates.json")))
    }

    fn read_all(&self) -> BTreeMap<String, RateRow> {
        match std::fs::read_to_string(&self.path) {
            Ok(body) => serde_json::from_str(&body).unwrap_or_default(), // unwrap_or_default: a corrupt file means no measured rate, so the next run calibrates at the bucket's threshold
            Err(_) => BTreeMap::new(),
        }
    }

    /// Wall seconds per character per epoch for `base`, as last measured.
    pub(crate) fn secs_per_char_epoch(&self, base: &str) -> Option<f64> {
        self.read_all().get(base).map(|r| r.secs_per_char_epoch).filter(|r| r.is_finite() && *r > 0.0)
    }

    /// Record a finished run: `chars` of example payload trained for `epochs` in `wall_ms`.
    /// The LATEST run's rate stands (the node, the model and serving's share all drift).
    pub(crate) fn record(&self, base: &str, chars: u64, epochs: u32, wall_ms: u64, job: uuid::Uuid) -> std::io::Result<()> {
        if chars == 0 || epochs == 0 || wall_ms == 0 {
            return Ok(());
        }
        let mut all = self.read_all();
        all.insert(
            base.to_string(),
            RateRow {
                secs_per_char_epoch: (wall_ms as f64 / 1000.0) / (chars as f64 * f64::from(epochs)),
                measured_at_ms: chrono::Utc::now().timestamp_millis(),
                job: job.to_string(),
            },
        );
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&all).map_err(std::io::Error::other)?)?;
        crate::utils::file_replace::replace_file(&tmp, &self.path)
    }
}

/// The characters of one example's payload: its serialized form, the same count at measurement
/// and at estimate, so the two can never disagree about what a character is.
pub(crate) fn example_chars(example: &TrainingExample) -> u64 {
    serde_json::to_string(example).map_or(0, |s| s.len() as u64)
}

/// How many of the NEWEST submissions a run should take: their estimated wall time at `rate`
/// stays within `budget`, and at least one is always taken (a single submission over budget
/// still trains, alone). `None` = take them all (they fit, or there is no rate and the caller
/// calibrates). `chars_per_submission` is oldest first.
pub(crate) fn newest_within_budget(chars_per_submission: &[u64], secs_per_char_epoch: f64, epochs: u32, budget: Duration) -> Option<usize> {
    let per_char = secs_per_char_epoch * f64::from(epochs.max(1));
    let total: f64 = chars_per_submission.iter().map(|&c| c as f64 * per_char).sum();
    if total <= budget.as_secs_f64() {
        return None;
    }
    let mut spent = 0.0;
    let mut take = 0;
    for &chars in chars_per_submission.iter().rev() {
        let cost = chars as f64 * per_char;
        if take > 0 && spent + cost > budget.as_secs_f64() {
            break;
        }
        spent += cost;
        take += 1;
    }
    Some(take)
}

/// The path a test can point the store at.
#[cfg(test)]
pub(crate) fn rates_at(path: &std::path::Path) -> TrainingRates {
    TrainingRates::at(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-10-10): a bucket that filled all day dispatched as ONE
    // ~100-hour run. A run takes the newest submissions that fit the budget at the measured rate,
    // never zero, and all of them when they fit; and the rate is what a finished run measured.
    #[test]
    fn a_run_takes_the_newest_submissions_that_fit_its_budget() {
        let hour = Duration::from_secs(3600);
        // 1 s per char per epoch x 3 epochs: each 1000-char submission costs 3000 s
        assert_eq!(newest_within_budget(&[1000, 1000, 1000], 1.0, 3, hour), Some(1), "only the newest fits an hour");
        assert_eq!(newest_within_budget(&[100, 100, 100], 1.0, 3, hour), None, "900 s: all of them fit");
        assert_eq!(newest_within_budget(&[10_000], 1.0, 3, hour), Some(1), "one over budget still trains, alone");
        let dir = tempfile::tempdir().expect("test: dir");
        let rates = rates_at(&dir.path().join("rates.json"));
        assert_eq!(rates.secs_per_char_epoch("m"), None, "unmeasured: none");
        rates.record("m", 1_000, 2, 4_000, uuid::Uuid::nil()).expect("test: record");
        assert_eq!(rates.secs_per_char_epoch("m"), Some(0.002), "4 s over 1000 chars x 2 epochs");
    }
}
