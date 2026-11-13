//! A resident's OUTSTANDING TRAINING DEMAND: the window her lived examples, parked because her
//! served window cannot train them whole, would need, and what that training costs (card
//! ea157b1a, gate 3).
//!
//! The windows 5090, 2026-10-28: Kimi's reviewed correction (11 lived turns, 51,670-64,041
//! tokens) parked for good at a served window of 49,664, because the training planner never
//! trains past what serving holds and nothing told serving what her learning needs. Each
//! partition records here, per example, the window that trains it whole, and the reservation
//! for the largest: the training graph's bytes there by the training plan's own footprint
//! arithmetic, labeled (a whole run, a lower bound, a prediction) and bound to the model and
//! training shape it was measured for, or unknown. The serving requirement carries it beside her
//! conversation ([`TrainingReserve`]); an unknown price grows nothing (Codex on ea157b1a).
//!
//! An entry clears only when a run TRAINS its example: at the run's successful finish, the
//! examples its partition sent minus any the run itself skipped ([`TrainingDemand::trained`]).
//! Being sent clears nothing: a sent run that fails, is refused or is cancelled leaves her
//! demand as it was, so her lane never shrinks under examples that still wait (Codex on #5557).
//!
//! Durable, per persona, keyed by each example's content (it survives a return and a
//! re-dispatch unchanged): `<genome root>/training-demand/<persona>.json`.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};

use continuum_saved_state::{Loaded, OwnedFile, Rebuildable, Retention, SaveError, SavedState, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::types::TrainingExample;
use crate::inference::planning::training_plan::Partition;
use crate::inference::planning::window_allocator::{TrainingPrice, TrainingReserve};

/// What the largest outstanding window costs to train, as a partition priced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
    pub window: u32,
    /// `None`: no footprint of that shape is known yet.
    pub price: Option<TrainingPrice>,
    pub job: Uuid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recorded {
    /// Each parked example's whole window, by the example's content key.
    examples: BTreeMap<String, u32>,
    /// The reservation for the largest window, from the partition that last measured it.
    reservation: Option<Reservation>,
}

/// One persona's outstanding demand, saved through the one saved-state contract
/// (`<genome root>/training-demand/<persona>.json`).
struct Demand;
impl SavedState for Demand {
    const KIND: &'static str = "training-demand";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one file per persona with parked examples" };
    type Key = Uuid;
    type Contract = ();
    type Value = Recorded;
    type Medium = OwnedFile;
    /// The file before the envelope was this value: adopted once, rewritten by the next save.
    fn upgrade(found: u32, old: serde_json::Value) -> Option<Self::Value> {
        continuum_saved_state::adopt_bare(found, old)
    }
}
/// Rebuildable for the lenient readers: the next partition re-records a demand it finds missing.
/// The strict readers ([`TrainingDemand::clear`], [`TrainingDemand::records_all`]) match the verdict.
impl Rebuildable for Demand {}

/// Why the demand store could not answer or keep a change.
#[derive(Debug)]
pub enum DemandError {
    /// The store holds no readable record (no home, corrupt, another format).
    Unanswered(Loaded<Infallible>),
    /// The changed record was not saved.
    Unsaved(SaveError),
}

impl std::fmt::Display for DemandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DemandError::Unanswered(found) => write!(f, "the demand store cannot answer: {found}"),
            DemandError::Unsaved(e) => write!(f, "the demand change was not saved: {e}"),
        }
    }
}

impl std::error::Error for DemandError {}

/// One persona's outstanding training demand.
pub struct TrainingDemand {
    /// Unrooted with no continuum home: nothing persists and nothing is outstanding.
    store: Store<Demand>,
    persona: Uuid,
}

impl TrainingDemand {
    /// Where a genome root keeps every persona's demand.
    pub fn store_dir(genome: &Path) -> PathBuf {
        genome.join("training-demand")
    }

    /// `persona`'s demand under the node's genome root: what serving reads.
    pub fn for_persona(persona: Uuid) -> Self {
        Self::in_dir(super::job_dir::genome_root().map(|root| Self::store_dir(&root)).as_deref(), persona)
    }

    /// `persona`'s demand in `dir` (a tuner's own store, a test's temp dir); `None`: nothing persists.
    pub fn in_dir(dir: Option<&Path>, persona: Uuid) -> Self {
        Self { store: Store::new(dir.map(Path::to_path_buf)), persona }
    }

