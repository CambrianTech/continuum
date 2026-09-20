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
//! per-stream tokens/s)`; per model the ledger keeps an EMA of the rate at each
//! concurrency. The KNEE is the largest concurrency whose per-stream rate still clears
//! [`DECODE_FLOOR_TPS`] — an ABSOLUTE floor, never a fraction of the catalog: the first
//! clean hour (2026-09-16 13:03–13:14Z) measured this box at 15 t/s per stream with 2 in
//! flight and 9 t/s with 4, against a catalog 68 measured on an EMPTY M5; a 0.25-of-catalog
//! floor read every lane as collapsed and walked the knee 4 → 3 → 2 → 1 — sixteen minds
//! on one lane, which no law asked for. The physics: on a memory-bound box the AGGREGATE
//! rate is roughly constant across lane counts (2 × 15 ≈ 4 × 9), so lanes only choose
//! between queueing and per-stream speed; the floor names the speed below which a
//! stream is slower than a person reads, and the knee never drops below
//! [`MIN_KNEE_LANES`] while a roster is seated (one lane serialises prefill behind
//! decode and idles the box between turns). Fewer warm slots plus paging IS the design
//! (the restore economy, #4069) — not a loss of minds.
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
//!    when the constant-aggregate prediction says it will hold (tps × n/(n+1) ≥ floor)
//!    and no fresh point above contradicts it — one flap per model per hour at most.
//! 3. The clamp probe fires when the clamp CHANGES, not every planning tick.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// A lane count "holds" while its per-stream decode stays at or above this many tokens
/// per second — an absolute floor. Ten is 1.5× a reader's pace (~7 t/s) and three times
/// speech: below it a citizen's voice in a call drags and an 800-token act takes longer
/// than the wait for a lane it was meant to shorten. The catalog single-stream rate
/// (68 on an empty M5) is the collapse ALARM's reference (`serving.throughput.degraded`),
/// never this rule's — a box carrying sixteen minds is not empty.
pub const DECODE_FLOOR_TPS: f64 = 10.0;
/// The knee never clamps a seated roster below this: one lane serialises the next
/// turn's prefill behind the current decode and idles the box between turns.
pub const MIN_KNEE_LANES: u32 = 2;
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
    /// Per-stream tokens/s at this concurrency (EMA). A record from before this field
    /// (ratio-only) loads as 0 and is stale by `last_ms` anyway.
    #[serde(default)]
    pub tps_ema: f64,
    /// Unix ms of the newest sample; a record without one (pre-freshness) is stale.
    #[serde(default)]
    pub last_ms: u64,
}

impl CurvePoint {
    fn trusted_at(&self, now_ms: u64) -> bool {
        self.samples >= MIN_SAMPLES && now_ms.saturating_sub(self.last_ms) <= FRESH_MS
    }
}

/// The measured decode curve of one model: concurrency → per-stream tokens/s.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DecodeCurve {
    pub points: BTreeMap<u32, CurvePoint>,
}

impl DecodeCurve {
    pub fn observe(&mut self, inflight: u32, tps: f64, now_ms: u64) {
        if inflight == 0 || !tps.is_finite() || tps <= 0.0 {
            return;
        }
        let p = self.points.entry(inflight).or_default();
        // A stale point restarts: last hour's curve is not this hour's evidence.
        if now_ms.saturating_sub(p.last_ms) > FRESH_MS {
            p.samples = 0;
        }
        p.tps_ema = if p.samples == 0 {
            tps
        } else {
            p.tps_ema + EMA_ALPHA * (tps - p.tps_ema)
        };
        p.samples = p.samples.saturating_add(1);
        p.last_ms = now_ms;
    }

