//! Which ENGINE INCARNATION hosts which live work, as a fact that outlives the core
//! (docs/architecture/SHARED-RESIDENT-LIFECYCLE.md, step 1; card ef9df13b).
//!
//! Kimi's attempt 2 (5090, 2026-09-28 12:29–12:30Z, job 4169e970) died because nothing told
//! serving that the lane it replaced hosted a live `/train` run: the planner downshifted the
//! 27B, the swap killed the engine, and the run with it. Training admission already waited on
//! serving's lifecycle gate, but only for the moment of admission; after that the memory lease
//! kept serving out of the MEMORY and nothing kept it out of the ENGINE.
//!
//! This module is that missing record, and nothing else:
//!
//! - [`EngineIncarnation`]: one engine PROCESS, not an endpoint. A port or a pid alone is
//!   reused; a pid plus the OS process start time is not. A successor core compares both, so
//!   adoption verifies the same process instead of trusting a freshly minted generation.
//! - [`ResidentWork`]: one live piece of work bound to one incarnation, persisted in
//!   `state/engine-residency.json` so a relaunched core reads it before it plans (step 3).
//! - The store is FAIL-CLOSED, the `training_hold_store` shape (#4522): a missing file is no
//!   work; an unreadable or corrupt one is an `Err`, and every destructive caller reads `Err`
//!   as "held". A record is never guessed away.
//! - [`Occupancy`]: the one question serving asks before it replaces or measures an engine.
//!   A record whose incarnation is VERIFIABLY dead (no such process, or the pid now belongs to
//!   a process started at another time) is released right there: an engine that no longer
//!   exists cannot acknowledge anything, and its death is the release evidence.
//!
//! Who writes and releases it is decided elsewhere: the record is written inside training
//! admission's hold (`forge::training_admission`), and released only on the engine's own
//! terminal or cancel acknowledgment or on verified death. A dropped controller future, a
//! lost HTTP connection or a timeout releases nothing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One engine process: its pid, its OS start time, and the port it serves. A binding is only
/// ever recorded with a NONZERO start time; `started_s == 0` (a lane record from before the
/// field) cannot be verified either way and reads as [`Liveness::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineIncarnation {
    pub pid: u32,
    /// OS process start time, seconds since the Unix epoch (sysinfo's `start_time`).
    pub started_s: u64,
    pub port: u16,
}

/// Is this incarnation the process running now? THREE answers, not two (Codex on #4531): an OS
/// inspection that could not read the process is not proof that it died.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// A process with this pid exists and started at exactly this time.
    Alive,
    /// VERIFIED gone: no process has this pid, or one does and it started at a different
    /// (readable, nonzero) time, i.e. the pid was reused.
    Dead,
    /// The process table could not answer (a start time it cannot read, a legacy record with
    /// no start time). Held, never released.
    Unknown,
}

/// What the OS says about one pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessProbe {
    /// A process with this pid exists; its start time, or 0 when it cannot be read.
    Present(u64),
    /// POSITIVELY absent: the kernel said no such process.
    Absent,
    /// The OS could not answer either way. Never read as absence (Codex on #4531).
    Unreadable,
}

/// Ask the OS about `pid`. The process table answers when it lists the pid. When it does not,
/// absence needs POSITIVE evidence from the kernel, and every other failure is `Unreadable`:
/// - Unix: `kill(pid, 0)` returning `ESRCH` is absence; success or `EPERM` means it exists
///   (another user's); any other error is unreadable.
/// - Windows: `OpenProcess` failing with `ERROR_INVALID_PARAMETER` is absence (no such pid);
///   a handle means it exists; any other failure (access denied, …) is unreadable.
pub fn probe_process(pid: u32) -> ProcessProbe {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    if pid == 0 {
        return ProcessProbe::Absent;
    }
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    if let Some(p) = sys.process(target) {
        return ProcessProbe::Present(p.start_time());
    }
    kernel_says(pid)
}

#[cfg(unix)]
fn kernel_says(pid: u32) -> ProcessProbe {
    // SAFETY: signal 0 performs only the existence and permission check; nothing is sent.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return ProcessProbe::Present(0);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => ProcessProbe::Absent,
        Some(libc::EPERM) => ProcessProbe::Present(0),
        _ => ProcessProbe::Unreadable,
    }
}

#[cfg(windows)]
fn kernel_says(pid: u32) -> ProcessProbe {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: a query-only handle for exactly this pid, closed once below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if !handle.is_null() {
        // SAFETY: `handle` came from a successful OpenProcess and is closed once, here.
        unsafe { CloseHandle(handle) };
        return ProcessProbe::Present(0);
    }
    // SAFETY: reads this thread's last error, set by the failed OpenProcess above.
    if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
        ProcessProbe::Absent
    } else {
        ProcessProbe::Unreadable
    }
}