    /// The stable key of one example: its content, so the same lived turn keys the same after a
    /// return or a re-dispatch.
    pub fn key_of(example: &TrainingExample) -> String {
        let bytes = serde_json::to_vec(example).unwrap_or_default(); // unwrap_or_default: a TrainingExample always serializes; an empty key would only merge two entries
        format!("{:x}", Sha256::digest(&bytes))
    }

    fn read(&self) -> Recorded {
        self.store.load_rebuildable(&self.persona, &())
    }

    /// Record what changed: `parked` examples need these windows; `settled` ones owe nothing more
    /// (a run trained them, or they were withdrawn from her training input: card 4545eafd); every
    /// other entry is kept. `reservation` is what the largest parked window costs, as a
    /// partition's footprints price it.
    pub fn record(&self, parked: &[(String, u32)], settled: &[String], reservation: Option<Reservation>) -> std::io::Result<()> {
        if self.store.dir().is_none() {
            return Ok(()); // no store: nothing persists, and nothing is outstanding to change
        }
        let (before, generation) = self.store.rebuildable_at_generation(&self.persona, &());
        let mut now = before.clone();
        for key in settled {
            now.examples.remove(key);
        }
        for (key, window) in parked {
            now.examples.insert(key.clone(), *window);
        }
        if now.examples.is_empty() {
            now.reservation = None;
        } else if reservation.is_some() {
            now.reservation = reservation;
        }
        if now == before {
            return Ok(());
        }
        Ok(self.store.save_over(&self.persona, &(), &now, generation).map(|_saved| ())?)
    }

    /// Settle `keys` strictly: a store that cannot be read or parsed (or no store) is an error,
    /// never an empty one, so a caller that must clear before it commits never commits over a
    /// clear that did not happen (Codex on #5572). A missing file has nothing to clear.
    pub fn clear(&self, keys: &[String]) -> Result<(), DemandError> {
        let (before, generation) = self.read_strict().map_err(DemandError::Unanswered)?;
        let mut now = before.clone();
        for key in keys {
            now.examples.remove(key);
        }
        if now.examples.is_empty() {
            now.reservation = None;
        }
        if now == before {
            return Ok(());
        }
        self.store.save_over(&self.persona, &(), &now, generation).map(|_saved| ()).map_err(DemandError::Unsaved)
    }

    /// The largest window an outstanding example needs. `None` when nothing waits.
    pub fn outstanding(&self) -> Option<u32> {
        self.read().examples.into_values().max()
    }

    /// Whether every one of `keys` is outstanding here: serving has been told what those examples
    /// need. Empty `keys` is false: nothing to wait for is not a recorded demand. `Err` when the
    /// store cannot answer (no store, unreadable, corrupt): never read as "not recorded", since
    /// a caller acting on that would return examples it cannot record (Codex on #5569).
    pub fn records_all(&self, keys: &[String]) -> Result<bool, Loaded<Infallible>> {
        let examples = self.read_strict()?.0.examples;
        Ok(!keys.is_empty() && keys.iter().all(|key| examples.contains_key(key)))
    }

    /// The store as recorded: a missing file is an empty store; every other verdict (no store, a
    /// file that cannot be read or parsed, another format) is an error.
    fn read_strict(&self) -> Result<(Recorded, u64), Loaded<Infallible>> {
        let (loaded, generation) = self.store.load_at_generation(&self.persona, &());
        match loaded.matched() {
            Ok(recorded) => Ok((recorded, generation)),
            Err(Loaded::Absent) => Ok((Recorded::default(), generation)),
            Err(found) => Err(found),
        }
    }

    /// How many examples wait.
    pub fn waiting(&self) -> usize {
        self.read().examples.len()
    }

    /// Her reservation for the serving requirement: the largest outstanding window, with the
    /// bytes a partition measured for EXACTLY that window, else unknown (never priced from
    /// another window). `None` when nothing waits.
    pub fn reservation(&self) -> Option<TrainingReserve> {
        let recorded = self.read();
        let window = recorded.examples.values().copied().max()?;
        match recorded.reservation.filter(|r| r.window == window) {
            Some(r) => Some(TrainingReserve::new(window, r.price, Some(r.job))),
            None => Some(TrainingReserve::new(window, None, None)),
        }
    }

