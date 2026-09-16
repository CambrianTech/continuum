//! The DECODE KNEE — lanes stop where the measured per-stream decode rate stops holding.
//!
//! Joel, 2026-09-16: "tok/sec is everything"; the latency law: a 1.5× tax is fine, 5–10×
//! is disqualifying. Measured that night on the M5 (Ornith-1.5-35B, Metal): 2 lanes in
//! flight decoded ~20 t/s per stream, 8 in flight 6–10 t/s against a catalog 68 — a
//! ten-times tax, `serving.throughput.degraded` ×68 an hour, sixteen minds each waiting
//! two hundred seconds for a turn. The planner grew lanes with the ROSTER (one warm slot
//! per resident) and never asked the decode curve, because nothing recorded it.
//!
//! This module records it. Every measured decode reports `(in-flight model calls,
//! measured/expected ratio)`; per model the ledger keeps an EMA of the ratio at each
//! concurrency. The KNEE is the largest concurrency whose ratio still clears
//! [`DECODE_TAX_FLOOR`]; the lane demand is clamped to it. With the KV pages surviving
//! relaunches (#4069) and restoring from disk in ~0.4 s, fewer warm slots plus paging IS
//! the design — the restore economy — not a loss of minds.
//!
//! Shape: a pure rule ([`knee_lanes`]) over a small ledger, a durable record per model
//! (`state/decode-knee.json`) so a reboot starts AT the knee instead of re-climbing to
//! the roster and re-measuring the collapse, and one probe when the clamp binds. No env
//! var, no config: the knee is a measured fact.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// A lane count "holds" while the per-stream ratio to the single-stream catalog rate stays
/// at or above this: a 4× tax at worst, inside the law's "disqualifying" line. Below it,
/// one more lane makes every stream slower than the minds it was added for.
pub const DECODE_TAX_FLOOR: f64 = 0.25;
/// Samples at a concurrency before its EMA is trusted — one decode is noise, a handful
/// is a curve.
pub const MIN_SAMPLES: u32 = 4;
const EMA_ALPHA: f64 = 0.2;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CurvePoint {
    pub samples: u32,
    pub ratio_ema: f64,
}

/// The measured decode curve of one model: concurrency → per-stream ratio.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DecodeCurve {
    pub points: BTreeMap<u32, CurvePoint>,
}

impl DecodeCurve {
    pub fn observe(&mut self, inflight: u32, ratio: f64) {
        if inflight == 0 || !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        let p = self.points.entry(inflight).or_default();
        p.ratio_ema = if p.samples == 0 {
            ratio
        } else {
            p.ratio_ema + EMA_ALPHA * (ratio - p.ratio_ema)
        };
        p.samples = p.samples.saturating_add(1);
    }

    /// The knee under `floor`: the LARGEST measured concurrency whose ratio holds.
    /// If every measured concurrency has collapsed, one below the smallest collapsed
    /// one (never below 1) — shrink and re-measure. `None` = nothing trusted yet
    /// (no clamp; the roster rules as before).
    pub fn knee(&self, floor: f64) -> Option<u32> {
        let trusted: Vec<(&u32, &CurvePoint)> =
            self.points.iter().filter(|(_, p)| p.samples >= MIN_SAMPLES).collect();
        if trusted.is_empty() {
            return None;
        }
        if let Some((n, _)) = trusted.iter().rev().find(|(_, p)| p.ratio_ema >= floor) {
            return Some(**n);
        }
        trusted.first().map(|(n, _)| (**n).saturating_sub(1).max(1))
    }
}

/// The pure rule the planner applies: the roster's demand, clamped to the knee when one
/// is known. A knee never RAISES demand.
pub fn knee_lanes(demand: u32, knee: Option<u32>) -> u32 {
    match knee {
        Some(k) => demand.min(k.max(1)),
        None => demand,
    }
}

static CURVES: LazyLock<parking_lot::Mutex<BTreeMap<String, DecodeCurve>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));