#[cfg(not(any(unix, windows)))]
fn kernel_says(_pid: u32) -> ProcessProbe {
    ProcessProbe::Unreadable
}

/// The OS start time of `pid` (nonzero), or `None` when the process is absent or its start
/// time cannot be read. A binding is only recorded from a `Some`.
pub fn process_start_s(pid: u32) -> Option<u64> {
    match probe_process(pid) {
        ProcessProbe::Present(s) if s != 0 => Some(s),
        _ => None,
    }
}

impl EngineIncarnation {
    /// The incarnation of the process `pid` serving `port` right now, only when its start time
    /// is readable (a binding without one could never be verified or released).
    pub fn of(pid: u32, port: u16) -> Option<Self> {
        process_start_s(pid).map(|started_s| Self {
            pid,
            started_s,
            port,
        })
    }

    pub fn liveness_with(&self, probe: impl Fn(u32) -> ProcessProbe) -> Liveness {
        if self.started_s == 0 {
            return Liveness::Unknown;
        }
        match probe(self.pid) {
            ProcessProbe::Absent => Liveness::Dead,
            ProcessProbe::Unreadable | ProcessProbe::Present(0) => Liveness::Unknown,
            ProcessProbe::Present(s) if s == self.started_s => Liveness::Alive,
            ProcessProbe::Present(_) => Liveness::Dead, // the pid was reused by a later process
        }
    }

    pub fn liveness(&self) -> Liveness {
        self.liveness_with(probe_process)
    }

    /// The root url every in-process client addresses this engine by.
    pub fn root_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// One live piece of work bound to the engine incarnation that hosts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentWork {
    /// The job (`JobHandle::local_id`).
    pub job: Uuid,
    /// The engine's own name for the run (`/train`'s `out`).
    pub out: String,
    pub engine: EngineIncarnation,
    pub base_model: String,
    pub created_ms: u64,
    /// The governed reservation this work holds (the resource consumer and its bytes), so a
    /// successor core re-accounts exactly that allocation instead of re-admitting (step 3).
    #[serde(default)]
    pub consumer: String,
    #[serde(default)]
    pub reserved_bytes: u64,
    /// Set when serving had to replace this work's engine anyway (an emergency it may not
    /// wait out), BEFORE the replacement commits, so the run's failure names the reason
    /// instead of it being reconstructed after the engine is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<String>,
}

/// The store's file under a continuum home.
pub fn store_path(home: &Path) -> PathBuf {
    home.join("state").join("engine-residency.json")
}

