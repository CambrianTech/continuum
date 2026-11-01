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
use std::path::{Path, PathBuf};

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

/// One persona's outstanding training demand.
pub struct TrainingDemand {
    /// `None` with no continuum home: nothing persists and nothing is outstanding.
    path: Option<PathBuf>,
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
        Self { path: dir.map(|dir| dir.join(format!("{persona}.json"))) }
    }

    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// The stable key of one example: its content, so the same lived turn keys the same after a
    /// return or a re-dispatch.
    pub fn key_of(example: &TrainingExample) -> String {
        let bytes = serde_json::to_vec(example).unwrap_or_default(); // unwrap_or_default: a TrainingExample always serializes; an empty key would only merge two entries
        format!("{:x}", Sha256::digest(&bytes))
    }

    fn read(&self) -> Recorded {
        let Some(path) = &self.path else { return Recorded::default() };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(), // unwrap_or_default: a torn file is re-recorded by the next partition; it never blocks serving
            Err(_) => Recorded::default(),
        }
    }

    /// Record what changed: `parked` examples need these windows; `settled` ones owe nothing more
    /// (a run trained them, or they were withdrawn from her training input: card 4545eafd); every
    /// other entry is kept. `reservation` is what the largest parked window costs, as a
    /// partition's footprints price it.
    pub fn record(&self, parked: &[(String, u32)], settled: &[String], reservation: Option<Reservation>) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let before = self.read();
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
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = serde_json::to_vec(&now).map_err(std::io::Error::other)?; // disk boundary: training-demand/<persona>.json, read back by serving's requirements_for
        continuum_host::durable::replace_with(path, &body)
    }

    /// Settle `keys` strictly: a store that cannot be read or parsed (or no store) is an error,
    /// never an empty one, so a caller that must clear before it commits never commits over a
    /// clear that did not happen (Codex on #5572). A missing file has nothing to clear.
    pub fn clear(&self, keys: &[String]) -> Result<(), String> {
        let Some(path) = &self.path else { return Err("no continuum home: no demand store".into()) };
        let before = self.read_strict()?;
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
        let body = serde_json::to_vec(&now).map_err(|e| e.to_string())?; // disk boundary: training-demand/<persona>.json, read back by serving's requirements_for
        continuum_host::durable::replace_with(path, &body).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The largest window an outstanding example needs. `None` when nothing waits.
    pub fn outstanding(&self) -> Option<u32> {
        self.read().examples.into_values().max()
    }

    /// Whether every one of `keys` is outstanding here: serving has been told what those examples
    /// need. Empty `keys` is false: nothing to wait for is not a recorded demand. `Err` when the
    /// store cannot answer (no store, unreadable, corrupt): never read as "not recorded", since
    /// a caller acting on that would return examples it cannot record (Codex on #5569).
    pub fn records_all(&self, keys: &[String]) -> Result<bool, String> {
        let examples = self.read_strict()?.examples;
        Ok(!keys.is_empty() && keys.iter().all(|key| examples.contains_key(key)))
    }

    /// The store as recorded: a missing file is an empty store; no store, or a file that cannot
    /// be read or parsed, is an error.
    fn read_strict(&self) -> Result<Recorded, String> {
        let Some(path) = &self.path else { return Err("no continuum home: no demand store".into()) };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{}: not a demand record ({e})", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Recorded::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
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
        use crate::inference::planning::training_plan::NeedSource;
        use continuum_engine_protocol::{ExampleFit, MeasuredExample};
        let dir = tempfile::tempdir().expect("test: dir");
        let demand = TrainingDemand::at(dir.path().join("kimi.json"));
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
        assert_eq!(TrainingDemand::at(dir.path().join("kimi.json")).waiting(), 0, "durable: a reload reads what was recorded");
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
}
