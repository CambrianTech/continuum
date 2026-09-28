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

/// One engine process: its pid, its OS start time, and the port it serves. `started_s == 0`
/// is a record from before the field and never matches a live process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineIncarnation {
    pub pid: u32,
    /// OS process start time, seconds since the Unix epoch (sysinfo's `start_time`).
    pub started_s: u64,
    pub port: u16,
}

/// Is this incarnation the process running now?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// A process with this pid exists and started at this time.
    Alive,
    /// No such process, or the pid belongs to a process started at another time.
    Dead,
}

/// The OS start time of `pid`, seconds since the epoch, or `None` when no such process exists.
pub fn process_start_s(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    if pid == 0 {
        return None;
    }
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(target).map(|p| p.start_time())
}

impl EngineIncarnation {
    /// The incarnation of the process `pid` serving `port` right now.
    pub fn of(pid: u32, port: u16) -> Option<Self> {
        process_start_s(pid).map(|started_s| Self {
            pid,
            started_s,
            port,
        })
    }

    /// Alive only when the same pid exists with the same start time; anything else is death
    /// (or recycling, which is the same fact for the process this names).
    pub fn liveness_with(&self, start_of: impl Fn(u32) -> Option<u64>) -> Liveness {
        match start_of(self.pid) {
            Some(started) if self.started_s != 0 && started == self.started_s => Liveness::Alive,
            _ => Liveness::Dead,
        }
    }

    pub fn liveness(&self) -> Liveness {
        self.liveness_with(process_start_s)
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

/// Record live work. Refuses (never overwrites) when the store cannot be read.
pub fn record(path: &Path, work: ResidentWork) -> Result<(), String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut all = read(path)?;
    all.retain(|w| w.job != work.job);
    all.push(work);
    write(path, &all)
}

/// Release one job's record: its engine acknowledged a terminal state, or its incarnation is
/// verifiably dead. Returns whether a record was there.
pub fn release(path: &Path, job: Uuid) -> Result<bool, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut all = read(path)?;
    let before = all.len();
    all.retain(|w| w.job != job);
    if all.len() == before {
        return Ok(false);
    }
    write(path, &all)?;
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

/// THE QUESTION, with liveness injected: may serving disturb the engine serving `port`? Work
/// bound to an incarnation that is verifiably dead is released on the way (its death is the
/// release evidence). Work on another port is not this engine's.
pub fn occupancy_with(path: &Path, port: u16, start_of: &dyn Fn(u32) -> Option<u64>) -> Occupancy {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let all = match read(path) {
        Ok(all) => all,
        Err(why) => return Occupancy::Unknown(why),
    };
    let (dead, live): (Vec<ResidentWork>, Vec<ResidentWork>) = all
        .into_iter()
        .partition(|w| w.engine.liveness_with(start_of) == Liveness::Dead);
    if !dead.is_empty() {
        if let Err(why) = write(path, &live) {
            // could not release: the dead records stand, and so does the hold
            return Occupancy::Unknown(why);
        }
        for w in &dead {
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
    occupancy_with(path, port, &process_start_s)
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
            base_model: "qwen3-27b".into(),
            created_ms: 1,
            interrupted: None,
        }
    }

    // what this catches: Kimi's attempt 2. Serving replaced an engine that hosted a live run
    // because nothing recorded the binding. A recorded run holds its OWN incarnation's port and
    // no other; a recycled pid (same number, other start time) is death, and death releases
    // the record without any acknowledgment; an unreadable store holds rather than guesses.
    #[test]
    fn resident_work_holds_its_incarnation_until_it_is_verifiably_dead() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = store_path(dir.path());
        assert_eq!(
            occupancy_with(&path, 58057, &|_| None),
            Occupancy::Free,
            "no file, no work"
        );

        record(&path, work(1, 25280, 1_000, 58057)).expect("record");
        let alive = |pid: u32| (pid == 25280).then_some(1_000);
        assert!(
            matches!(occupancy_with(&path, 58057, &alive), Occupancy::Resident(ref w) if w.len() == 1)
        );
        assert_eq!(
            occupancy_with(&path, 58058, &alive),
            Occupancy::Free,
            "another port is another engine"
        );

        // the pid now belongs to a process started later: not the engine the run lived in
        let recycled = |pid: u32| (pid == 25280).then_some(2_000);
        assert_eq!(
            occupancy_with(&path, 58057, &recycled),
            Occupancy::Free,
            "a recycled pid is death"
        );
        assert!(
            read(&path).expect("read").is_empty(),
            "death released the record"
        );

        record(&path, work(2, 25280, 1_000, 58057)).expect("record");
        assert!(
            release(&path, Uuid::from_u128(2)).expect("release"),
            "an acknowledged end releases"
        );
        assert!(!release(&path, Uuid::from_u128(2)).expect("release"));

        // an emergency replacement tells the work BEFORE it commits
        record(&path, work(4, 25280, 1_000, 58057)).expect("record");
        assert_eq!(
            interrupt(&path, 58057, "kv paging unverified").expect("interrupt"),
            vec![Uuid::from_u128(4)]
        );
        assert_eq!(
            interruption_of(&path, Uuid::from_u128(4)).as_deref(),
            Some("kv paging unverified")
        );
        release(&path, Uuid::from_u128(4)).expect("release");

        std::fs::write(&path, b"{not a list").expect("corrupt");
        assert!(
            matches!(occupancy_with(&path, 58057, &alive), Occupancy::Unknown(_)),
            "unreadable holds"
        );
        assert!(occupancy_with(&path, 58057, &alive).holds());
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

    // what this catches: an incarnation matched by pid alone. A record from before the start
    // time existed (0) must never match, or every legacy record would read as a live engine.
    #[test]
    fn an_incarnation_matches_only_the_same_process_start() {
        let inc = EngineIncarnation {
            pid: 7,
            started_s: 100,
            port: 1,
        };
        assert_eq!(inc.liveness_with(|_| Some(100)), Liveness::Alive);
        assert_eq!(inc.liveness_with(|_| Some(101)), Liveness::Dead);
        assert_eq!(inc.liveness_with(|_| None), Liveness::Dead);
        let legacy = EngineIncarnation {
            started_s: 0,
            ..inc
        };
        assert_eq!(legacy.liveness_with(|_| Some(0)), Liveness::Dead);
    }
}