/// Serializes read-modify-write within this process.
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn read(path: &Path) -> Result<Vec<ResidentWork>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("{} is not a residency list: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()), // no file = no resident work
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write(path: &Path, all: &[ResidentWork]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let body = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Two records name the same binding: the same job, the same engine run, the same
/// incarnation. What `release` fences on, so a stale controller can never remove a successor's
/// binding for the same job (Codex on #4531).
fn same_binding(a: &ResidentWork, b: &ResidentWork) -> bool {
    a.job == b.job && a.out == b.out && a.engine == b.engine
}

/// Record live work. Idempotent for the same binding; REFUSES a different binding for a job
/// that is already bound (a rebind is its own decision, never an overwrite), and refuses when
/// the store cannot be read.
pub fn record(path: &Path, work: ResidentWork) -> Result<(), String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut all = read(path)?;
    if let Some(existing) = all.iter().find(|w| w.job == work.job) {
        if same_binding(existing, &work) {
            return Ok(());
        }
        return Err(format!(
            "job {} is already bound to engine pid {} (out {}); not overwriting it with pid {} (out {})",
            work.job, existing.engine.pid, existing.out, work.engine.pid, work.out
        ));
    }
    all.push(work);
    write(path, &all)?;
    // work is bound: a footprint sample begun before this may straddle it, and is refused
    super::lane_footprint::residency_changed();
    Ok(())
}

/// When this process last released resident work (unix ms, 0 = never), for
/// [`released_within`].
static LAST_RELEASE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// When this process first asked [`released_within`]. A predecessor core may have released work
/// moments before it exited, and this process cannot know (Codex on #4536), so its start counts
/// as a release: the first settle window is waited out, never assumed clean.
static FIRST_ASKED_MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// Work just left its engine (step 2, attribution). Two consequences, in one place:
/// - the model's measured footprint record is RETIRED if it was sampled while the work was
///   bound (`last_ms >= created_ms`): that reading may carry the work's allocations. A record
///   from before the work is the last clean measurement and stays (the next clean sample
///   replaces a retired one);
/// - the release time is noted, so the sampler waits out the engine's asynchronous frees and
///   the device's measurement lag before it reads the lane again ([`released_within`]).
fn on_released(work: &ResidentWork) {
    // first: a footprint sample in flight across the work's residency is refused
    super::lane_footprint::residency_changed();
    LAST_RELEASE_MS.store(crate::persona::trace::now_ms(), std::sync::atomic::Ordering::Relaxed);
    let retired = super::lane_footprint::retire_if_sampled_since(&work.base_model, work.created_ms);
    crate::probe!(
        class = "serving.residency.released",
        job = %work.job,
        base = work.base_model.as_str(),
        footprint_retired = retired,
        "resident work left its engine: the model's footprint record is retired until a clean sample"
    );
}

/// Did this process release resident work within `window_ms` of `now_ms`?
/// The first call in a process counts as a release (see [`FIRST_ASKED_MS`]).
pub fn released_within(now_ms: u64, window_ms: u64) -> bool {
    let first = *FIRST_ASKED_MS.get_or_init(|| now_ms);
    let last = LAST_RELEASE_MS.load(std::sync::atomic::Ordering::Relaxed).max(first);
    now_ms.saturating_sub(last) < window_ms
}

/// Release EXACTLY `expected`: its engine acknowledged a terminal state for this run, or its
/// incarnation is verifiably dead. A record for the same job with a different binding (a
/// successor) is left alone. Returns whether the binding was there.
pub fn release(path: &Path, expected: &ResidentWork) -> Result<bool, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut all = read(path)?;
    let before = all.len();
    all.retain(|w| !same_binding(w, expected));
    if all.len() == before {
        return Ok(false);
    }
    write(path, &all)?;
    on_released(expected);
    Ok(true)
}

/// Record that serving is replacing the engine on `port` under its live work, and why. Returns
/// the jobs told. Refuses when the store cannot be read (the replacement then has no record
/// to cite, and the caller says so).
pub fn interrupt(path: &Path, port: u16, reason: &str) -> Result<Vec<Uuid>, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut all = read(path)?;
    let mut told = Vec::new();
    for w in all.iter_mut().filter(|w| w.engine.port == port) {
        w.interrupted = Some(reason.to_string());
        told.push(w.job);
    }
    if !told.is_empty() {
        write(path, &all)?;
    }
    Ok(told)
}

/// Why serving replaced this job's engine under it, if it did.
pub fn interruption_of(path: &Path, job: Uuid) -> Option<String> {
    read(path)
        .ok()?
        .into_iter()
        .find(|w| w.job == job)
        .and_then(|w| w.interrupted)
}

/// What serving may do to the engine on `port` right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Occupancy {
    /// No live work on it: replace or measure freely.
    Free,
    /// Live work is bound to this very incarnation: do not replace it, do not read its
    /// footprint as serving cost.
    Resident(Vec<ResidentWork>),
    /// The record cannot be read, so ownership is unknown: treated as held.
    Unknown(String),
}

impl Occupancy {
    /// True when a destructive transition on this engine must not proceed.
    pub fn holds(&self) -> bool {
        !matches!(self, Occupancy::Free)
    }
}

/// THE QUESTION, with the process table injected: may serving disturb the engine serving
/// `port`? Work bound to an incarnation that is VERIFIABLY dead is released on the way (its
/// death is the release evidence); work whose engine cannot be verified either way is held,
/// never released. Work on another port is not this engine's.
pub fn occupancy_with(path: &Path, port: u16, probe: &dyn Fn(u32) -> ProcessProbe) -> Occupancy {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let all = match read(path) {
        Ok(all) => all,
        Err(why) => return Occupancy::Unknown(why),
    };
    let (dead, live): (Vec<ResidentWork>, Vec<ResidentWork>) = all
        .into_iter()
        .partition(|w| w.engine.liveness_with(probe) == Liveness::Dead);
    if !dead.is_empty() {
        if let Err(why) = write(path, &live) {
            // could not release: the dead records stand, and so does the hold
            return Occupancy::Unknown(why);
        }
        for w in &dead {
            on_released(w);
            crate::probe!(
                class = "serving.residency.released_on_death",
                job = %w.job,
                pid = w.engine.pid as u64,
                port = w.engine.port as u64,
                "resident work released: its engine incarnation is verifiably gone"
            );
        }
    }
    let here: Vec<ResidentWork> = live.into_iter().filter(|w| w.engine.port == port).collect();
    if here.is_empty() {
        Occupancy::Free
    } else {
        Occupancy::Resident(here)
    }
}

