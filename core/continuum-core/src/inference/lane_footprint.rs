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

/// The RESIDENCY EPOCH (Codex on #4536): advanced whenever work binds to an engine or leaves
/// one. A sample takes a [`SampleToken`] BEFORE it checks occupancy, measures (which may
/// await), and publishes only if the epoch has not moved. The advance and the publication both
/// hold the records' lock, so a publication is wholly before a residency change or sees it.
/// Binding happens inside the admission hold before POST /train, so no training byte is
/// allocated under an epoch a sample could still publish against.
static RESIDENCY_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A sample's claim on the residency it began under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleToken(pub(crate) u64);

/// Work bound to or left an engine did not publish: residency changed while it was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidencyChanged;

/// Take the token a sample publishes against. Taken before the occupancy check.
pub fn begin_sample() -> SampleToken {
    SampleToken(RESIDENCY_EPOCH.load(std::sync::atomic::Ordering::SeqCst))
}

/// Residency changed: work bound to an engine, or left one. Any sample begun before this
/// is refused at publication.
pub fn residency_changed() {
    let _records = COSTS.lock();
    RESIDENCY_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Publish a sample against the records IF the residency it began under still stands (`epoch`
/// is the current residency epoch, read under the records' lock). Pure, on the map it is given.
/// Returns whether the record moved, as [`apply_sample`].
pub(crate) fn publish_sample(
    costs: &mut BTreeMap<String, MeasuredCost>,
    epoch: u64,
    token: SampleToken,
    model: &str,
    sample: Option<&MeasuredCost>,
) -> Result<bool, ResidencyChanged> {
    if epoch != token.0 {
        return Err(ResidencyChanged);
    }
    Ok(apply_sample(costs, model, sample))
}

/// Record one measurement of the live lane. Returns the per-token cost it derived, or
/// [`ResidencyChanged`] when work bound to or left the engine since `token` was taken (the
/// reading may straddle that work, so it is dropped and the record stands).
///
/// The record is REPLACED by each sample (last write wins), never maxed across samples:
/// "upward only" is a rule about the measurement against the ESTIMATE at apply time
/// ([`corrects`]), not a ratchet over the measurement's own history. A reading taken
/// in a bad minute rules for one sample interval, and a record older than [`FRESH_MS`]
/// is ignored entirely — the plan falls back to its arithmetic, never to a remembered
/// maximum.
pub fn observe(
    token: SampleToken,
    model: &str,
    lanes: u32,
    window: u32,
    anon_bytes: u64,
    compute_floor_per_lane: u64,
    host_cache_bytes: u64,
) -> Result<Option<u64>, ResidencyChanged> {
    let now = now_ms();
    let sample = per_token_from(anon_bytes, lanes, window, compute_floor_per_lane, host_cache_bytes)
        .map(|per_token| MeasuredCost { per_token_bytes: per_token, lanes, window, anon_bytes, last_ms: now });
    use std::sync::atomic::Ordering;
    let mut costs = COSTS.lock();
    let moved = publish_sample(&mut costs, RESIDENCY_EPOCH.load(Ordering::SeqCst), token, model, sample.as_ref())?;
    if moved || now.saturating_sub(LAST_SAVE_MS.load(Ordering::Relaxed)) >= SAVE_EVERY_MS {
        save_all(&costs);
        LAST_SAVE_MS.store(now, Ordering::Relaxed);
    }
    Ok(sample.map(|c| c.per_token_bytes))
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

/// Retire the model's record only if it was SAMPLED at or after `since_ms` (Fable on #4536):
/// resident work bound at `since_ms` may be inside such a reading, while a record from before
/// it is the last clean measurement and must stay to rule the plan. Pure, on the map it is
/// given; [`retire_if_sampled_since`] applies it to the live records.
pub(crate) fn retire_sampled_since(costs: &mut BTreeMap<String, MeasuredCost>, model: &str, since_ms: u64) -> bool {
    match costs.get(model) {
        Some(c) if c.last_ms >= since_ms => apply_sample(costs, model, None),
        _ => false,
    }
}

/// [`retire_sampled_since`] on the live records, saved at once like [`retire`].
pub fn retire_if_sampled_since(model: &str, since_ms: u64) -> bool {
    let mut costs = COSTS.lock();
    let gone = retire_sampled_since(&mut costs, model, since_ms);
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
///
/// The RECENT maximum of the process's current reading ([`FOOTPRINT_WINDOW`]): the pager
/// can dip the current figure while nothing was freed (9.0 → 5.9 GB in a minute on a live
/// lane, 2026-09-17), so one reading under-states the need; the most it held in the window
/// does not. Never the LIFETIME peak, which keeps the engine's LOAD forever: on the M5,
/// 2026-10-10 23:34Z, the lifetime peak read 19.02 GB while the running engine held 11 GB,
/// and that 8 GB became a 4.6 GB "fixed" cost per lane that kept the plan at one lane.
pub fn anon_footprint_of(pid: u32) -> Option<u64> {
    let reading = anon_footprint_impl(pid)?;
    let process = (pid, crate::inference::engine_residency::process_start_s(pid).unwrap_or(0)); // unwrap_or: an unreadable start time keys by pid alone, as before
    let mut windows = RECENT_FOOTPRINTS.lock();
    Some(recent_max(&mut windows, process, now_ms(), reading))
}

/// How far back the recent maximum looks: longer than a pager dip (about a minute measured),
/// short enough that a load peak (the first readings after a launch) ages out within a plan
/// window. derived-or-floor: a floor over the measured dip.
pub const FOOTPRINT_WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);
/// Readings kept per process and processes kept: what bounds the window in memory.
const FOOTPRINT_SAMPLES: usize = 64;
const FOOTPRINT_PROCESSES: usize = 8;

/// Each process's recent readings, `(at_ms, bytes)`, keyed by (pid, OS start time) so a
/// recycled pid never inherits another process's peak.
type FootprintWindows = std::collections::VecDeque<((u32, u64), std::collections::VecDeque<(u64, u64)>)>;
static RECENT_FOOTPRINTS: LazyLock<parking_lot::Mutex<FootprintWindows>> =
    LazyLock::new(|| parking_lot::Mutex::new(std::collections::VecDeque::new()));

/// PURE: add `reading` to `process`'s window and return the largest reading still inside
/// [`FOOTPRINT_WINDOW`]. Bounded by [`FOOTPRINT_SAMPLES`] per process and
/// [`FOOTPRINT_PROCESSES`] processes (the least recently read is dropped).
fn recent_max(windows: &mut FootprintWindows, process: (u32, u64), now_ms: u64, reading: u64) -> u64 {
    let horizon = now_ms.saturating_sub(FOOTPRINT_WINDOW.as_millis() as u64);
    let mut ring = match windows.iter().position(|(p, _)| *p == process) {
        Some(i) => windows.remove(i).map(|(_, r)| r).unwrap_or_default(), // unwrap_or_default: position just found it, so this is always Some
        None => std::collections::VecDeque::new(),
    };
    ring.retain(|(at, _)| *at >= horizon);
    ring.push_back((now_ms, reading));
    while ring.len() > FOOTPRINT_SAMPLES {
        ring.pop_front();
    }
    let max = ring.iter().map(|(_, b)| *b).max().unwrap_or(reading); // unwrap_or: the ring holds at least this reading
    windows.push_back((process, ring));
    while windows.len() > FOOTPRINT_PROCESSES {
        windows.pop_front();
    }
    max
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
    // The CURRENT footprint only. The recent maximum over a window is taken by the caller
    // ([`anon_footprint_of`]), never the lifetime peak (index 28), which holds the engine's
    // load and read 19.02 GB against a running 11 GB on the M5 (2026-10-10).
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
    let mut info = RusageInfoV4 { uuid: [0; 16], fields: [0; 35] };
    // SAFETY: the buffer is a correctly sized, writable rusage_info_v4; libproc only
    // writes within it for flavor V4.
    let rc = unsafe { proc_pid_rusage(pid as libc::c_int, RUSAGE_INFO_V4, &mut info) };
    (rc == 0).then(|| info.fields[PHYS_FOOTPRINT])
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

/// The bytes of the model file a lane process was launched with (`-m` / `--model`),
/// read off the process's own argv and stat'd: the weights it maps. `anon_footprint_of`
/// excludes mapped files by design, so this is the other half of what the lane holds.
/// Measured on the IntelMac lane (2026-09-26, card fa21f81f): phys_footprint 6337 MB of
/// KV + compute, while the weights sat as a clean "mapped file" of 935 MB outside it.
/// A split GGUF (`…-00001-of-000NN.gguf`) is summed over its parts. `None` = could not
/// read the argv or stat the file (a dead pid, another platform): never zero.
pub fn model_file_bytes_of(pid: u32) -> Option<u64> {
    let args = process_args(pid)?;
    model_file_bytes(Path::new(model_path_in(&args)?))
}

/// PURE: the value of `-m` / `--model` (or `--model=…`) in an argv.
fn model_path_in(args: &[String]) -> Option<&str> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "-m" || arg == "--model" {
            return it.next().map(String::as_str);
        }
        if let Some(value) = arg.strip_prefix("--model=") {
            return Some(value);
        }
    }
    None
}

/// The bytes of a GGUF on disk, summing every part of a split model.
fn model_file_bytes(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let Some(split) = name.find("-00001-of-") else {
        return std::fs::metadata(path).ok().map(|m| m.len());
    };
    let (stem, rest) = (&name[..split], &name[split + "-00001-of-".len()..]);
    let parts: u32 = rest.strip_suffix(".gguf")?.parse().ok()?;
    let dir = path.parent()?;
    (1..=parts)
        .map(|i| {
            let part = dir.join(format!("{stem}-{i:05}-of-{rest}"));
            std::fs::metadata(part).ok().map(|m| m.len())
        })
        .sum()
}

#[cfg(target_os = "macos")]
fn process_args(pid: u32) -> Option<Vec<String>> {
    // KERN_PROCARGS2: an i32 argc, the exec path, NUL padding, then argc NUL-terminated args.
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = 0;
    // SAFETY: a size query (null buffer) on a valid mib.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: the buffer is `size` writable bytes, as the query returned.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    buf.truncate(size);
    parse_procargs2(&buf)
}

/// PURE: the argv inside a KERN_PROCARGS2 buffer.
#[cfg(any(target_os = "macos", test))]
fn parse_procargs2(buf: &[u8]) -> Option<Vec<String>> {
    let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?) as usize;
    let mut rest = &buf[4..];
    // Skip the exec path and the NUL padding after it.
    let path_end = rest.iter().position(|b| *b == 0)?;
    rest = &rest[path_end..];
    let first = rest.iter().position(|b| *b != 0)?;
    rest = &rest[first..];
    let args: Vec<String> = rest
        .split(|b| *b == 0)
        .take(argc)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    (args.len() == argc).then_some(args)
}

#[cfg(target_os = "linux")]
fn process_args(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_args(_pid: u32) -> Option<Vec<String>> {
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

    // what this catches (the M5, 2026-10-10 23:34Z: the reader returned the LIFETIME peak,
    // 19.02 GB, for an engine holding 11 GB; the load-time 8 GB became a 4.6 GB "fixed" cost
    // per lane and the plan settled at one lane): the footprint is the most the process held
    // RECENTLY. A load peak ages out of the window; a pager dip inside it does not lower the
    // reading; a recycled pid (another start time) never inherits a peak; the map is bounded.
    #[test]
    fn the_footprint_is_a_recent_max_never_the_load_peak() {
        let gb = 1_000_000_000u64;
        let window = FOOTPRINT_WINDOW.as_millis() as u64;
        let mut w = FootprintWindows::new();
        let engine = (22606, 1_700_000_000);
        assert_eq!(recent_max(&mut w, engine, 0, 19 * gb), 19 * gb, "the load peak is what it held then");
        assert_eq!(recent_max(&mut w, engine, 60_000, 11 * gb), 19 * gb, "still inside the window");
        assert_eq!(recent_max(&mut w, engine, window + 60_000, 11 * gb), 11 * gb, "the load peak aged out");
        assert_eq!(recent_max(&mut w, engine, window + 120_000, 7 * gb), 11 * gb, "a pager dip does not lower the reading");
        assert_eq!(recent_max(&mut w, (22606, 1_800_000_000), window + 180_000, 5 * gb), 5 * gb, "a recycled pid starts fresh");
        for pid in 0..(FOOTPRINT_PROCESSES as u32 * 2) {
            recent_max(&mut w, (pid, 1), window + 240_000, gb);
        }
        assert!(w.len() <= FOOTPRINT_PROCESSES, "bounded by processes");
    }

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
    // what this catches: card fa21f81f — the serving credit read only the anonymous half
    // of a lane (phys_footprint excludes mapped files), so the replace-myself budget was
    // short by the engine's own weights and the plan could only shrink. The weights half is
    // read off the lane's argv (`-m`), both spellings and a split GGUF, and read LIVE from a
    // real child process launched with `-m <file>`, so the argv path is proven on this OS.
    #[test]
    fn the_lane_weights_are_the_model_file_its_argv_names_split_parts_summed() {
        let args = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(model_path_in(&args(&["llama-server", "-m", "/w/a.gguf", "--port", "1"])), Some("/w/a.gguf"));
        assert_eq!(model_path_in(&args(&["llama-server", "--model=/w/b.gguf"])), Some("/w/b.gguf"));
        assert_eq!(model_path_in(&args(&["llama-server", "--port", "1"])), None);

        // KERN_PROCARGS2 layout: argc, exec path, NUL padding, then the args.
        let mut buf = 2i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(b"/bin/llama-server\0\0\0-m\0/w/a.gguf\0ENV=1\0");
        assert_eq!(parse_procargs2(&buf), Some(args(&["-m", "/w/a.gguf"])));

        let dir = tempfile::tempdir().expect("test: tempdir");
        let whole = dir.path().join("m.gguf");
        std::fs::write(&whole, vec![0u8; 1000]).unwrap();
        assert_eq!(model_file_bytes(&whole), Some(1000));
        for (i, len) in [(1, 300usize), (2, 200), (3, 100)] {
            std::fs::write(dir.path().join(format!("big-{i:05}-of-00003.gguf")), vec![0u8; len]).unwrap();
        }
        assert_eq!(model_file_bytes(&dir.path().join("big-00001-of-00003.gguf")), Some(600));
        std::fs::remove_file(dir.path().join("big-00003-of-00003.gguf")).unwrap();
        assert_eq!(model_file_bytes(&dir.path().join("big-00001-of-00003.gguf")), None, "a missing part is no reading");

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            // `sh -c '…; true'` keeps sh itself alive (no exec of the last command), so its
            // argv stays ["sh", "-c", …, "sh", "-m", <file>] for the read.
            let child = std::process::Command::new("sh")
                .args(["-c", "sleep 30; true", "sh", "-m"])
                .arg(&whole)
                .spawn();
            let mut child = child.expect("test: spawn a child with -m in its argv");
            std::thread::sleep(std::time::Duration::from_millis(200));
            let read = model_file_bytes_of(child.id());
            let _ = child.kill();
            let _ = child.wait();
            assert_eq!(read, Some(1000), "the live argv names the file and its size is read");
        }
    }

    #[test]
    fn our_own_anonymous_footprint_is_positive_and_a_dead_pid_is_none() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let own = anon_footprint_of(std::process::id()).expect("own pid readable");
            assert!(own > 0);
        }
        assert_eq!(anon_footprint_of(u32::MAX - 1), None);
    }
}
