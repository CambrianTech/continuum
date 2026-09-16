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
//!
//! Three things the first live deploy taught (2026-09-16, 05:44–06:02Z):
//! 1. A decode measured inside our OWN transients measures them, not the lane:
//!    `rustc -j18` for the reboot's warm build read 2 in flight at 0.18 of catalog and
//!    the knee fell 4 → 2. Two windows are known facts, not guesses, and samples inside
//!    them are skipped with a probe: the deploy in flight (the CLI's deploy claim,
//!    published before its warm build — `runtime::deploy_claim::in_flight`) and the
//!    boot window (`runtime::boot_clock`, [`BOOT_QUIET`]: board projections rebuilding,
//!    the roster reseating, the desktop dist building). No load-average heuristic: on a
//!    CPU-decode tier the serving lane IS the load (Cormac measured 14 on 12 cores with
//!    nothing but serving), so "busy host" cannot tell a build from a lane.
//! 2. A clamp is a ratchet with no way up: at the knee nothing ever runs ABOVE it, so a
//!    collapsed point above is never re-measured. Points go STALE after
//!    [`FRESH_MS`]; the knee EXPLORES one lane above the largest fresh holding point
//!    when the aggregate-throughput prediction says it will hold (ratio × n/(n+1) ≥
//!    floor) and no fresh point above contradicts it — one flap per model per hour at
//!    most.
//! 3. The clamp probe fires when the clamp CHANGES, not every planning tick.

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
/// A point older than this is not evidence about the box as it is now: a collapse
/// measured under last hour's build must not hold the roster down all day.
pub const FRESH_MS: u64 = 60 * 60 * 1000;
/// Samples inside this window after boot are the boot's, not the lane's.
pub const BOOT_QUIET: std::time::Duration = std::time::Duration::from_secs(600);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CurvePoint {
    pub samples: u32,
    pub ratio_ema: f64,
    /// Unix ms of the newest sample; a record without one (pre-freshness) is stale.
    #[serde(default)]
    pub last_ms: u64,
}

impl CurvePoint {
    fn trusted_at(&self, now_ms: u64) -> bool {
        self.samples >= MIN_SAMPLES && now_ms.saturating_sub(self.last_ms) <= FRESH_MS
    }
}

/// The measured decode curve of one model: concurrency → per-stream ratio.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DecodeCurve {
    pub points: BTreeMap<u32, CurvePoint>,
}

impl DecodeCurve {
    pub fn observe(&mut self, inflight: u32, ratio: f64, now_ms: u64) {
        if inflight == 0 || !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        let p = self.points.entry(inflight).or_default();
        // A stale point restarts: last hour's curve is not this hour's evidence.
        if now_ms.saturating_sub(p.last_ms) > FRESH_MS {
            p.samples = 0;
        }
        p.ratio_ema = if p.samples == 0 {
            ratio
        } else {
            p.ratio_ema + EMA_ALPHA * (ratio - p.ratio_ema)
        };
        p.samples = p.samples.saturating_add(1);
        p.last_ms = now_ms;
    }