    /// The knee under `floor_tps` at `now_ms`: the LARGEST fresh, trusted concurrency
    /// whose per-stream rate holds — plus one when the constant-aggregate prediction says
    /// the next lane holds too (`tps × n/(n+1) ≥ floor`) and no fresh point above says
    /// otherwise (the way back up from a clamp). If every fresh concurrency has
    /// collapsed, one below the smallest collapsed one, never below [`MIN_KNEE_LANES`] —
    /// shrink and re-measure. `None` = nothing fresh and trusted (no clamp; the roster
    /// rules).
    /// The trusted point with the fewest streams in flight — a single turn's decode rate on
    /// this box. `None` = no trusted point (an absence is not a number).
    pub fn lightest_trusted_tps(&self, now_ms: u64) -> Option<f64> {
        self.points.iter().filter(|(_, p)| p.trusted_at(now_ms)).map(|(_, p)| p.tps_ema).next()
    }
    pub fn knee(&self, floor_tps: f64, now_ms: u64) -> Option<u32> {
        // FRESH points first: a fresh measurement is trusted enough to EXPLORE one lane
        // above the largest holding point when the constant-aggregate prediction says so.
        let fresh: Vec<(&u32, &CurvePoint)> =
            self.points.iter().filter(|(_, p)| p.trusted_at(now_ms)).collect();
        if !fresh.is_empty() {
            return Self::knee_over(&fresh, floor_tps, true);
        }
        // No fresh point, but a STALE measurement of THIS box is still better evidence than
        // the roster's blind demand: a ceiling measured an hour ago CAPS (it does not
        // explore — we are not confident enough to grow past it uncorroborated), and the
        // lane re-measures at the capped count on its next decode. Returning None here
        // unclamped the roster to its full demand and relaunched the exact shape that had
        // collapsed: measured 2026-09-17 11:34Z, the M5 booted 8 lanes (the knee's 7/8-lane
        // points 102 min old), Metal-OOM'd every decode, and every reboot re-picked 8 —
        // a dead node with the answer sitting stale on disk (card 03243d67).
        let stale: Vec<(&u32, &CurvePoint)> = self
            .points
            .iter()
            // A pre-freshness record loads with `tps_ema` 0 (the field did not exist): that
            // is UNKNOWN, never a measured 0 t/s — reading it as a collapse is the
            // absence-as-value trap. A real stale measurement has a positive rate.
            .filter(|(_, p)| p.samples >= MIN_SAMPLES && p.tps_ema > 0.0)
            .collect();
        if stale.is_empty() {
            return None; // never measured = the roster rules, nothing is clamped on nothing
        }
        Self::knee_over(&stale, floor_tps, false)
    }

    /// The knee rule over a set of points (already filtered for trust/staleness): the
    /// largest concurrency whose per-stream rate holds the floor, plus one when `explore`
    /// and the constant-aggregate prediction says the next lane holds AND no measured
    /// point above contradicts it. If every point has collapsed, one below the smallest.
    fn knee_over(points: &[(&u32, &CurvePoint)], floor_tps: f64, explore: bool) -> Option<u32> {
        if points.is_empty() {
            return None;
        }
        if let Some((n, p)) = points.iter().rev().find(|(_, p)| p.tps_ema >= floor_tps) {
            let n = **n;
            let is_top = points.last().map(|(top, _)| **top == n).unwrap_or(false); // unwrap_or: non-empty by the guard above
            let predicted_next = p.tps_ema * n as f64 / (n + 1) as f64;
            let grow = explore && is_top && predicted_next >= floor_tps;
            return Some((if grow { n + 1 } else { n }).max(MIN_KNEE_LANES));
        }
        points.first().map(|(n, _)| (**n).saturating_sub(1).max(MIN_KNEE_LANES))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a clock before 1970 makes every point stale — the roster rules, nothing is clamped on a lie
}

/// The pure rule the planner applies: the roster's demand, clamped to the knee when one
/// is known. A knee never RAISES demand, and never takes a seated roster below
/// [`MIN_KNEE_LANES`].
pub fn knee_lanes(demand: u32, knee: Option<u32>) -> u32 {
    match knee {
        Some(k) => demand.min(k.max(MIN_KNEE_LANES)),
        None => demand,
    }
}

static CURVES: LazyLock<parking_lot::Mutex<BTreeMap<String, DecodeCurve>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));

/// Unix ms of the last write of the curve file by this process (0 = none yet).
static LAST_SAVE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The curve file is refreshed at least this often while samples flow, whether or not
/// the knee moved: the file is what the NEXT boot loads, and a point is only as fresh
/// as its last WRITE. (2026-09-17: saved only on a knee move, the M5's points aged 81
/// minutes on disk under 144 acts/hour, the boot read them past [`FRESH_MS`], and the
/// plan launched eight lanes for a knee of four.)
pub const SAVE_EVERY_MS: u64 = 60 * 1000;