/// Record one measured decode for `model` (the adapter's seam, every decode — not only
/// the collapsed ones). Persists the knee when it changes.
pub fn observe(model: &str, inflight: u32, measured_tps: f64, expected_tps: f64) {
    if expected_tps <= 0.0 || measured_tps <= 0.0 {
        return;
    }
    let ratio = measured_tps / expected_tps;
    let mut curves = CURVES.lock();
    let curve = curves.entry(model.to_string()).or_default();
    let before = curve.knee(DECODE_TAX_FLOOR);
    curve.observe(inflight, ratio);
    let after = curve.knee(DECODE_TAX_FLOOR);
    if before != after {
        crate::probe!(
            class = "serving.decode_knee.moved",
            model = model,
            from = before.map(|k| k as u64).unwrap_or(0),
            to = after.map(|k| k as u64).unwrap_or(0),
            inflight = inflight as u64,
            ratio,
            "the measured decode knee moved — lanes follow it, not the roster"
        );
        save_all(&curves);
    }
}

/// The knee for `model`, if measured.
pub fn knee_for(model: &str) -> Option<u32> {
    CURVES.lock().get(model).and_then(|c| c.knee(DECODE_TAX_FLOOR))
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| h.join("state").join("decode-knee.json"))
}

fn load() -> BTreeMap<String, DecodeCurve> {
    default_path().map(|p| load_from(&p)).unwrap_or_default() // unwrap_or_default: no home = nothing remembered; the roster rules until measured
}

pub fn load_from(path: &Path) -> BTreeMap<String, DecodeCurve> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(), // unwrap_or_default: a corrupt record is forgotten, never trusted — the curve re-measures
        Err(_) => BTreeMap::new(),
    }
}

fn save_all(curves: &BTreeMap<String, DecodeCurve>) {
    if let Some(p) = default_path() {
        save_to(&p, curves);
    }
}

pub fn save_to(path: &Path, curves: &BTreeMap<String, DecodeCurve>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(curves) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-16, the M5 at 8 lanes / 7 t/s): the knee is the largest
    // concurrency that still clears the tax floor; a roster of 16 clamps to it; when every
    // measured concurrency has collapsed the rule shrinks below the smallest one instead
    // of standing still; nothing trusted = no clamp; a knee never raises demand.
    #[test]
    fn lanes_stop_at_the_largest_concurrency_that_still_holds() {
        let mut c = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES {
            c.observe(2, 0.30); // 20 of 68 t/s
            c.observe(4, 0.22); // 15 of 68
            c.observe(8, 0.10); // 7 of 68
        }
        assert_eq!(c.knee(DECODE_TAX_FLOOR), Some(2));
        assert_eq!(knee_lanes(16, c.knee(DECODE_TAX_FLOOR)), 2, "the roster's 16 clamps to the knee");
        assert_eq!(knee_lanes(1, Some(4)), 1, "a knee never raises demand");
        // Only collapsed concurrencies measured → one below the smallest, never zero.
        let mut worst = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES {
            worst.observe(6, 0.12);
            worst.observe(8, 0.09);
        }
        assert_eq!(worst.knee(DECODE_TAX_FLOOR), Some(5));
        let mut floor = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES { floor.observe(1, 0.05); }
        assert_eq!(floor.knee(DECODE_TAX_FLOOR), Some(1));
        // Untrusted (too few samples) = no clamp.
        let mut thin = DecodeCurve::default();
        thin.observe(8, 0.1);
        assert_eq!(thin.knee(DECODE_TAX_FLOOR), None);
        assert_eq!(knee_lanes(16, None), 16);
    }

    // what this catches: the record survives a reboot and a corrupt file is forgotten.
    #[test]
    fn the_knee_is_remembered_across_a_reboot_and_a_corrupt_record_is_forgotten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("decode-knee.json");
        let mut curves = BTreeMap::new();
        let mut c = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES { c.observe(3, 0.4); }
        curves.insert("m".to_string(), c);
        save_to(&p, &curves);
        assert_eq!(load_from(&p), curves);
        std::fs::write(&p, b"{nope").expect("write");
        assert!(load_from(&p).is_empty());
    }
}
