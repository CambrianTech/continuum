//! THE LANE'S REAL FOOTPRINT — measured off the serving process, fed back into the plan.
//!
//! Measured 2026-09-17 10:2xZ on the M5 (64 GB, Ornith-1.5-35B, Metal): the plan fitted
//! 8 lanes × 51,712 into a 30 GB budget from `weights 21.7 GB + kv_per_token × tokens`
//! (the header's attending-layer rate, halved for q8_0 K) — a model of the server worth
//! ~30 GB. `vmmap` of the live server: mapped file 19.9 GB (the weights, file-backed),
//! MALLOC_LARGE 25.7 GB anonymous of which 25.1 GB SWAPPED. Compute buffers for a 412k
//! total context at ubatch 2048, the hybrid's per-slot recurrent state and its
//! `--ctx-checkpoints`, allocator slack — none of it in `ModelFootprint`. The box swapped
//! (94–96%) and every stream decoded at 7.9 t/s: the knee measured the SYMPTOM of a plan
//! that never checked its own arithmetic against the process it launched.
//!
//! This module checks. On the serving tick the daemon reads the lane's ANONYMOUS
//! footprint (macOS `proc_pid_rusage`: the larger of phys_footprint — dirty + compressed,
//! mapped files excluded, so everything BUT the mmapped weights — and the process's
//! lifetime peak of it, since the current figure dips as the pager moves pages; Linux
//! `RssAnon + VmSwap`), decomposes it with the plan's own formula
//! (`lanes × (compute_floor + per_token × window)`) into a measured per-token cost, and
//! keeps it per model in `state/lane-footprint.json` — fresh by its last WRITE, on a
//! cadence, never only on change ([[a-record-saved-only-on-change-is-stale-at-the-next-boot]]).
//! The plan raises its `kv_per_token` to match when the measurement is fresh and larger:
//! a measured footprint is a LOWER bound of the need (untouched pages are not resident),
//! so it only ever corrects the estimate upward. The same shape as the decode knee:
//! a measured fact over a catalog guess, one probe when it moves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// A measurement older than this is not this hour's box.
pub const FRESH_MS: u64 = 60 * 60 * 1000;
/// The record is written at least this often while samples flow.
pub const SAVE_EVERY_MS: u64 = 60 * 1000;
/// Samples are taken at most this often — one syscall a minute, not one a tick.
pub const SAMPLE_EVERY_MS: u64 = 60 * 1000;
/// A measured per-token cost within this much of the estimate is agreement, not a
/// correction: the plan is not re-derived on rounding.
pub const AGREEMENT_PCT: u64 = 20;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MeasuredCost {
    /// Bytes per served token beyond the weights and the per-lane compute floor.
    pub per_token_bytes: u64,
    pub lanes: u32,
    pub window: u32,
    pub anon_bytes: u64,
    pub last_ms: u64,
}

impl MeasuredCost {
    fn fresh_at(&self, now_ms: u64) -> bool {
        self.per_token_bytes > 0 && now_ms.saturating_sub(self.last_ms) <= FRESH_MS
    }
}

/// The plan's own decomposition, inverted: `anon = host_cache + lanes × (compute_floor +
/// per_token × window)` → per_token. `None` when the shape is degenerate or the process
/// holds no more than the host prompt cache it was granted plus its compute floors
/// (nothing to attribute to tokens).
///
/// `host_cache_bytes` is the `--cache-ram` the daemon itself handed the engine: that
/// RAM fills with saved prompt states over the serve's life and lives in the same
/// anonymous footprint as the KV. Measured 2026-09-19 on the M5: a 19,519 MiB grant
/// filled the lane's footprint from 16.7 to 20.4 GB across an hour at 2 × 67,584, and
/// read as 133,720 B/token against a 32,768 estimate — the planner "corrected" to
/// ~107k, found two lanes could not fit, and starved five minds on one lane while the
/// process still ran two. Subtracting the whole grant makes the reading a LOWER bound
/// on the KV-attributable bytes, which is the only direction a footprint may correct
/// an estimate ([`corrects`]).
/// The smallest served window a per-token reading may be taken at: one full turn
/// (`BOOTSTRAP_WORKING_SET`, 16,384 tokens). Below it the bytes beyond weights are the
/// engine's fixed per-lane buffers, not KV, and dividing them by the window reads as
/// hundreds of KB per token (2,048 tokens: ~500 MB of buffers → ~244k B/token against a
/// 32k estimate). Saved as the record, that number fits no window and pins the next plan
/// at the floor the node fell to — the trap the 5090 sat in on 2026-09-20 (1 × 2,048,
/// refusing every 17–31k prompt). A starved window is a fact for the plan to escape,
/// never a measurement to remember.
pub const MEASURABLE_WINDOW_MIN: u32 = crate::cognition::serving_plan::BOOTSTRAP_WORKING_SET;