/// Whether this sample writes the file: a knee move always; otherwise once per
/// [`SAVE_EVERY_MS`] so the record on disk never lags the lane by more than a minute.
pub fn persists(knee_moved: bool, last_save_ms: u64, now_ms: u64) -> bool {
    knee_moved || now_ms.saturating_sub(last_save_ms) >= SAVE_EVERY_MS
}

/// Record one measured decode for `model` (the adapter's seam, every decode — not only
/// the collapsed ones). Persists on a knee move and on cadence while samples flow.
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
    let before = curve.knee(DECODE_FLOOR_TPS, now);
    curve.observe(inflight, measured_tps, now);
    let after = curve.knee(DECODE_FLOOR_TPS, now);
    let moved = before != after;
    if moved {
        crate::probe!(
            class = "serving.decode_knee.moved",
            model = model,
            from = before.map(|k| k as u64).unwrap_or(0),
            to = after.map(|k| k as u64).unwrap_or(0),
            inflight = inflight as u64,
            measured_tps,
            ratio,
            "the measured decode knee moved — lanes follow it, not the roster"
        );
    }
    use std::sync::atomic::Ordering;
    if persists(moved, LAST_SAVE_MS.load(Ordering::Relaxed), now) {
        save_all(&curves);
        LAST_SAVE_MS.store(now, Ordering::Relaxed);
    }
}

/// Which of our own transients is open right now, if any: a deploy in flight (a live
/// `continuum reboot` holds the claim while its warm build compiles beside this core) or
/// the boot window. Named so the skip probe says which.
pub(crate) fn own_transient(now: u64) -> Option<&'static str> {
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
    CURVES.lock().get(model).and_then(|c| c.knee(DECODE_FLOOR_TPS, now))
}

/// The knee of the model this box measured most recently, with its name — for a boot that
/// remembers a model with no curve (2026-09-20: a record poisoned with "coder-14b" asked
/// for a knee nobody measured, read None, and planned 8 lanes on a box whose 27B knee is
/// 2). The last measured curve IS this box's decode ceiling until a lane says otherwise.
pub fn last_measured_knee() -> Option<(String, u32)> {
    let now = now_ms();
    let curves = CURVES.lock();
    last_measured(&curves).and_then(|(m, c)| c.knee(DECODE_FLOOR_TPS, now).map(|k| (m.clone(), k)))
}