/// [`occupancy_with`] against the real process table.
pub fn occupancy(path: &Path, port: u16) -> Occupancy {
    occupancy_with(path, port, &probe_process)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(job: u128, pid: u32, started_s: u64, port: u16) -> ResidentWork {
        ResidentWork {
            job: Uuid::from_u128(job),
            out: format!("{job}.gguf"),
            engine: EngineIncarnation {
                pid,
                started_s,
                port,
            },
            // never a real model: a release retires the live footprint record of this name
            base_model: "residency-test-base".into(),
            created_ms: 1,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 1 << 30,
            interrupted: None,
        }
    }

    // what this catches: Kimi's attempt 2 (serving replaced an engine hosting a live run, with
    // no record of the binding), and Codex's fence on #4531. A recorded run holds its OWN
    // incarnation's port and no other; a reused pid (a readable, different start) is death and
    // releases the record; an unreadable store holds; a release removes only its exact binding.
    #[test]
    fn resident_work_holds_its_incarnation_until_it_is_verifiably_dead() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = store_path(dir.path());
        let gone = |_: u32| ProcessProbe::Absent;
        assert_eq!(
            occupancy_with(&path, 58057, &gone),
            Occupancy::Free,
            "no file, no work"
        );

        record(&path, work(1, 25280, 1_000, 58057)).expect("record");
        let alive = |pid: u32| {
            if pid == 25280 {
                ProcessProbe::Present(1_000)
            } else {
                ProcessProbe::Absent
            }
        };
        assert!(
            matches!(occupancy_with(&path, 58057, &alive), Occupancy::Resident(ref w) if w.len() == 1)
        );
        assert_eq!(
            occupancy_with(&path, 58058, &alive),
            Occupancy::Free,
            "another port is another engine"
        );

        // the pid now belongs to a process started later: not the engine the run lived in
        let reused = |pid: u32| {
            if pid == 25280 {
                ProcessProbe::Present(2_000)
            } else {
                ProcessProbe::Absent
            }
        };
        assert_eq!(
            occupancy_with(&path, 58057, &reused),
            Occupancy::Free,
            "a reused pid is verified death"
        );
        assert!(
            read(&path).expect("read").is_empty(),
            "death released the record"
        );

        // a release is FENCED on the exact binding: a stale controller cannot remove a successor
        let first = work(2, 25280, 1_000, 58057);
        record(&path, first.clone()).expect("record");
        assert!(
            record(&path, work(2, 30988, 3_000, 58057)).is_err(),
            "a different binding for the job is refused, never overwritten"
        );
        let stale = ResidentWork {
            out: "someone-else.gguf".into(),
            ..first.clone()
        };
        assert!(
            !release(&path, &stale).expect("release"),
            "a stale binding releases nothing"
        );
        assert!(
            release(&path, &first).expect("release"),
            "the exact binding releases"
        );
        assert!(!release(&path, &first).expect("release"));

        // an emergency replacement tells the work BEFORE it commits
        let w4 = work(4, 25280, 1_000, 58057);
        record(&path, w4.clone()).expect("record");
        assert_eq!(
            interrupt(&path, 58057, "kv paging unverified").expect("interrupt"),
            vec![Uuid::from_u128(4)]
        );
        assert_eq!(
            interruption_of(&path, Uuid::from_u128(4)).as_deref(),
            Some("kv paging unverified")
        );
        let told = ResidentWork {
            interrupted: Some("kv paging unverified".into()),
            ..w4
        };
        assert!(release(&path, &told).expect("release"));

        std::fs::write(&path, b"{not a list").expect("corrupt");
        assert!(
            matches!(occupancy_with(&path, 58057, &alive), Occupancy::Unknown(_)),
            "unreadable holds"
        );
        assert!(
            record(&path, work(3, 1, 1, 1)).is_err(),
            "a write refuses over unreadable state"
        );
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"{not a list",
            "left for a human"
        );
    }

    // what this catches: step 2 (attribution). A footprint read while training was resident
    // carries the training allocation; left standing it rules the next plan as serving cost (the
    // 5090's ~7.4 GB "fixed per-lane residency"). On release, a record sampled during the work
    // is retired, one from before it (the last clean measurement) stays (Fable on #4536), and
    // the release is noted for the sampler's settle. The records are a local map: this test
    // never writes the live lane-footprint store.
    #[test]
    fn a_release_retires_only_the_footprint_sampled_during_the_work() {
        use super::super::lane_footprint::{retire_sampled_since, MeasuredCost};
        let bound_at = 1_000;
        let reading = |last_ms| MeasuredCost { per_token_bytes: 36_000, lanes: 1, window: 32_768, anon_bytes: 20 << 30, last_ms };
        let mut costs = std::collections::BTreeMap::new();
        costs.insert("during".to_string(), reading(bound_at + 60_000));
        costs.insert("before".to_string(), reading(bound_at - 1));
        assert!(retire_sampled_since(&mut costs, "during", bound_at), "sampled while bound: retired");
        assert!(!retire_sampled_since(&mut costs, "before", bound_at), "the last clean record stays");
        assert!(costs.contains_key("before") && !costs.contains_key("during"));

        let dir = tempfile::tempdir().expect("tempdir");
        let path = store_path(dir.path());
        let w = work(9, 25280, 1_000, 58057);
        record(&path, w.clone()).expect("record");
        assert!(release(&path, &w).expect("release"));
        assert!(released_within(crate::persona::trace::now_ms(), 60_000), "noted for the sampler's settle");
    }

    // what this catches (Codex on #4536): a footprint sample that checked occupancy while the
    // engine was free, then measured (awaiting) while work bound and trained, publishing the
    // contaminated reading, or one in flight across a release writing it back after the
    // release retired the record. Binding and releasing advance the residency epoch; a sample
    // publishes only under the epoch it began with.
    #[test]
    fn a_sample_that_straddles_a_residency_change_is_never_published() {
        use super::super::lane_footprint::{begin_sample, publish_sample, MeasuredCost};
        let reading = MeasuredCost { per_token_bytes: 36_000, lanes: 1, window: 32_768, anon_bytes: 20 << 30, last_ms: 5 };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = store_path(dir.path());
        let w = work(10, 25281, 1_000, 58058);

        // a sample begins with the engine free, then work binds while it measures
        let before_bind = begin_sample();
        record(&path, w.clone()).expect("record");
        let after_bind = begin_sample();
        assert!(after_bind.0 > before_bind.0, "binding advances the epoch");
        let mut costs = std::collections::BTreeMap::new();
        assert!(publish_sample(&mut costs, after_bind.0, before_bind, "m", Some(&reading)).is_err());
        assert!(costs.is_empty(), "the straddling reading is dropped");

        // a sample in flight across the release
        assert!(release(&path, &w).expect("release"));
        let after_release = begin_sample();
        assert!(after_release.0 > after_bind.0, "releasing advances the epoch");
        assert!(publish_sample(&mut costs, after_release.0, after_bind, "m", Some(&reading)).is_err());
        assert!(costs.is_empty(), "nothing written back after the release");

        // control: a sample whose residency held publishes
        assert_eq!(publish_sample(&mut costs, after_release.0, after_release, "m", Some(&reading)), Ok(true));
        assert!(costs.contains_key("m"));
    }

    // what this catches (Codex on #4531): an OS inspection failure read as death. An unreadable
    // start time and a legacy record with none are UNKNOWN, and unknown work is held, never
    // released; only absence or a readable different start is death.
    #[test]
    fn an_unreadable_process_is_unknown_and_held_never_dead() {
        let inc = EngineIncarnation {
            pid: 7,
            started_s: 100,
            port: 1,
        };
        assert_eq!(
            inc.liveness_with(|_| ProcessProbe::Present(100)),
            Liveness::Alive
        );
        assert_eq!(
            inc.liveness_with(|_| ProcessProbe::Present(101)),
            Liveness::Dead,
            "reused pid"
        );
        assert_eq!(
            inc.liveness_with(|_| ProcessProbe::Absent),
            Liveness::Dead,
            "no such process"
        );
        assert_eq!(
            inc.liveness_with(|_| ProcessProbe::Present(0)),
            Liveness::Unknown,
            "start unreadable"
        );
        assert_eq!(
            inc.liveness_with(|_| ProcessProbe::Unreadable),
            Liveness::Unknown,
            "the OS could not answer: never death"
        );
        let legacy = EngineIncarnation {
            started_s: 0,
            ..inc
        };
        assert_eq!(
            legacy.liveness_with(|_| ProcessProbe::Present(0)),
            Liveness::Unknown,
            "legacy record"
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let path = store_path(dir.path());
        record(&path, work(5, 7, 100, 1)).expect("record");
        let unreadable = |_: u32| ProcessProbe::Present(0);
        assert!(
            occupancy_with(&path, 1, &unreadable).holds(),
            "unknown liveness holds the engine"
        );
        assert_eq!(read(&path).expect("read").len(), 1, "and releases nothing");
    }
}