/// PURE: whether a per-token reading taken at `window` is a measurement at all.
pub fn window_measurable(window: u32) -> bool {
    window >= MEASURABLE_WINDOW_MIN
}

pub fn per_token_from(
    anon_bytes: u64,
    lanes: u32,
    window: u32,
    compute_floor_per_lane: u64,
    host_cache_bytes: u64,
) -> Option<u64> {
    if lanes == 0 || window == 0 {
        return None;
    }
    let beyond_cache = anon_bytes.checked_sub(host_cache_bytes)?;
    let per_lane = beyond_cache / lanes as u64;
    let beyond_floor = per_lane.checked_sub(compute_floor_per_lane)?;
    let per_token = beyond_floor / window as u64;
    (per_token > 0).then_some(per_token)
}

/// Whether a measured per-token cost CORRECTS an estimate: larger by more than
/// [`AGREEMENT_PCT`]. Smaller never corrects (a footprint is a lower bound).
pub fn corrects(estimate: u64, measured: u64) -> bool {
    measured.saturating_mul(100) > estimate.saturating_mul(100 + AGREEMENT_PCT)
}

static COSTS: LazyLock<parking_lot::Mutex<BTreeMap<String, MeasuredCost>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));
static LAST_SAVE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LAST_SAMPLE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// True once per [`SAMPLE_EVERY_MS`]: the daemon's tick asks before paying the syscall.
pub fn sample_due(now_ms: u64) -> bool {
    use std::sync::atomic::Ordering;
    let last = LAST_SAMPLE_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) >= SAMPLE_EVERY_MS {
        LAST_SAMPLE_MS.store(now_ms, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// Record one measurement of the live lane. Returns the per-token cost it derived.
///
/// The record is REPLACED by each sample (last write wins), never maxed across samples:
/// "upward only" is a rule about the measurement against the ESTIMATE at apply time
/// ([`corrects`]), not a ratchet over the measurement's own history. A reading taken
/// in a bad minute rules for one sample interval, and a record older than [`FRESH_MS`]
/// is ignored entirely — the plan falls back to its arithmetic, never to a remembered
/// maximum.
pub fn observe(
    model: &str,
    lanes: u32,
    window: u32,
    anon_bytes: u64,
    compute_floor_per_lane: u64,
    host_cache_bytes: u64,
) -> Option<u64> {
    let now = now_ms();
    let sample = per_token_from(anon_bytes, lanes, window, compute_floor_per_lane, host_cache_bytes)
        .map(|per_token| MeasuredCost { per_token_bytes: per_token, lanes, window, anon_bytes, last_ms: now });
    let mut costs = COSTS.lock();
    let moved = apply_sample(&mut costs, model, sample.as_ref());
    use std::sync::atomic::Ordering;
    if moved || now.saturating_sub(LAST_SAVE_MS.load(Ordering::Relaxed)) >= SAVE_EVERY_MS {
        save_all(&costs);
        LAST_SAVE_MS.store(now, Ordering::Relaxed);
    }
    sample.map(|c| c.per_token_bytes)
}

/// One sample against the record: a reading replaces the model's record; NO reading
/// (nothing attributable this minute) RETIRES it. Returns whether the record moved
/// materially (a save is due). A record that cannot be re-confirmed by the live
/// process must not keep ruling the plan from disk — the M5's 133,720 B/token record
/// (2026-09-19) would otherwise survive a relaunch for [`FRESH_MS`] while every fresh
/// sample of the new process read "nothing beyond the cache" and left it standing.
fn apply_sample(costs: &mut BTreeMap<String, MeasuredCost>, model: &str, sample: Option<&MeasuredCost>) -> bool {
    let before = costs.get(model).map(|c| c.per_token_bytes).unwrap_or(0);
    match sample {
        Some(cost) => {
            costs.insert(model.to_string(), cost.clone());
            before == 0 || corrects(before, cost.per_token_bytes) || corrects(cost.per_token_bytes, before)
        }
        None => costs.remove(model).is_some(),
    }
}

/// RETIRE the model's record: a record that cannot be re-confirmed at a measurable window
/// must not keep ruling the plan from disk (Cormac's condition on #4255). A node already in
/// the trap — a ~244k B/token record taken at 2,048 — would otherwise loop: the record
/// corrects the plan up to 1 × 2,048, the starved window withholds the reading, the record
/// stands, the plan stays. Retiring it drops the plan back to the estimate (~33k), which
/// plans a real window, which the next honest reading replaces. Returns whether a record
/// was there to retire; the save is immediate so a boot never reads it again.
pub fn retire(model: &str) -> bool {
    let mut costs = COSTS.lock();
    let gone = apply_sample(&mut costs, model, None);
    if gone {
        save_all(&costs);
        LAST_SAVE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    }
    gone
}

/// The fresh measured record for `model`, if any — per-token AND the geometry it was
/// taken at, so a caller can turn an excess over a known rate into fixed bytes.
pub fn measured_record(model: &str) -> Option<MeasuredCost> {
    let now = now_ms();
    COSTS.lock().get(model).filter(|c| c.fresh_at(now)).cloned()
}


/// The ANONYMOUS memory a process holds — dirty + compressed/swapped, mapped files
/// excluded. On macOS `proc_pid_rusage` (libproc, no entitlement for our own user's
/// processes); on Linux `/proc/<pid>/status` `RssAnon + VmSwap`. `None` = could not
/// look (a dead pid, another platform) — never zero.
pub fn anon_footprint_of(pid: u32) -> Option<u64> {
    anon_footprint_impl(pid)
}

#[cfg(target_os = "macos")]
fn anon_footprint_impl(pid: u32) -> Option<u64> {
    // struct rusage_info_v4: ri_uuid[16], then 35 u64. ri_phys_footprint is index 7
    // (v0: user_time, system_time, pkg_idle_wkups, interrupt_wkups, pageins, wired_size,
    // resident_size, PHYS_FOOTPRINT, proc_start_abstime, proc_exit_abstime); v1 adds six
    // child_* fields (10..16), v2 two diskio (16..18), v3 nine cpu_time_qos/billed/serviced
    // (18..27), v4 logical_writes (27), LIFETIME_MAX_PHYS_FOOTPRINT (28), instructions,
    // cycles, billed_energy, serviced_energy, interval_max_phys_footprint, runnable_time.
    //
    // The measurement is the LARGER of the current footprint and the lifetime peak: the
    // need is what the process ever held, and the current figure dips as the pager moves
    // its pages (measured 2026-09-17 11:21Z: 9.0 → 5.9 GB in one minute on a live lane
    // while swap grew 2 GB — the same server, nothing freed).
    #[repr(C)]
    struct RusageInfoV4 {
        uuid: [u8; 16],
        fields: [u64; 35],
    }
    extern "C" {
        fn proc_pid_rusage(pid: libc::c_int, flavor: libc::c_int, buffer: *mut RusageInfoV4) -> libc::c_int;
    }
    const RUSAGE_INFO_V4: libc::c_int = 4;
    const PHYS_FOOTPRINT: usize = 7;
    const LIFETIME_MAX_PHYS_FOOTPRINT: usize = 28;
    let mut info = RusageInfoV4 { uuid: [0; 16], fields: [0; 35] };
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v4; libproc only
    // writes within it for flavor V4.
    let rc = unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V4, &mut info) };
    (rc == 0).then(|| info.fields[PHYS_FOOTPRINT].max(info.fields[LIFETIME_MAX_PHYS_FOOTPRINT]))
}

