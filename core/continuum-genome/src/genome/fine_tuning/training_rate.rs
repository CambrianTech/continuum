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

use continuum_saved_state::{OwnedFile, Rebuildable, Retention, SavedState, Singleton, Store};
use serde::{Deserialize, Serialize};

use super::types::TrainingExample;

/// The longest one training run should take before its gene can be judged and the next run
/// starts. derived-or-floor: a floor on the learning loop's cadence; an hour lets a run land
/// within a working session, and the 5090's measured rate decides how many examples that is.
pub const TRAINING_RUN_BUDGET: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct RateRow {
    secs_per_char_epoch: f64,
    measured_at_ms: u64,
    job: String,
}

/// Measured training rates, one row per (base model, chunk), in one small JSON file beside the
/// footprints (bounded by the bases and the handful of chunks ever trained, not by runs). Keyed by
/// chunk because a step's cost per token depends on it: a chunk-64 step is several times slower
/// per token than a chunk-256 one (Fable on #4939), and the trigger sizes a run before the adapter
/// has chosen its chunk.
pub struct TrainingRates {
    store: Store<Rates>,
}

/// The rates, saved through the one saved-state contract (`<genome root>/training-rate.json`).
struct Rates;
impl SavedState for Rates {
    const KIND: &'static str = "training-rate";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one file per node, one row per (base model, chunk) ever trained" };
    type Key = Singleton;
    type Contract = ();
    type Value = BTreeMap<String, RateRow>;
    type Medium = OwnedFile;
}
/// A missing rate means the next run calibrates at the bucket's threshold.
impl Rebuildable for Rates {}

impl TrainingRates {
    pub(crate) fn in_dir(dir: PathBuf) -> Self {
        Self { store: Store::at(dir) }
    }

    /// The store in the continuum home's genome root, `None` when the home cannot be resolved.
    pub fn in_home() -> Option<Self> {
        super::job_dir::genome_root().map(Self::in_dir)
    }

    fn read_all(&self) -> BTreeMap<String, RateRow> {
        self.store.load_rebuildable(&Singleton, &())
    }

    /// Wall seconds per character per epoch for `base`: the SLOWEST measured at any chunk (each as
    /// last measured). The trigger sizes a run before its chunk is chosen, so it budgets at the
    /// slowest the base has run: a run then ends within the budget whatever chunk it gets (the safe
    /// direction, as an over-counted character is). A row from before the chunk key counts too.
    pub fn secs_per_char_epoch(&self, base: &str) -> Option<f64> {
        let chunked = format!("{base}|c");
        self.read_all()
            .iter()
            .filter(|(key, _)| key.as_str() == base || key.starts_with(&chunked))
            .map(|(_, r)| r.secs_per_char_epoch)
            .filter(|r| r.is_finite() && *r > 0.0)
            .reduce(f64::max)
    }

    /// Record a finished run at `chunk`: `chars` of example payload trained for `epochs` in
    /// `wall_ms`. The LATEST run's rate at that chunk stands (the node, the model and serving's
    /// share all drift).
    pub(crate) fn record(&self, base: &str, chunk: u32, chars: u64, epochs: u32, wall_ms: u64, job: uuid::Uuid) -> std::io::Result<()> {
        if chars == 0 || epochs == 0 || wall_ms == 0 {
            return Ok(());
        }
        let (mut all, generation) = self.store.rebuildable_at_generation(&Singleton, &());
        // a rate from before the chunk key is superseded by the first chunked one: left in place it
        // would cap every budget at an old, unkeyed rate forever (Cormac on #4939)
        all.remove(base);
        all.insert(
            format!("{base}|c{chunk}"),
            RateRow {
                secs_per_char_epoch: (wall_ms as f64 / 1000.0) / (chars as f64 * f64::from(epochs)),
                measured_at_ms: continuum_base::time::EpochMs::now().millis(),
                job: job.to_string(),
            },
        );
        Ok(self.store.save_over(&Singleton, &(), &all, generation).map(|_saved| ())?)
    }
}

/// The epochs a run trains when its request names no schedule: the ONE default
/// (`native_jobs::default_schedule`), never a second copy of its number.
pub fn default_epochs() -> u32 {
    super::native_jobs::default_schedule().epochs
}

/// The characters of one example's payload: the length of its serialized form, the same count
/// at measurement and at estimate, so the two can never disagree about what a character is.
/// Counted by streaming the encoding into a byte counter: nothing is allocated or kept, so
/// sizing a 311-conversation batch costs no copy of it.
pub fn example_chars(example: &TrainingExample) -> u64 {
    struct Count(u64);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    match serde_json::to_writer(&mut count, example) {
        Ok(()) => count.0,
        Err(_) => 0, // an example that cannot be encoded cannot be sent either: it costs nothing to plan for
    }
}