    /// The knee under `floor` at `now_ms`: the LARGEST fresh, trusted concurrency whose
    /// ratio holds — plus one when the aggregate-throughput prediction says the next
    /// lane holds too (`ratio × n/(n+1) ≥ floor`) and no fresh point above says
    /// otherwise (the way back up from a clamp). If every fresh concurrency has
    /// collapsed, one below the smallest collapsed one (never below 1) — shrink and
    /// re-measure. `None` = nothing fresh and trusted (no clamp; the roster rules).
    pub fn knee(&self, floor: f64, now_ms: u64) -> Option<u32> {
        let trusted: Vec<(&u32, &CurvePoint)> =
            self.points.iter().filter(|(_, p)| p.trusted_at(now_ms)).collect();
        if trusted.is_empty() {
            return None;
        }
        if let Some((n, p)) = trusted.iter().rev().find(|(_, p)| p.ratio_ema >= floor) {
            let n = **n;
            let is_top = trusted.last().map(|(top, _)| **top == n).unwrap_or(false); // unwrap_or: non-empty by the guard above
            let predicted_next = p.ratio_ema * n as f64 / (n + 1) as f64;
            return Some(if is_top && predicted_next >= floor { n + 1 } else { n });
        }
        trusted.first().map(|(n, _)| (**n).saturating_sub(1).max(1))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a clock before 1970 makes every point stale — the roster rules, nothing is clamped on a lie
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
    let now = now_ms();
    // A decode measured inside our own transients measures them, not the lane.
    if let Some(why) = own_transient(now) {
        crate::probe!(
            class = "serving.decode_knee.sample_skipped",
            model = model,
            inflight = inflight as u64,
            measured_tps,
            why,
            "decode sample skipped — inside our own transient, the rate is not the lane's"
        );
        return;
    }
    let ratio = measured_tps / expected_tps;
    let mut curves = CURVES.lock();
    let curve = curves.entry(model.to_string()).or_default();
    let before = curve.knee(DECODE_TAX_FLOOR, now);
    curve.observe(inflight, ratio, now);
    let after = curve.knee(DECODE_TAX_FLOOR, now);
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

/// Which of our own transients is open right now, if any: a deploy in flight (a live
/// `continuum reboot` holds the claim while its warm build compiles beside this core) or
/// the boot window. Named so the skip probe says which.
fn own_transient(now: u64) -> Option<&'static str> {
    if let Some(root) = crate::commands::benchmark::continuum_home().ok() {
        if crate::runtime::deploy_claim::in_flight(&root, now).blocks() {
            return Some("deploy_in_flight");
        }
    }
    match crate::runtime::boot_clock::elapsed() {
        Some(up) if up < BOOT_QUIET => Some("boot_window"),
        _ => None,
    }
}

/// The knee for `model`, if measured.
pub fn knee_for(model: &str) -> Option<u32> {
    let now = now_ms();
    CURVES.lock().get(model).and_then(|c| c.knee(DECODE_TAX_FLOOR, now))
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

    const T: u64 = 10_000_000_000; // a fixed "now" — every sample fresh unless a test ages it

    fn measured(c: &mut DecodeCurve, n: u32, ratio: f64) {
        for _ in 0..MIN_SAMPLES {
            c.observe(n, ratio, T);
        }
    }

    // what this catches (2026-09-16, the M5 at 8 lanes / 7 t/s): the knee is the largest
    // concurrency that still clears the tax floor; a roster of 16 clamps to it; when every
    // measured concurrency has collapsed the rule shrinks below the smallest one instead
    // of standing still; nothing trusted = no clamp; a knee never raises demand.
    #[test]
    fn lanes_stop_at_the_largest_concurrency_that_still_holds() {
        let mut c = DecodeCurve::default();
        measured(&mut c, 2, 0.30); // 20 of 68 t/s
        measured(&mut c, 4, 0.22); // 15 of 68
        measured(&mut c, 8, 0.10); // 7 of 68
        assert_eq!(c.knee(DECODE_TAX_FLOOR, T), Some(2));
        assert_eq!(knee_lanes(16, c.knee(DECODE_TAX_FLOOR, T)), 2, "the roster's 16 clamps to the knee");
        assert_eq!(knee_lanes(1, Some(4)), 1, "a knee never raises demand");
        // Only collapsed concurrencies measured → one below the smallest, never zero.
        let mut worst = DecodeCurve::default();
        measured(&mut worst, 6, 0.12);
        measured(&mut worst, 8, 0.09);
        assert_eq!(worst.knee(DECODE_TAX_FLOOR, T), Some(5));
        let mut floor = DecodeCurve::default();
        measured(&mut floor, 1, 0.05);
        assert_eq!(floor.knee(DECODE_TAX_FLOOR, T), Some(1));
        // Untrusted (too few samples) = no clamp.
        let mut thin = DecodeCurve::default();
        thin.observe(8, 0.1, T);
        assert_eq!(thin.knee(DECODE_TAX_FLOOR, T), None);
        assert_eq!(knee_lanes(16, None), 16);
    }

    // what this catches (the first live deploy, 06:02Z): a clamp is a ratchet with no way
    // up — at the knee nothing runs above it, so a collapse measured under a build held
    // seventeen minds on two lanes. The knee EXPLORES one lane above the largest fresh
    // holding point when the aggregate-throughput prediction says it holds, a fresh
    // collapsed point above blocks the climb, and a stale one (older than an hour) no
    // longer does. A stale point also restarts its EMA instead of averaging last hour in.
    #[test]
    fn the_knee_climbs_back_when_the_collapse_above_it_goes_stale() {
        let mut c = DecodeCurve::default();
        measured(&mut c, 2, 0.45); // 0.45 × 2/3 = 0.30 ≥ floor → predicted to hold at 3
        assert_eq!(c.knee(DECODE_TAX_FLOOR, T), Some(3), "explores one above a comfortable hold");
        measured(&mut c, 3, 0.20); // measured: 3 collapses (a build was running)
        assert_eq!(c.knee(DECODE_TAX_FLOOR, T), Some(2), "a fresh collapse above blocks the climb");
        let later = T + FRESH_MS + 1;
        assert_eq!(c.knee(DECODE_TAX_FLOOR, later), None, "nothing fresh = nothing trusted = no clamp");
        measured_at(&mut c, 2, 0.45, later);
        assert_eq!(c.knee(DECODE_TAX_FLOOR, later), Some(3), "the stale collapse no longer holds it down");
        c.observe(3, 0.40, later);
        let p = &c.points[&3];
        assert_eq!((p.samples, p.ratio_ema), (1, 0.40), "a stale point restarts, it does not average last hour in");
        // At the tax line the prediction says the next lane will NOT hold: no exploring.
        let mut edge = DecodeCurve::default();
        measured(&mut edge, 4, 0.26); // 0.26 × 4/5 = 0.208 < floor
        assert_eq!(edge.knee(DECODE_TAX_FLOOR, T), Some(4));
    }

    fn measured_at(c: &mut DecodeCurve, n: u32, ratio: f64, at: u64) {
        for _ in 0..MIN_SAMPLES {
            c.observe(n, ratio, at);
        }
    }

    // what this catches: the record survives a reboot and a corrupt file is forgotten;
    // a record from before freshness (no `last_ms`) loads as stale, never as a clamp.
    #[test]
    fn the_knee_is_remembered_across_a_reboot_and_a_corrupt_record_is_forgotten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("decode-knee.json");
        let mut curves = BTreeMap::new();
        let mut c = DecodeCurve::default();
        measured(&mut c, 3, 0.4);
        curves.insert("m".to_string(), c);
        save_to(&p, &curves);
        assert_eq!(load_from(&p), curves);
        std::fs::write(&p, b"{nope").expect("write");
        assert!(load_from(&p).is_empty());
        std::fs::write(&p, br#"{"m":{"points":{"2":{"samples":9,"ratio_ema":0.1}}}}"#).expect("write");
        let old = load_from(&p);
        assert_eq!(old["m"].knee(DECODE_TAX_FLOOR, T), None, "a pre-freshness record is stale evidence");
    }
}