    /// The keys a finished run TRAINED: what its partition sent, minus what the run itself
    /// skipped (amended into `parked` by `with_run_skipped`). An index outside `examples` is not
    /// one of this request's and is ignored.
    pub fn trained(examples: &[TrainingExample], partition: &Partition) -> Vec<String> {
        partition
            .sent
            .iter()
            .filter(|i| !partition.parked.contains(i))
            .filter_map(|&i| examples.get(i))
            .map(Self::key_of)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card ea157b1a; Codex on #5557): resolving one parked example dropping
    // every outstanding demand; an example SENT to a run that then fails clearing her demand, so
    // her lane shrinks under examples that still wait; an example a run skipped counted as
    // trained; and a reservation priced for one window read as the price of another.
    #[test]
    fn only_a_trained_example_clears_and_a_price_holds_only_for_its_window() {
        use crate::inference::planning::training_plan::{ExampleFit, MeasuredExample, NeedSource};
        let dir = tempfile::tempdir().expect("test: dir");
        let kimi = Uuid::from_u128(0x41);
        let demand = TrainingDemand::in_dir(Some(dir.path()), kimi);
        assert_eq!(demand.reservation(), None, "nothing parked, nothing reserved");
        let ex = |c: &str| TrainingExample { prompt: "p".into(), completion: c.into(), metadata: None, lived: None };
        let (a, b, c) = (ex("a"), ex("b"), ex("c"));
        let key = TrainingDemand::key_of;
        let job = Uuid::from_u128(0x1e02);
        let price = TrainingPrice { bytes: 6_000_000_000, source: NeedSource::LowerBound, model: "kimi-base".into(), shape: "kimi-base|w64256".into() };
        let priced = Reservation { window: 64_256, price: Some(price.clone()), job };
        demand.record(&[(key(&a), 51_712), (key(&b), 64_256), (key(&c), 40_192)], &[], Some(priced)).expect("test: record");
        assert_eq!((demand.outstanding(), demand.waiting()), (Some(64_256), 3));
        assert_eq!(demand.reservation(), Some(TrainingReserve::new(64_256, Some(price), Some(job))));
        // b and c are sent to a run; it fails: no finish, nothing cleared
        assert_eq!(demand.waiting(), 3, "sent is not trained: a failed or cancelled run leaves her demand");
        // the retry trains b, and the run itself skips c: only b clears
        let row = |index: usize| MeasuredExample { index, tokens_full: 1, tokens_min: 1, tokens_trained: 1, fit: ExampleFit::Kept };
        let ran = Partition { window: 65_536, sent: vec![1, 2], parked: vec![0, 2], rows: vec![row(0), row(1), row(2)] };
        demand.record(&[], &TrainingDemand::trained(&[a.clone(), b, c], &ran), None).expect("test: b trained");
        assert_eq!((demand.outstanding(), demand.waiting()), (Some(51_712), 2), "a and the run-skipped c still wait");
        let left = demand.reservation().expect("a still reserves");
        assert_eq!((left.window, left.recorded_price(), left.job), (51_712, None, None), "64,256's price is never read as 51,712's: unknown until priced");
        demand.record(&[], &[key(&a), key(&ex("c"))], None).expect("test: a and c trained");
        assert_eq!(demand.reservation(), None);
        assert_eq!(TrainingDemand::in_dir(Some(dir.path()), kimi).waiting(), 0, "durable: a reload reads what was recorded");
    }

    // what this catches: a lived turn keyed by something a return or a re-dispatch changes, so
    // its demand can never clear.
    #[test]
    fn an_example_keys_by_its_content() {
        let example = TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None };
        assert_eq!(TrainingDemand::key_of(&example), TrainingDemand::key_of(&example.clone()));
        let other = TrainingExample { completion: "d".into(), ..example.clone() };
        assert_ne!(TrainingDemand::key_of(&example), TrainingDemand::key_of(&other));
    }

    // what this catches: a torn, newer or pre-envelope demand file read as an empty demand by the
    // strict readers (a clear that never happened committed over), or a save that does not come
    // back: every outcome of the saved-state contract, for this kind.
    #[test]
    fn the_demand_round_trips_and_any_other_file_is_a_typed_outcome() {
        let dir = tempfile::tempdir().expect("test: dir");
        let recorded = Recorded { examples: BTreeMap::from([("k".to_string(), 64_256)]), reservation: None };
        continuum_saved_state::round_trip::every_outcome::<Demand>(dir.path(), Uuid::from_u128(0x41), (), None, recorded);
        let torn = TrainingDemand::in_dir(Some(dir.path()), Uuid::from_u128(0x42));
        std::fs::write(dir.path().join(format!("{}.json", Uuid::from_u128(0x42))), b"{ torn").expect("test: torn");
        assert!(matches!(torn.records_all(&["k".to_string()]), Err(Loaded::Corrupt { .. })), "a strict reader refuses a torn store");
        assert_eq!(torn.waiting(), 0, "a lenient reader starts empty; the next partition re-records");
    }
}
