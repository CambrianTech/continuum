//! A resident's OUTSTANDING TRAINING DEMAND: the window her lived examples, parked because her
//! served window cannot train them whole, would need, and what that training costs (card
//! ea157b1a, gate 3).
//!
//! The windows 5090, 2026-10-28: Kimi's reviewed correction (11 lived turns, 51,670-64,041
//! tokens) parked for good at a served window of 49,664, because the training planner never
//! trains past what serving holds and nothing told serving what her learning needs. Each
//! partition records here, per example, the window that trains it whole, and the reservation
//! for the largest: the training graph's bytes there by the training plan's own footprint
//! arithmetic, labeled (a whole run, a lower bound, a prediction), or unknown. The serving
//! requirement carries it ([`TrainingReserve`]): a measured one raises her target and is kept
//! out of the allocator's growth; an unknown one grows nothing (Codex on ea157b1a). An entry
//! clears when its example is sent whole by a later partition; the others stay.
//!
//! Durable, per persona, keyed by each example's content (it survives a return and a
//! re-dispatch unchanged): `<genome root>/training-demand/<persona>.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::types::TrainingExample;
use crate::inference::planning::training_plan::NeedSource;
use crate::inference::planning::window_allocator::TrainingReserve;

/// What the largest outstanding window costs to train, as a partition measured it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
    pub window: u32,
    pub bytes: Option<u64>,
    pub source: Option<NeedSource>,
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

    /// Record what one partition learned: `parked` examples need these windows, `sent` ones
    /// trained whole and owe nothing, every other entry is kept; `reservation` is what the
    /// largest parked window costs, as this partition's footprints price it.
    pub fn record(&self, parked: &[(String, u32)], sent: &[String], reservation: Option<Reservation>) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let before = self.read();
        let mut now = before.clone();
        for key in sent {
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
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&now).map_err(std::io::Error::other)?)?;
        continuum_host::durable::replace(&tmp, path)
    }

    /// The largest window an outstanding example needs. `None` when nothing waits.
    pub fn outstanding(&self) -> Option<u32> {
        self.read().examples.into_values().max()
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
        let measured = recorded.reservation.filter(|r| r.window == window);
        Some(TrainingReserve {
            window,
            bytes: measured.and_then(|r| r.bytes),
            source: measured.and_then(|r| r.source),
            job: measured.map_or(Uuid::nil(), |r| r.job), // map_or: no partition priced this window yet: no job measured it
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card ea157b1a, Codex's ask): resolving one parked example dropping every
    // outstanding demand, so her lane shrinks while other lived turns still cannot train; an example
    // that trained whole still holding her target up; and a reservation priced for one window
    // read as the price of another.
    #[test]
    fn resolving_one_example_keeps_the_others_and_a_reservation_prices_only_its_window() {
        let dir = tempfile::tempdir().expect("test: dir");
        let demand = TrainingDemand::at(dir.path().join("kimi.json"));
        assert_eq!(demand.reservation(), None, "nothing parked, nothing reserved");
        let job = Uuid::from_u128(0x1e02);
        let priced = Reservation { window: 64_256, bytes: Some(6_000_000_000), source: Some(NeedSource::LowerBound), job };
        demand.record(&[("a".into(), 51_712), ("b".into(), 64_256)], &[], Some(priced)).expect("test: record");
        assert_eq!((demand.outstanding(), demand.waiting()), (Some(64_256), 2));
        assert_eq!(demand.reservation(), Some(TrainingReserve { window: 64_256, bytes: Some(6_000_000_000), source: Some(NeedSource::LowerBound), job }));
        demand.record(&[], &["b".into()], None).expect("test: b trained whole");
        assert_eq!((demand.outstanding(), demand.waiting()), (Some(51_712), 1), "a still waits");
        let left = demand.reservation().expect("a still reserves");
        assert_eq!((left.window, left.bytes, left.source), (51_712, None, None), "64,256's price is never read as 51,712's: unknown until measured");
        demand.record(&[], &["a".into()], None).expect("test: a trained whole");
        assert_eq!(demand.reservation(), None);
        let again = TrainingDemand::at(dir.path().join("kimi.json"));
        assert_eq!(again.waiting(), 0, "durable: a reload reads what was recorded");
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