/// PURE: the curve whose newest point is newest.
pub fn last_measured(curves: &BTreeMap<String, DecodeCurve>) -> Option<(&String, &DecodeCurve)> {
    curves
        .iter()
        .filter_map(|(m, c)| c.points.values().map(|p| p.last_ms).max().map(|t| (t, m, c)))
        .max_by_key(|(t, _, _)| *t)
        .map(|(_, m, c)| (m, c))
}
/// The decode rate this box measured for `model` at its lightest trusted concurrency —
/// what one turn's decode costs per token here. `None` = unmeasured.
pub fn tps_for(model: &str) -> Option<f64> {
    let now = now_ms();
    CURVES.lock().get(model).and_then(|c| c.lightest_trusted_tps(now))
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

    // what this catches (2026-09-17): the curve file was written only when the knee
    // MOVED, so a lane sampling steadily at the same knee left its points aging on disk;
    // the next boot loaded them past FRESH_MS and planned unclamped. A sample writes on a
    // move, and otherwise once per SAVE_EVERY_MS while samples flow.
    #[test]
    fn a_steady_knee_still_refreshes_the_file_on_cadence() {
        assert!(persists(true, T, T), "a move writes at once");
        assert!(!persists(false, T, T + SAVE_EVERY_MS - 1), "inside the cadence, a steady knee waits");
        assert!(persists(false, T, T + SAVE_EVERY_MS), "at the cadence a steady knee writes");
        assert!(persists(false, 0, T), "a process that never saved writes on its first sample");
    }

    const T: u64 = 10_000_000_000; // a fixed "now" — every sample fresh unless a test ages it
    const F: f64 = DECODE_FLOOR_TPS;

    fn measured(c: &mut DecodeCurve, n: u32, tps: f64) {
        measured_at(c, n, tps, T);
    }

    fn measured_at(c: &mut DecodeCurve, n: u32, tps: f64, at: u64) {
        for _ in 0..MIN_SAMPLES {
            c.observe(n, tps, at);
        }
    }

    // what this catches (2026-09-16, the M5 at 8 lanes / 7 t/s): the knee is the largest
    // concurrency whose per-stream rate still clears the absolute floor; a roster of 16
    // clamps to it; when every measured concurrency has collapsed the rule shrinks below
    // the smallest one — but never below two lanes for a seated roster (the first clean
    // hour walked 4 → 3 → 2 → 1 against a 0.25-of-catalog floor: sixteen minds on one
    // lane); nothing trusted = no clamp; a knee never raises demand.
    #[test]
    fn lanes_stop_at_the_largest_concurrency_that_still_holds() {
        let mut c = DecodeCurve::default();
        measured(&mut c, 2, 20.0);
        measured(&mut c, 4, 9.0);
        measured(&mut c, 8, 7.0);
        assert_eq!(c.knee(F, T), Some(2));
        assert_eq!(knee_lanes(16, c.knee(F, T)), 2, "the roster's 16 clamps to the knee");
        assert_eq!(knee_lanes(1, Some(4)), 1, "a knee never raises demand");
        // Only collapsed concurrencies measured → one below the smallest.
        let mut worst = DecodeCurve::default();
        measured(&mut worst, 6, 8.0);
        measured(&mut worst, 8, 6.0);
        assert_eq!(worst.knee(F, T), Some(5));
        // Even 2 in flight collapsed: the floor of the rule is two lanes, not one.
        let mut two = DecodeCurve::default();
        measured(&mut two, 2, 8.0);
        measured(&mut two, 3, 6.0);
        assert_eq!(two.knee(F, T), Some(MIN_KNEE_LANES), "never one lane for a roster");
        assert_eq!(knee_lanes(16, Some(1)), MIN_KNEE_LANES, "a remembered 1 is lifted to two");
        // Untrusted (too few samples) = no clamp.
        let mut thin = DecodeCurve::default();
        thin.observe(8, 7.0, T);
        assert_eq!(thin.knee(F, T), None);
        assert_eq!(knee_lanes(16, None), 16);
    }

    // what this catches (the first live deploy, 06:02Z): a clamp is a ratchet with no way
    // up — at the knee nothing runs above it, so a collapse measured under a build held
    // seventeen minds on two lanes. The knee EXPLORES one lane above the largest fresh
    // holding point when the constant-aggregate prediction says it holds, a fresh
    // collapsed point above blocks the climb, and a stale one (older than an hour) no
    // longer does. A stale point also restarts its EMA instead of averaging last hour in.
    #[test]
    fn the_knee_climbs_back_when_the_collapse_above_it_goes_stale() {
        let mut c = DecodeCurve::default();
        measured(&mut c, 2, 30.0); // 30 × 2/3 = 20 ≥ floor → predicted to hold at 3
        assert_eq!(c.knee(F, T), Some(3), "explores one above a comfortable hold");
        measured(&mut c, 3, 8.0); // measured: 3 collapses (a build was running)
        assert_eq!(c.knee(F, T), Some(2), "a fresh collapse above blocks the climb");
        let later = T + FRESH_MS + 1;
        // Stale now CAPS at the last holding count (2) rather than unclamping to None —
        // 3 collapsed, stale, so no exploration back to it (card 03243d67).
        assert_eq!(c.knee(F, later), Some(2), "a stale measurement caps, it does not unclamp");
        measured_at(&mut c, 2, 30.0, later);
        assert_eq!(c.knee(F, later), Some(3), "the stale collapse no longer holds it down");
        c.observe(3, 18.0, later);
        let p = &c.points[&3];
        assert_eq!((p.samples, p.tps_ema), (1, 18.0), "a stale point restarts, it does not average last hour in");
        // At the floor the prediction says the next lane will NOT hold: no exploring.
        let mut edge = DecodeCurve::default();
        measured(&mut edge, 4, 11.0); // 11 × 4/5 = 8.8 < floor
        assert_eq!(edge.knee(F, T), Some(4));
    }

    // what this catches: the record survives a reboot and a corrupt file is forgotten;
    // a record from before freshness (no `last_ms`) loads as stale, never as a clamp.
    // what this catches (2026-09-17 11:34Z, card 03243d67): when every point has aged past
    // FRESH_MS the knee falls back to the STALE measurement (a cap, no exploration) rather
    // than to None — returning None unclamped the roster to 8 lanes and relaunched the shape
    // that Metal-OOM'd, dead-looping the node with the answer stale on disk. A stale 8@8.6
    // (below floor) + 7@12.1 caps to 7 and does NOT explore back to 8.
    #[test]
    fn a_stale_knee_still_caps_and_does_not_explore() {
        let mut c = DecodeCurve::default();
        let old = T - FRESH_MS - 1; // every point stale
        measured_at(&mut c, 7, F + 2.0, old);
        measured_at(&mut c, 8, F - 2.0, old); // 8 collapsed, and it is stale
        // Fresh view: nothing trusted → but the stale fallback caps at 7, no +1 to 8.
        assert_eq!(c.knee(F, T), Some(7), "a stale ceiling caps; the collapsed top blocks exploration");
        // A box that only ever measured a HOLDING top, stale, caps there without growing.
        let mut c2 = DecodeCurve::default();
        measured_at(&mut c2, 4, F + 5.0, old);
        assert_eq!(c2.knee(F, T), Some(4), "stale = no exploration bonus, unlike a fresh top");
        // Truly never measured is still None (the roster rules).
        assert_eq!(DecodeCurve::default().knee(F, T), None);
    }

    #[test]
    fn the_knee_is_remembered_across_a_reboot_and_a_corrupt_record_is_forgotten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("decode-knee.json");
        let mut curves = BTreeMap::new();
        let mut c = DecodeCurve::default();
        measured(&mut c, 3, 15.0);
        curves.insert("m".to_string(), c);
        save_to(&p, &curves);
        assert_eq!(load_from(&p), curves);
        std::fs::write(&p, b"{nope").expect("write");
        assert!(load_from(&p).is_empty());
        std::fs::write(&p, br#"{"m":{"points":{"2":{"samples":9,"ratio_ema":0.1}}}}"#).expect("write");
        let old = load_from(&p);
        assert_eq!(old["m"].knee(F, T), None, "a pre-freshness record is stale evidence");
    }
    // what this catches: the occupancy bound reads a single turn's decode rate — the
    // trusted point with the FEWEST streams — never an untrusted or heavier one.
    #[test]
    fn the_lightest_trusted_point_is_one_turns_decode_rate() {
        let mut c = DecodeCurve::default();
        let now = 10_000_000;
        for _ in 0..MIN_SAMPLES { c.observe(2, 9.0, now); }
        assert_eq!(c.lightest_trusted_tps(now), Some(9.0), "two in flight is the lightest trusted point");
        c.observe(1, 15.0, now);
        assert_eq!(c.lightest_trusted_tps(now), Some(9.0), "one sample at 1 is not trusted yet");
        for _ in 0..MIN_SAMPLES { c.observe(1, 15.0, now); }
        assert_eq!(c.lightest_trusted_tps(now), Some(15.0), "trusted at 1: the single-turn rate");
        assert_eq!(c.lightest_trusted_tps(now + FRESH_MS + 1), None, "stale points are not a number");
    }


    // what this catches (2026-09-20): a boot remembering a model with no curve — the last
    // measured curve answers, named, so the plan is clamped by THIS box's evidence.
    #[test]
    fn the_last_measured_curve_answers_when_the_remembered_model_has_none() {
        let mut curves: BTreeMap<String, DecodeCurve> = BTreeMap::new();
        let mut old = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES { old.observe(1, 30.0, 1_000); }
        let mut newer = DecodeCurve::default();
        for _ in 0..MIN_SAMPLES { newer.observe(2, 6.0, 5_000); }
        curves.insert("old-model".into(), old);
        curves.insert("the-27b".into(), newer);
        let (m, c) = last_measured(&curves).expect("two curves, one newest");
        assert_eq!(m, "the-27b");
        assert_eq!(c.knee(DECODE_FLOOR_TPS, 6_000), Some(MIN_KNEE_LANES), "2 lanes at 6 t/s: the knee holds at the floor's minimum");
        assert!(last_measured(&BTreeMap::new()).is_none(), "nothing measured = nothing to answer");
    }

}