/// How many of the OLDEST submissions a run should take, in arrival order: their estimated wall
/// time at `rate` stays within `budget`, and at least one is always taken (a single submission
/// over budget still trains, alone). `None` = take them all. Oldest first is the DRAIN: under
/// steady inflow every submission is eventually taken, where newest-first would starve the
/// oldest forever and grow the pending bucket without bound (Cormac on #4926).
/// `chars_per_submission` is oldest first.
pub fn oldest_within_budget(chars_per_submission: &[u64], secs_per_char_epoch: f64, epochs: u32, budget: Duration) -> Option<usize> {
    let per_char = secs_per_char_epoch * f64::from(epochs.max(1));
    let total: f64 = chars_per_submission.iter().map(|&c| c as f64 * per_char).sum();
    if total <= budget.as_secs_f64() {
        return None;
    }
    let mut spent = 0.0;
    let mut take = 0;
    for &chars in chars_per_submission {
        let cost = chars as f64 * per_char;
        if take > 0 && spent + cost > budget.as_secs_f64() {
            break;
        }
        spent += cost;
        take += 1;
    }
    Some(take)
}

/// The directory a test can point the store at.
#[cfg(test)]
pub(crate) fn rates_at(dir: &std::path::Path) -> TrainingRates {
    TrainingRates::in_dir(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-10-10): a bucket that filled all day dispatched as ONE
    // ~100-hour run. A run takes the newest submissions that fit the budget at the measured rate,
    // never zero, and all of them when they fit; and the rate is what a finished run measured.
    // what this catches (Fable on #4939): one rate per base, so the first chunk-64 run was budgeted
    // at a chunk-256 rate (several times too fast) and overran its hour. Rates are kept per chunk,
    // and the trigger, which sizes a run before its chunk is chosen, budgets at the slowest.
    #[test]
    fn a_run_is_budgeted_at_the_slowest_chunk_the_base_has_run() {
        let dir = tempfile::tempdir().expect("test: dir");
        let rates = rates_at(dir.path());
        rates.record("m", 256, 1_000, 1, 1_000, uuid::Uuid::nil()).expect("test: c256");
        rates.record("m", 64, 1_000, 1, 4_000, uuid::Uuid::nil()).expect("test: c64");
        rates.record("other", 64, 1_000, 1, 9_000, uuid::Uuid::nil()).expect("test: other base");
        assert_eq!(rates.secs_per_char_epoch("m"), Some(0.004), "the chunk-64 rate, the slower of m's two");
        rates.record("m", 64, 1_000, 1, 2_000, uuid::Uuid::nil()).expect("test: c64 again");
        assert_eq!(rates.secs_per_char_epoch("m"), Some(0.002), "the latest at each chunk stands");
        // a row from before the chunk key is read until the first chunked record supersedes it
        let legacy = rates_at(&dir.path().join("legacy"));
        let unkeyed = RateRow { secs_per_char_epoch: 9.0, measured_at_ms: 0, job: "old".into() };
        legacy.store.save(&Singleton, &(), &BTreeMap::from([("m".to_string(), unkeyed)])).expect("test: legacy");
        assert_eq!(legacy.secs_per_char_epoch("m"), Some(9.0), "a pre-chunk rate still counts");
        legacy.record("m", 256, 1_000, 1, 1_000, uuid::Uuid::nil()).expect("test: chunked");
        assert_eq!(legacy.secs_per_char_epoch("m"), Some(0.001), "and is gone once a chunked rate is recorded");
    }

    #[test]
    fn a_run_takes_the_oldest_submissions_that_fit_its_budget() {
        let hour = Duration::from_secs(3600);
        // 1 s per char per epoch x 3 epochs: each 1000-char submission costs 3000 s
        assert_eq!(oldest_within_budget(&[1000, 1000, 1000], 1.0, 3, hour), Some(1), "only the oldest fits an hour");
        assert_eq!(oldest_within_budget(&[100, 100, 100], 1.0, 3, hour), None, "900 s: all of them fit");
        assert_eq!(oldest_within_budget(&[10_000], 1.0, 3, hour), Some(1), "one over budget still trains, alone");
        let dir = tempfile::tempdir().expect("test: dir");
        let rates = rates_at(dir.path());
        assert_eq!(rates.secs_per_char_epoch("m"), None, "unmeasured: none");
        rates.record("m", 256, 1_000, 2, 4_000, uuid::Uuid::nil()).expect("test: record");
        assert_eq!(rates.secs_per_char_epoch("m"), Some(0.002), "4 s over 1000 chars x 2 epochs");
        let ex = TrainingExample { prompt: "p".into(), completion: "a reply".into(), metadata: None, lived: None };
        let mut encoded = Vec::new();
        serde_json::to_writer(&mut encoded, &ex).expect("test: encodes");
        assert_eq!(example_chars(&ex), encoded.len() as u64, "the counted length is the encoded length");
    }

    // what this catches: a corrupt, newer or pre-envelope rate file silently read as "no rate"
    // with nothing said, or a save that does not come back: every outcome of the saved-state
    // contract, for this kind.
    #[test]
    fn the_rates_round_trip_and_any_other_file_is_a_typed_outcome() {
        let dir = tempfile::tempdir().expect("test: dir");
        let row = RateRow { secs_per_char_epoch: 0.002, measured_at_ms: 5, job: "j".into() };
        continuum_saved_state::round_trip::every_outcome::<Rates>(dir.path(), Singleton, (), None, BTreeMap::from([("m|c256".to_string(), row)]));
    }
}