#[cfg(target_os = "linux")]
fn anon_footprint_impl(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let mut anon = None;
    let mut swap = 0u64;
    for line in status.lines() {
        let kb = |l: &str| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()).map(|v| v * 1024);
        if line.starts_with("RssAnon:") {
            anon = kb(line);
        } else if line.starts_with("VmSwap:") {
            swap = kb(line).unwrap_or(0);
        }
    }
    anon.map(|a| a + swap)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn anon_footprint_impl(_pid: u32) -> Option<u64> {
    None
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a clock before 1970 makes every record stale — the estimate rules
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| h.join("state").join("lane-footprint.json"))
}

fn load() -> BTreeMap<String, MeasuredCost> {
    default_path().map(|p| load_from(&p)).unwrap_or_default() // unwrap_or_default: no home = nothing remembered; the estimate rules until measured
}

pub fn load_from(path: &Path) -> BTreeMap<String, MeasuredCost> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(), // unwrap_or_default: a corrupt record is forgotten, never trusted — it re-measures within a minute
        Err(_) => BTreeMap::new(),
    }
}

fn save_all(costs: &BTreeMap<String, MeasuredCost>) {
    if let Some(p) = default_path() {
        save_to(&p, costs);
    }
}

pub fn save_to(path: &Path, costs: &BTreeMap<String, MeasuredCost>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(costs) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: no per-token reading at a starved window. At 2,048 tokens the
    // excess over weights is fixed buffers and reads as ~244k B/token; remembered, it pins
    // the next plan at the floor (the 5090, 2026-09-20). One full turn is the floor.
    #[test]
    fn a_starved_window_is_not_a_measurement() {
        assert!(!window_measurable(2_048));
        assert!(!window_measurable(MEASURABLE_WINDOW_MIN - 1));
        assert!(window_measurable(MEASURABLE_WINDOW_MIN));
        assert!(window_measurable(67_072));
        // The arithmetic the floor guards against: 500 MB of fixed buffers over 2,048 tokens.
        let per_token = per_token_from(500_000_000, 1, 2_048, 0, 0).expect("reads");
        assert!(per_token > 200_000, "a starved window reads fixed buffers as tokens: {per_token} B/token");
        // A node ALREADY in the trap climbs out: the record taken at the starved window is
        // RETIRED, not merely left standing (Cormac's condition on #4255) — otherwise
        // record → 1 × 2,048 → withhold → record is a loop nothing opens.
        let model = "trapped-fixture";
        COSTS.lock().insert(
            model.to_string(),
            MeasuredCost { per_token_bytes: per_token, lanes: 1, window: 2_048, anon_bytes: 500_000_000, last_ms: now_ms() },
        );
        assert!(measured_record(model).is_some(), "the trap record rules before retirement");
        assert!(retire(model), "a record was there to retire");
        assert!(measured_record(model).is_none(), "retired: the plan falls back to the estimate and plans a real window");
        assert!(!retire(model), "nothing left to retire");
    }

    // what this catches (2026-09-17, the M5 swapping at 8 × 51,712): the plan's per-token
    // cost is derived from the process it launched with the plan's own decomposition, and
    // only a measurement materially ABOVE the estimate corrects it — a footprint is a
    // lower bound, so a smaller reading never shrinks the plan's reserve.
    #[test]
    fn the_measured_cost_inverts_the_plans_decomposition_and_only_corrects_upward() {
        let gb = 1u64 << 30;
        // The live shape: 25.7 GB anonymous, 8 lanes × 51,712, compute floor 1.36 GB/lane.
        let per_token = per_token_from(25_700_000_000, 8, 51_712, 1_357_000_000, 0).expect("attributable");
        assert!((35_000..38_000).contains(&per_token), "{per_token} B/token");
        assert!(corrects(10_240, per_token), "3.5× the estimate is a correction");
        assert!(!corrects(per_token, 10_240), "a smaller reading never corrects downward");
        assert!(!corrects(10_240, 12_000), "17% is agreement, not a correction");
        // Degenerate shapes attribute nothing.
        assert_eq!(per_token_from(gb, 0, 51_712, 0, 0), None);
        assert_eq!(per_token_from(gb, 2, 0, 0, 0), None);
        assert_eq!(per_token_from(gb, 2, 4096, gb, 0), None, "under the compute floors: nothing beyond them to attribute");
    }

    // what this catches (2026-09-19, the M5 starved to one lane while running two): the
    // host prompt cache the daemon granted the engine fills the same anonymous footprint
    // as the KV and is NOT per-token cost. The live shape: 20.4 GB footprint at
    // 2 × 67,584 under a 19,519 MiB `--cache-ram` — nothing is attributable (the
    // estimate rules), and a record left from before the grant was subtracted RETIRES on
    // that sample instead of ruling the plan from disk for an hour. Under a cache the
    // process has outgrown, what lies beyond it is attributed — a lower bound, upward-only.
    #[test]
    fn the_prompt_cache_the_daemon_granted_is_not_kv_and_an_unattributable_sample_retires_the_record() {
        let mib = 1u64 << 20;
        let (anon, lanes, window, compute) = (20_446_525_320u64, 2u32, 67_584u32, 512 * mib);
        assert_eq!(per_token_from(anon, lanes, window, compute, 19_519 * mib), None, "the cache can explain the whole footprint");
        let inflated = per_token_from(anon, lanes, window, compute, 0).expect("without the cache term it reads as KV");
        assert!(inflated > 130_000, "{inflated} B/token — the reading that starved the M5");
        let beyond = per_token_from(anon, lanes, window, compute, 4_096 * mib).expect("beyond a cache the process outgrew");
        assert!(beyond < inflated && beyond > 32_768, "{beyond}: less than the inflated read, still a real excess");

        let mut costs = BTreeMap::new();
        let stale = MeasuredCost { per_token_bytes: 133_720, lanes, window, anon_bytes: anon, last_ms: 1 };
        assert!(apply_sample(&mut costs, "m", Some(&stale)), "first record is a move");
        assert!(apply_sample(&mut costs, "m", None), "an unattributable sample retires it — a save is due");
        assert!(costs.get("m").is_none(), "nothing rules the plan from disk");
        assert!(!apply_sample(&mut costs, "m", None), "already retired: nothing moved");
    }

    // what this catches: the record survives a reboot by its last write and a corrupt
    // file is forgotten, never trusted.
    #[test]
    fn the_record_round_trips_and_a_corrupt_one_is_forgotten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("lane-footprint.json");
        let mut costs = BTreeMap::new();
        costs.insert("m".to_string(), MeasuredCost { per_token_bytes: 36_000, lanes: 8, window: 51_712, anon_bytes: 25_700_000_000, last_ms: 5 });
        save_to(&path, &costs);
        assert_eq!(load_from(&path), costs);
        std::fs::write(&path, b"{not json").expect("write");
        assert!(load_from(&path).is_empty());
    }

    // what this catches: reading our own process attributes a positive anonymous
    // footprint on the platforms that can look; a dead pid is "could not look", not zero.
    #[test]
    fn our_own_anonymous_footprint_is_positive_and_a_dead_pid_is_none() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let own = anon_footprint_of(std::process::id()).expect("own pid readable");
            assert!(own > 0);
        }
        assert_eq!(anon_footprint_of(u32::MAX - 1), None);
    }
}
