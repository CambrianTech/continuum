//! A node's measured SERVING RATES per model (card df58b8b8): the server-total prefill rate
//! and the clean per-stream decode rate at each concurrency, both read off `/slots` by the
//! prefill knee ([`crate::inference::prefill_knee`]) and kept per model, so the rate bar
//! ([`crate::cognition::service_rate`]) can judge a model this node is not serving right now.
//!
//! Neither rate comes from a generation's own timings: those count a stream's stalls behind
//! its neighbours' prefill as decode, and read the in-flight count at the stream's end
//! (Cormac on #4489: the M5's 27B decodes ~27 t/s alone, its timings-built curve read 6.7).
//! One record per model in `state/serving-rates.json`, bounded by the models on the node.

use crate::cognition::service_rate::ModelRates;
use crate::inference::decode_knee::DecodeCurve;
use crate::inference::prefill_rate::PrefillPoint;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// One model's measured rates.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ServingRates {
    /// Server-total prefill tok/s over busy windows.
    #[serde(default)]
    pub prefill: PrefillPoint,
    /// Clean per-stream decode tok/s by streams in flight.
    #[serde(default)]
    pub decode: DecodeCurve,
}

impl ServingRates {
    /// The rates as the rule reads them: every point with lifetime evidence, fresh or stale
    /// (a node's hardware does not drift overnight), nothing below it.
    pub fn as_model_rates(&self) -> ModelRates {
        let min = crate::inference::decode_knee::MIN_SAMPLES;
        ModelRates {
            prefill_tps: (self.prefill.samples >= crate::inference::prefill_rate::MIN_SAMPLES && self.prefill.tps_ema > 0.0)
                .then_some(self.prefill.tps_ema),
            decode_per_stream: self
                .decode
                .points
                .iter()
                .filter(|(_, p)| p.samples >= min && p.tps_ema > 0.0)
                .map(|(n, p)| (*n, p.tps_ema))
                .collect(),
        }
    }
}

static RATES: LazyLock<parking_lot::Mutex<BTreeMap<String, ServingRates>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));

/// A server-total prefill rate measured over one busy window for `model`.
pub fn observe_prefill(model: &str, tps: f64, now_ms: u64) {
    let snapshot = {
        let mut rates = RATES.lock();
        rates.entry(model.to_string()).or_default().prefill.observe(tps, now_ms);
        rates.clone()
    };
    save_all(&snapshot);
}

/// A clean per-stream decode rate for `model` with `streams` in flight.
pub fn observe_decode(model: &str, streams: u32, tps: f64, now_ms: u64) {
    let snapshot = {
        let mut rates = RATES.lock();
        rates.entry(model.to_string()).or_default().decode.observe(streams, tps, now_ms);
        rates.clone()
    };
    crate::probe!(
        class = "serving.rates.clean_decode",
        model = model,
        streams = u64::from(streams),
        tps,
        "a clean decode interval: the same tasks decoding at both reads, none prefilling"
    );
    // written outside the lock (Cormac on #4489); reads are serialized by the knee's
    // in-flight flag, so one writer at a time
    save_all(&snapshot);
}

/// The measured rates of `model` on this node; empty (every verdict unmeasured) when none.
pub fn rates_for(model: &str) -> ModelRates {
    RATES.lock().get(model).map(ServingRates::as_model_rates).unwrap_or_default() // unwrap_or_default: an unmeasured model has no rates, and the rule reads that as unmeasured
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| h.join("state").join("serving-rates.json"))
}

fn load() -> BTreeMap<String, ServingRates> {
    default_path().map(|p| load_from(&p)).unwrap_or_default() // unwrap_or_default: no home = nothing remembered; every model re-measures
}

pub fn load_from(path: &Path) -> BTreeMap<String, ServingRates> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(), // unwrap_or_default: a corrupt record is forgotten, never trusted
        Err(_) => BTreeMap::new(),
    }
}

fn save_all(rates: &BTreeMap<String, ServingRates>) {
    let Some(path) = default_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(rates) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a thin or zero point read as a rate. The rule sees a prefill rate
    // only after MIN_SAMPLES windows and a decode point only after MIN_SAMPLES clean
    // intervals, stale or fresh; below that the model is unmeasured, never slow.
    #[test]
    fn the_rule_reads_only_points_with_enough_evidence() {
        let mut r = ServingRates::default();
        r.prefill.observe(300.0, 1);
        r.decode.observe(2, 25.0, 1);
        let thin = r.as_model_rates();
        assert_eq!((thin.prefill_tps, thin.decode_per_stream.len()), (None, 0));
        for t in 2..=6 {
            r.prefill.observe(300.0, t);
            r.decode.observe(2, 25.0, t);
        }
        let read = r.as_model_rates();
        assert_eq!(read.prefill_tps, Some(300.0));
        assert_eq!(read.decode_per_stream.get(&2), Some(&25.0));
    }
}
