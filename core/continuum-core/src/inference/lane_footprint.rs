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
//! footprint (macOS `proc_pid_rusage` phys_footprint: dirty + compressed, mapped files
//! excluded — llama.cpp mmaps the weights, so this is everything BUT the weights; Linux
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

/// The plan's own decomposition, inverted: `anon = lanes × (compute_floor + per_token ×
/// window)` → per_token. `None` when the shape is degenerate or the process holds less
/// than its compute floors (nothing to attribute to tokens).
pub fn per_token_from(anon_bytes: u64, lanes: u32, window: u32, compute_floor_per_lane: u64) -> Option<u64> {
    if lanes == 0 || window == 0 {
        return None;
    }
    let per_lane = anon_bytes / lanes as u64;
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
pub fn observe(model: &str, lanes: u32, window: u32, anon_bytes: u64, compute_floor_per_lane: u64) -> Option<u64> {
    let per_token = per_token_from(anon_bytes, lanes, window, compute_floor_per_lane)?;
    let now = now_ms();
    let mut costs = COSTS.lock();
    let before = costs.get(model).map(|c| c.per_token_bytes).unwrap_or(0);
    costs.insert(
        model.to_string(),
        MeasuredCost { per_token_bytes: per_token, lanes, window, anon_bytes, last_ms: now },
    );
    use std::sync::atomic::Ordering;
    let moved = before == 0 || corrects(before, per_token) || corrects(per_token, before);
    if moved || now.saturating_sub(LAST_SAVE_MS.load(Ordering::Relaxed)) >= SAVE_EVERY_MS {
        save_all(&costs);
        LAST_SAVE_MS.store(now, Ordering::Relaxed);
    }
    Some(per_token)
}

/// The fresh measured per-token cost for `model`, if any.
pub fn measured_per_token(model: &str) -> Option<u64> {
    let now = now_ms();
    COSTS.lock().get(model).filter(|c| c.fresh_at(now)).map(|c| c.per_token_bytes)
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
    // struct rusage_info_v0: ri_uuid[16], then ten u64 — ri_phys_footprint is the 8th
    // u64 (index 7): user_time, system_time, pkg_idle_wkups, interrupt_wkups, pageins,
    // wired_size, resident_size, PHYS_FOOTPRINT, proc_start_abstime, proc_exit_abstime.
    #[repr(C)]
    struct RusageInfoV0 {
        uuid: [u8; 16],
        fields: [u64; 10],
    }
    extern "C" {
        fn proc_pid_rusage(pid: libc::c_int, flavor: libc::c_int, buffer: *mut RusageInfoV0) -> libc::c_int;
    }
    const RUSAGE_INFO_V0: libc::c_int = 0;
    let mut info = RusageInfoV0 { uuid: [0; 16], fields: [0; 10] };
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v0; libproc only
    // writes within it for flavor V0.
    let rc = unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V0, &mut info) };
    (rc == 0).then_some(info.fields[7])
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

    // what this catches (2026-09-17, the M5 swapping at 8 × 51,712): the plan's per-token
    // cost is derived from the process it launched with the plan's own decomposition, and
    // only a measurement materially ABOVE the estimate corrects it — a footprint is a
    // lower bound, so a smaller reading never shrinks the plan's reserve.
    #[test]
    fn the_measured_cost_inverts_the_plans_decomposition_and_only_corrects_upward() {
        let gb = 1u64 << 30;
        // The live shape: 25.7 GB anonymous, 8 lanes × 51,712, compute floor 1.36 GB/lane.
        let per_token = per_token_from(25_700_000_000, 8, 51_712, 1_357_000_000).expect("attributable");
        assert!((35_000..38_000).contains(&per_token), "{per_token} B/token");
        assert!(corrects(10_240, per_token), "3.5× the estimate is a correction");
        assert!(!corrects(per_token, 10_240), "a smaller reading never corrects downward");
        assert!(!corrects(10_240, 12_000), "17% is agreement, not a correction");
        // Degenerate shapes attribute nothing.
        assert_eq!(per_token_from(gb, 0, 51_712, 0), None);
        assert_eq!(per_token_from(gb, 2, 0, 0), None);
        assert_eq!(per_token_from(gb, 2, 4096, gb), None, "under the compute floors: nothing beyond them to attribute");
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
