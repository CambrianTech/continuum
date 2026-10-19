//! Concrete disk eviction pools — wire (2) of task #155.
//!
//! The 2026-07-13 incident, part two: the eviction ECONOMY was already
//! live (PressureBrokerModule ticks `relieve()` every 5 s at boot, and
//! `DiskPressureMonitor` registers as a signal-only pool), but no pool
//! that OWNS deletable disk content was ever registered — so the broker
//! spent the incident emitting the designed "disk hot AND nobody owns
//! the eviction" zero-byte alerts while cargo-target grew to 363 GB.
//! On an operator's machine that's an inconvenience; on a public user's
//! machine it trashes their system. This module is the first concrete
//! owner; each cache class from `standard_tracked_dirs` gets one over
//! time (genome-models and hf-hub need reference-aware LRU and come
//! later; cargo-target is pure derived artifact and comes first).
//!
//! ## Safety invariants (each pinned by a test)
//!
//! 1. **Never race a live build.** Eviction takes non-blocking exclusive
//!    flocks on cargo's own lock files (`<root>/.cargo-lock`,
//!    `<root>/{debug,release}/.cargo-lock`) before touching anything; any lock we
//!    can't get means a build is in flight → free 0 bytes and let the
//!    broker retry next tick.
//! 2. **Never leave the root.** The eviction ladder is a fixed list of
//!    `root.join(...)` subpaths — no pattern matching, no following
//!    symlinks or Windows reparse points out of the tree. Validation and removal
//!    stay under the same Cargo custody locks.
//! 3. **Derived artifacts only.** The ladder deletes incremental state,
//!    test binaries, then the debug tree — all reproducible by the next
//!    `cargo build`. Standard release incremental state is also reclaimable;
//!    release build inputs require an explicit producer static-input declaration.
//!    Generic deps/build and top-level installer inputs remain retained and reported. Custom profiles
//!    and target-triple subtrees are not inferred from arbitrary directory names.

use crate::modules::work::HandsLease;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

use super::disk_reporters::{dir_size_bytes, TrackedDir};

/// FLOOR for the cargo-target budget: 50 GiB. This was the whole budget until
/// 2026-09-08, and on a large developer machine it is not a cache budget at all
/// — it is a scheduled `cargo clean`.
///
/// MEASURED on BigMama (1.9 TB volume), from this pool's own eviction log:
///
/// ```text
/// evictions in one day   36
/// GB freed that day      356
/// median gap             13.5 minutes
/// ```
///
/// This workspace's debug tree passes 50 GiB during an ordinary
/// `cargo test -p continuum-core`, so the pool sat permanently over budget, the
/// broker asked for relief on every tick, and the eviction obliged — forever.
/// The code did exactly what it was told; it was told the wrong number. Those
/// reports of "Windows rustc is flaky" and "~70 minutes to rebuild" arrived the
/// same day, and this is the leading explanation for them — not a demonstrated
/// one. Whether any particular build was denied a warm cache was never observed.
///
/// It survives as the FLOOR rather than the budget: at or below the crossover
/// volume the computed budget is byte-identical to this constant, so the change
/// is a no-op there. That is a statement about THIS number, not a guarantee about
/// the eviction behaviour it feeds.
pub const DEFAULT_CARGO_TARGET_BUDGET_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Is this gap between two evictions short enough that the cache did not survive
/// to be reused? Pure so the cadence rule is testable without a clock.
fn gap_is_thrash(gap_ms: u64) -> bool {
    gap_ms < THRASH_GAP_MS
}

/// Should a run of `run` consecutive fast gaps produce a warning? Loud on reaching
/// the threshold, then only every further multiple — a sustained pathology must not
/// become per-eviction noise (@Astra, #3910 review).
fn thrash_warning_due(run: u32) -> bool {
    run >= CONSECUTIVE_FAST_GAPS_BEFORE_WARNING
        && run % CONSECUTIVE_FAST_GAPS_BEFORE_WARNING == 0
}

/// THRASH IS A CADENCE, NOT A COUNT.
///
/// The first version of this fired after 8 evictions in a process lifetime, and
/// @Astra rejected it: eight evictions over an arbitrary uptime prove nothing
/// about whether builds are starting cold. @IntelMac then produced the
/// counter-example against their own review — 16 evictions on their box, which
/// would have been named loudly, spread across days at a cadence that is ordinary
/// housekeeping. A count cannot tell 36-in-a-day from 36-in-a-month, and only one
/// of those is the defect.
///
/// So the signal is the GAP between consecutive evictions, and the threshold is
/// chosen — not derived — from two observations: the measured defect ran at a
/// 13.5-minute median, and a full `cargo test -p continuum-core` on the machines
/// that hit it takes longer than 30 minutes.
///
/// An eviction inside that window is therefore SUSPICIOUS, not conclusive. This
/// function sees only timestamps: it does not know which rung ran (the ladder may
/// have taken incremental state alone), whether a build was in flight, or whether
/// anything was reused. A short gap is consistent with a cache being emptied
/// faster than a build can use it; it does not establish it.
const THRASH_GAP_MS: u64 = 30 * 60 * 1000;

/// How many consecutive fast gaps before saying so. Two, because one short gap is
/// a coincidence (a big test run finishing beside a build) and a RUN of them is a
/// pattern. Small on purpose: the failure this names cost a day.
const CONSECUTIVE_FAST_GAPS_BEFORE_WARNING: u32 = 2;

/// The cargo-target budget for a machine, DERIVED from the volume that holds it —
/// 10% of the volume, floored at [`DEFAULT_CARGO_TARGET_BUDGET_BYTES`].
///
/// This follows [`serving_tier_reserve_bytes`] below rather than inventing a
/// policy: a derived budget is generous exactly where the drive is, and a constant
/// cannot be. The build cache is the artifact class whose working set scales with
/// the WORKSPACE rather than the user, so a fixed number is guaranteed wrong at one
/// end. It was wrong at the big end, which is where the work happens.
///
/// ## What 10% is, honestly (@Astra, review of #3910)
///
/// **It is a capacity policy, not a measured working set.** Nobody has measured
/// what this workspace's debug tree actually needs; what IS measured is that 50 GiB
/// is below it on a workstation (#3906: 36 evictions, 356 GB, 13.5-minute median).
/// So this fixes a budget known to be too small by tying it to the resource it
/// competes for, and it does not claim to have found the right number. A measured
/// working set would be better and is not blocked by this.
///
/// ## Drives are sold in decimal and measured in binary (@IntelMac)
///
/// Their box is a "500 GB" container = 465 GiB, so 10% = 46.5 GiB and the 50 GiB
/// FLOOR wins — their budget is unchanged by this PR. An earlier version of this
/// table said "500 GiB — the crossover", which is arithmetically true and
/// practically misleading, because nobody owns a 500 GiB drive; they own a 500 GB
/// one. The crossover in the units drives are ADVERTISED in is ~537 GB.
///
/// | advertised | binary | budget | vs. the old constant |
/// |---|---|---|---|
/// | 256 GB laptop | 238 GiB | 50 GiB (floor) | unchanged |
/// | 500 GB | 465 GiB | 50 GiB (floor) | unchanged |
/// | 1 TB | 931 GiB | 93 GiB | ~1.9x |
/// | 2 TB workstation | 1863 GiB | 186 GiB | 3.7x the old budget |
/// | 4 TB | 3725 GiB | 372 GiB | 7.4x |
///
/// The right-hand column is the BUDGET RATIO and nothing more. Whether a larger
/// budget ends the observed eviction cadence on a given machine is untested —
/// plausible, since the pool evicts when over budget, but not demonstrated, and
/// the earlier version of this table asserted "no longer thrashes" as if it were.
///
/// At or below ~537 GB the computed budget equals the floor, so those machines
/// see no change from this function. It also means the fix does NOTHING for small
/// machines: if a laptop is thrashing, this is not the change that helps it.
pub fn cargo_target_budget_bytes(volume_total_bytes: u64) -> u64 {
    (volume_total_bytes / 10).max(DEFAULT_CARGO_TARGET_BUDGET_BYTES)
}

/// The pressure the broker sees for a derived-artifact cache: its own budget
/// pressure OR the pressure of the volume it sits on, whichever is higher. A
/// per-pool budget cut from the volume's TOTAL cannot see a volume filled by other
/// classes; the volume can, and a cache is the first thing a hot volume should shed.
pub fn cache_pool_pressure(own: f64, volume: f64) -> f64 {
    own.max(volume)
}

/// Non-blocking exclusive flock on `path`. `Some(file)` holds the lock
/// until dropped; `None` = someone else (a live cargo build) holds it.
/// A missing lock file is created — holding it makes a cargo invocation
/// that starts mid-eviction block until we finish, instead of racing us.
/// The outcome of trying to take the lock — THREE states, not two.
///
/// The previous signature was `Option<File>`, and `None` meant BOTH "a live cargo
/// build holds this" and "I could not even open the file". @Astra caught that on
/// review of #3910: the decline reason built on it claimed a live build in a case
/// where there might be no build at all. That is the absence-versus-refusal defect
/// this PR exists to fix, committed inside the mechanism added to fix it — the
/// second time in two PRs I have written it, which is why the type is now the
/// thing that prevents it rather than a comment asking me not to.
#[derive(Debug)]
enum LockAttempt {
    /// Held by us until the file drops.
    Acquired(std::fs::File),
    /// Someone else holds it. On this path that someone is a live cargo build, and
    /// standing down is the guard WORKING.
    HeldByAnother,
    /// The lock file could not be opened or queried at all — a permissions problem,
    /// a vanished directory, a filesystem that cannot lock. NOT evidence of a build,
    /// and NOT evidence the guard protected anything.
    Unavailable(String),
}

/// The first process whose executable lives under `dir`, if any: the test binary cargo
/// is running, a build script, a tool it built. Read from the process table the other
/// pools already consult; a table that cannot be read answers "nobody", which keeps the
/// eviction honest in the direction it already errs (derived artifacts, next build
/// recreates them). Canonicalized on both sides so a symlinked target dir still matches.
pub(crate) fn process_executing_under(dir: &Path) -> Option<(u32, String)> {
    let dir = dir.canonicalize().ok()?;
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().iter().find_map(|(pid, p)| {
        let exe = p.exe()?;
        let exe = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf()); // unwrap_or_else: an exe that vanished mid-read compares by its recorded path
        exe.starts_with(&dir).then(|| (pid.as_u32(), exe.display().to_string()))
    })
}

fn try_exclusive_flock_detailed(path: &Path) -> LockAttempt {
    use fs2::FileExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => return LockAttempt::Unavailable(format!("open failed: {e}")),
    };
    match file.try_lock_exclusive() {
        Ok(()) => LockAttempt::Acquired(file),
        // CONTENTION IS NOT `WouldBlock` ON WINDOWS. @Astra caught this on the
        // second review of #3910: `fs2` signals a held lock with the platform's own
        // code — `EWOULDBLOCK` on Unix, but `ERROR_LOCK_VIOLATION` (33) on Windows,
        // which `std` does not map to `ErrorKind::WouldBlock`. Matching on the kind
        // therefore classified a GENUINELY HELD cargo lock as `Unavailable` on
        // Windows — the same misclassification this tri-state was added to fix,
        // inverted, and live on the only platform where I could have measured it.
        //
        // `fs2::lock_contended_error()` is the library's own answer to "what does
        // contention look like here", so we ask it rather than guessing per
        // platform. [[dir-opened-as-file-windows-only]] — Windows chooses
        // differently and says nothing.
        Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            LockAttempt::HeldByAnother
        }
        Err(e) => LockAttempt::Unavailable(format!("lock query failed: {e}")),
    }
}

#[allow(dead_code)] // retained for callers outside this pool; the detailed form is what the guard uses.
fn try_exclusive_flock(path: &Path) -> Option<std::fs::File> {
    // Cross-platform advisory file lock via `fs2` (Unix: flock; Windows:
    // LockFileEx) — ONE code path on every platform. The lock is held until the
    // returned `File` is dropped, matching the previous flock-until-drop
    // semantics. `try_lock_exclusive` returns Err when another process (a live
    // cargo build) holds it → `None`.
    use fs2::FileExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .ok()?;
    file.try_lock_exclusive().ok().map(|_| file)
}

/// WHY an eviction did nothing. #3906, at @Astra's request: "declined eviction
/// deserves a typed reason."
///
/// The pool logged what it FREED and never logged what it DECLINED, so a guard
/// that held and a guard that was never reached produced the identical record:
/// silence. That is the same absence-versus-refusal defect this card turned out
/// to be three instances of, sitting inside the mechanism meant to diagnose it.
///
/// It matters here specifically because the open question on #3906 is whether
/// the flock guard holds on Windows. That question is unanswerable from a log
/// that cannot distinguish "a live build held the lock, so I stood down" from
/// "I never ran". These variants are that distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclinedEviction {
    /// The tracked directory does not exist — nothing to evict, and NOT a sign
    /// the guard worked.
    NoSuchDirectory,
    /// A live cargo build holds the target-dir lock. THIS IS THE GUARD WORKING,
    /// and until now it was indistinguishable from the guard never running.
    BuildHoldsRootLock,
    /// A live cargo build holds `debug/.cargo-lock`. Same as above, one level in.
    BuildHoldsDebugLock,
    /// Release incremental state belongs to the release profile's build lock.
    BuildHoldsReleaseLock,
    /// The deletion owner cannot establish an unredirected path inside its root.
    UnsafePath(String),
    CitizenBusy(String),
    /// A process is EXECUTING an image under a profile (a test binary, a tool cargo built
    /// and ran). Cargo releases its locks at "Finished" and then runs what it built, so
    /// the locks say nothing about that window, and deleting a running image is a
    /// SIGKILL when its next page faults in (the M5, 2026-10-05: three test runs killed
    /// at launch while the pool sat 14 GB over budget). The profile rung is declined while
    /// it is in use; the earlier rungs still cut.
    ProfileInUse { pid: u32, exe: String },
    ActiveArtifact { path: PathBuf },
    ReleaseInputsRetained { bytes: u64 },
    AwaitingReleaseMeasurement,
    /// The lock could not be opened or queried — permissions, a vanished path, a
    /// filesystem that cannot lock. @Astra, #3910 review: the first version folded
    /// this into the two above, so an IO failure was reported as "a live build holds
    /// it". That is a FALSE claim of protection, and worse than no reason at all: it
    /// would have answered #3906's open question ("does the guard hold on Windows?")
    /// with a confident yes the data does not support.
    LockUnavailable(String),
}

impl DeclinedEviction {
    /// A stable tag for "is this the same reason as last time". Not the payload —
    /// two IO failures with different error text are the same standing condition.
    fn discriminant(&self) -> u8 {
        match self {
            Self::NoSuchDirectory => 0,
            Self::BuildHoldsRootLock => 1,
            Self::BuildHoldsDebugLock => 2,
            Self::LockUnavailable(_) => 3,
            Self::ProfileInUse { .. } => 4,
            Self::BuildHoldsReleaseLock => 5,
            Self::UnsafePath(_) => 6,
            Self::ActiveArtifact { .. } => 7,
            Self::ReleaseInputsRetained { .. } => 8,
            Self::AwaitingReleaseMeasurement => 9,
            Self::CitizenBusy(_) => 10,
        }
    }

    /// One line an operator can act on, per reason.
    fn why(&self) -> &'static str {
        match self {
            Self::NoSuchDirectory => {
                "the tracked cargo-target directory does not exist — this pool is \
                 managing nothing, which is worth knowing before trusting its numbers"
            }
            Self::BuildHoldsRootLock => {
                "a live cargo build holds the target-dir lock — standing down, the \
                 build's cache is safe"
            }
            Self::BuildHoldsDebugLock => {
                "a live cargo build holds debug/.cargo-lock — standing down, the \
                 build's cache is safe"
            }
            Self::BuildHoldsReleaseLock => "a live cargo build holds release/.cargo-lock — standing down",
            Self::UnsafePath(_) => "a cache path is redirected or unverifiable — refusing reclamation",
            Self::CitizenBusy(_) => "citizen-owned build residue waits for hands and node quiescence",
            Self::ProfileInUse { .. } => {
                "a process is executing an image under this Cargo profile — its output closure stands down"
            }
            Self::ActiveArtifact { .. } => "serving retains an artifact under this output closure — refusing reclamation",
            Self::ReleaseInputsRetained { .. } => "release installation inputs or undeclared outputs remain; reclamation needs their producer custody, not Cargo's build lock alone",
            Self::AwaitingReleaseMeasurement => "release custody was already checked for this measurement; await the existing scanner before repeating its content reads",
            Self::LockUnavailable(_) => {
                "the lock could not be opened or queried — NOT evidence a build was                  protected, and NOT evidence the guard ran; the cache was left alone                  because we could not establish it was safe to touch"
            }
        }
    }
}

/// Budget-capped eviction owner for the shared cargo-target cache.
/// Shares its [`TrackedDir`] with the disk reporter — one measurement,
/// two consumers. Pressure = cached usage / budget, so this pool goes
/// over-budget (and the broker acts) long before the whole disk is
/// critical — the cache is bounded by policy, not by the disk filling.
pub struct CargoTargetPool {
    /// PER-INSTANCE pool name. It was a hardcoded `"disk-cargo-target"` for every
    /// instance, and `PressureBroker::register` dedups by name — so when #3907
    /// started registering a SECOND cache (`cargo-target-wt`) in a loop, the second
    /// registration silently REPLACED the first and the shared cache became
    /// ungoverned. Two boot log lines, one governed pool. Found by S6 via @Astra.
    /// Derived from the tracked class so a third cache cannot repeat it.
    tier: String,
    tracked: Arc<TrackedDir>,
    budget_bytes: u64,
    active_artifacts: Arc<ActiveArtifactSet>,
    /// Read the owning volume's already-published sample; never probe here.
    volume_sample: fn(&Path) -> Option<crate::capacity::system_profile::DriveInfo>,
    citizen_quiescence: fn(&Path) -> Result<(), String>,
    /// The last decline reason (as a discriminant) and how many times it has
    /// repeated unchanged — so a persistent condition is reported once and then
    /// rarely, instead of once per broker tick (@Astra, #3910 review).
    last_decline: std::sync::atomic::AtomicU8,
    decline_repeats: std::sync::atomic::AtomicU32,
    /// Wall-clock ms of the previous eviction, and how many consecutive gaps since
    /// have been shorter than [`THRASH_GAP_MS`]. CADENCE, not a lifetime count —
    /// see [`CargoTargetPool::note_eviction`] for why the count was wrong.
    last_eviction_ms: std::sync::atomic::AtomicU64,
    fast_gaps: std::sync::atomic::AtomicU32,
    /// Never a cached permission: only defers another release proof attempt
    /// until the existing measurement owner observes the root again.
    release_observation: std::sync::atomic::AtomicU64,
}

impl CargoTargetPool {
    pub(crate) fn empty_root_custody(root: &Path) -> Result<Option<Vec<std::fs::File>>, String> {
        Self::owned_metadata(root, root)?;
        let mut locks = Vec::new();
        for relative in [".cargo-lock", "debug/.cargo-lock", "release/.cargo-lock"] {
            let path = root.join(relative);
            let parent = path.parent().ok_or("cache lock has no parent")?;
            if !parent.exists() {
                continue;
            }
            Self::owned_metadata(parent, root)?;
            if std::fs::symlink_metadata(&path).is_ok() {
                Self::owned_metadata(&path, root)?;
            }
            match try_exclusive_flock_detailed(&path) {
                LockAttempt::Acquired(lock) => locks.push(lock),
                LockAttempt::HeldByAnother => return Ok(None),
                LockAttempt::Unavailable(error) => return Err(error),
            }
        }
        fn has_content(path: &Path, root: &Path) -> Result<bool, String> {
            let metadata = CargoTargetPool::owned_metadata(path, root)?;
            if metadata.is_dir() {
                for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
                    if has_content(&entry.map_err(|error| error.to_string())?.path(), root)? {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            let custody = [".cargo-lock", "debug/.cargo-lock", "release/.cargo-lock"]
                .iter()
                .any(|relative| root.join(relative) == path);
            Ok(!custody || !metadata.is_file() || metadata.len() != 0)
        }
        if has_content(root, root)? {
            Ok(None)
        } else {
            Ok(Some(locks))
        }
    }

    fn budget_for_volume(&self, volume: Option<&crate::capacity::system_profile::DriveInfo>) -> u64 {
        volume.map(|volume| cargo_target_budget_bytes(volume.total_bytes)).unwrap_or(self.budget_bytes)
    }

    fn pressure_for_volume(&self, volume: Option<&crate::capacity::system_profile::DriveInfo>) -> f64 {
        let own = self.tracked.bytes() as f64 / self.budget_for_volume(volume).max(1) as f64;
        let volume = volume.filter(|volume| volume.total_bytes > 0)
            .map(|volume| volume.total_bytes.saturating_sub(volume.available_bytes) as f64 / volume.total_bytes as f64)
            .unwrap_or(0.0);
        cache_pool_pressure(own, volume)
    }

    pub fn new(tracked: Arc<TrackedDir>, budget_bytes: u64) -> Self {
        use sha2::{Digest, Sha256};
        // Display is lossy for native non-Unicode roots. It is a label, never
        // identity: two distinct writer paths must retain separate broker pools.
        let root_id = Sha256::digest(tracked.path().as_os_str().as_encoded_bytes());
        Self {
            // Class plus physical root: distinct configured roots must not replace
            // each other in the broker's dedup-by-name registration.
            tier: format!("disk-{}:{root_id:x}", tracked.class()),
            last_decline: std::sync::atomic::AtomicU8::new(u8::MAX),
            decline_repeats: std::sync::atomic::AtomicU32::new(0),
            last_eviction_ms: std::sync::atomic::AtomicU64::new(0),
            fast_gaps: std::sync::atomic::AtomicU32::new(0),
            release_observation: std::sync::atomic::AtomicU64::new(u64::MAX),
            tracked,
            budget_bytes: budget_bytes.max(1),
            volume_sample: super::DiskPressureMonitor::volume_for,
            citizen_quiescence: super::citizen_workspace_pool::node_quiescence,
            active_artifacts: serving_active_artifacts(),
        }
    }

    /// The eviction ladder, cheapest-regret first. Each rung is fully
    /// reproducible by the next build; order matters — incremental
    /// state is the biggest win with the smallest rebuild cost.
    /// Record a decline WITH ITS REASON, and answer 0 as before. Behaviour is
    /// unchanged; the difference is that the ledger can now tell a guard that
    /// HELD from a guard that never ran.
    fn declined(&self, reason: DeclinedEviction) -> u64 {
        use std::sync::atomic::Ordering::Relaxed;
        // Bounded, and by the REASON rather than by time: a persistent held lock or
        // a missing root is asked again on every broker tick, so warning per attempt
        // is a per-tick log line for a condition that has not changed. @Astra caught
        // that the success-path cadence bound did nothing for this path.
        //
        // A CHANGE of reason always speaks (that is news), an unchanged reason
        // speaks on the 1st, 8th, 64th … repeat, and the pool's identity is in the
        // line because there is more than one of these now (#3911).
        let key = reason.discriminant();
        let repeats = if self.last_decline.swap(key, Relaxed) == key {
            self.decline_repeats.fetch_add(1, Relaxed) + 1
        } else {
            self.decline_repeats.store(0, Relaxed);
            0
        };
        let speak = repeats == 0 || repeats.is_power_of_two() && repeats % 8 == 0;
        if speak {
            crate::clog_warn!(
                "💾 {} eviction DECLINED ({:?}{}): {}",
                self.tier,
                reason,
                if repeats > 0 {
                    format!(", unchanged for {repeats} attempts")
                } else {
                    String::new()
                },
                reason.why()
            );
        }
        0
    }

    /// Record this eviction's CADENCE, and speak when the cache is being emptied
    /// faster than a build can use it.
    ///
    /// The first version counted evictions and warned at 8. Both reviewers rejected
    /// it independently and they were right: a lifetime count cannot distinguish
    /// 36-in-a-day (the measured defect) from 36-spread-over-a-week (ordinary
    /// housekeeping), and @IntelMac's own box would have been named loudly for the
    /// latter.
    ///
    /// WHAT THIS DOES AND DOES NOT KNOW (@Astra, second review). It knows the
    /// INTERVAL between evictions. It does NOT know that any build started cold:
    /// the ladder may have taken only incremental state, a build can complete
    /// between two evictions, and no hit/miss observation reaches this function.
    /// So the warning reports the observed frequency and names its consequence as
    /// SUSPECTED. An earlier draft asserted "every build here is starting cold",
    /// which is precisely the over-claim this PR exists to stop — written into the
    /// warning added to stop it, for the third time in this file's history.
    ///
    /// Note the counter is CONSECUTIVE FAST GAPS, not evictions: a run of 3 means
    /// four evictions, and the message says gaps for that reason.
    ///
    /// The whole failure took a day to find for want of a line like this: the pool
    /// logged what it FREED and never what the freeing MEANT, so 36 evictions and
    /// 356 GB of destroyed compilation looked exactly like 36 successful runs.
    fn note_eviction(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let now = // A clock we cannot read must not manufacture a "fast gap" — 0 makes the
            // first comparison look ancient, which is the quiet side.
            crate::time::EpochMs::now().millis();
        let previous = self.last_eviction_ms.swap(now, Relaxed);
        // First eviction of the run has no gap to measure, and a zero clock is not a
        // measurement either.
        if previous == 0 || now == 0 || now <= previous {
            return;
        }
        let gap_ms = now - previous;
        if !gap_is_thrash(gap_ms) {
            self.fast_gaps.store(0, Relaxed);
            return;
        }
        let run = self.fast_gaps.fetch_add(1, Relaxed) + 1;
        if !thrash_warning_due(run) {
            return;
        }
        // OBSERVED FREQUENCY, then the SUSPECTED consequence — never asserted as
        // fact. @Astra, #3910 review: this cannot establish that builds start cold.
        // The ladder may have removed only incremental state, a build can finish
        // between two evictions, and no hit/miss observation enters this function.
        // What is measured is the interval. What follows from it is a suspicion,
        // and saying more than that is the defect this whole PR is about.
        crate::clog_warn!(
            "💾 {} saw {} consecutive eviction GAPS under {} min (last gap {} min)              against a {} GB budget. That is the frequency, not a verdict: this pool              cannot see whether any build reused the cache. SUSPECTED — a budget              below this workspace's working set, with builds paying rebuild cost they              need not. To confirm, compare a build's wall time against a run with a              larger budget. The budget derives from the volume              (cargo_target_budget_bytes); a volume this pool could not resolve falls              back to the floor, which is the first thing to check.",
            self.tier,
            run,
            THRASH_GAP_MS / 60_000,
            gap_ms / 60_000,
            self.capacity_bytes() / (1024 * 1024 * 1024)
        );
    }

    /// The rungs, freed in order. Profile rungs preserve their custody and
    /// installation inputs — see [`Self::free_rung`].
    fn ladder(root: &Path) -> [PathBuf; 5] {
        [
            root.join("debug/incremental"),
            root.join("release/incremental"),
            root.join("tests"),
            root.join("debug"),
            root.join("release"),
        ]
    }

    /// Inspect every existing component under the owned root without following
    /// redirects. Cargo's locks exclude cooperating writers during this check
    /// and removal; they are not a security boundary against a hostile OS user.
    fn owned_metadata(path: &Path, root: &Path) -> Result<std::fs::Metadata, String> {
        let relative = path.strip_prefix(root).map_err(|_| "cache path escaped its root")?;
        let mut current = root.to_path_buf();
        let check = |path: &Path| -> Result<std::fs::Metadata, String> {
            let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
            if Self::is_redirect(&metadata) { return Err(format!("redirected cache path: {}", path.display())); }
            Ok(metadata)
        };
        let mut metadata = check(&current)?;
        // A durable root was physical when admitted. Refuse if an ancestor has
        // since been redirected instead of silently reclaiming its destination.
        for ancestor in root.ancestors().skip(1) { check(ancestor)?; }
        for component in relative.components() {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err("cache path contains a non-child component".into());
            }
            current.push(component);
            metadata = check(&current)?;
        }
        Ok(metadata)
    }

    /// A symlink, or on Windows a reparse point (junction): an entry that names another
    /// place. One definition for the root check and the tree walk.
    fn is_redirect(metadata: &std::fs::Metadata) -> bool {
        let redirected = metadata.file_type().is_symlink();
        #[cfg(windows)]
        let redirected = {
            use std::os::windows::fs::MetadataExt;
            redirected || metadata.file_attributes() & 0x400 != 0
        };
        redirected
    }

    /// The rung itself must be physical (its whole chain is checked); BELOW it, a link
    /// is a leaf the removal unlinks and never follows, so it is no reason to refuse the
    /// rung. Cargo writes such links into its own build tree: cxxbridge's
    /// `out/cxxbridge/include/<crate>/src/*.rs` link back to the source, and refusing on
    /// them refused every eviction of the shared cache for 29 hours (the M5,
    /// 2026-10-17 04:58Z to 10-18 09:28Z, 211 declines, 96% disk), which refused every
    /// KV page save and cooled every resident's turn.
    fn validate_tree(path: &Path, root: &Path) -> Result<(), String> {
        if Self::owned_metadata(path, root)?.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
                Self::validate_below(&entry.map_err(|error| error.to_string())?.path())?;
            }
        }
        Ok(())
    }

    fn validate_below(path: &Path) -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if !Self::is_redirect(&metadata) && metadata.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
                Self::validate_below(&entry.map_err(|error| error.to_string())?.path())?;
            }
        }
        Ok(())
    }

    /// Unlink a link found inside a physical owned directory: the link goes, its target
    /// is never touched. A Windows junction is a directory entry, so it takes `remove_dir`.
    fn unlink_redirect(path: &Path, root: &Path) {
        let Some(parent) = path.parent() else { return };
        if path == root || Self::owned_metadata(parent, root).is_err() { return; }
        if std::fs::symlink_metadata(path).is_ok_and(|metadata| Self::is_redirect(&metadata))
            && std::fs::remove_file(path).is_err()
        {
            let _ = std::fs::remove_dir(path);
        }
    }

    /// Recheck each child during removal and never traverse a redirected entry.
    /// Count only successfully removed file bytes, including a partial IO failure.
    fn free_owned_tree(path: &Path, root: &Path, preserve: Option<&Path>) -> u64 {
        if preserve == Some(path) { return 0; }
        let Ok(metadata) = Self::owned_metadata(path, root) else {
            Self::unlink_redirect(path, root);
            return 0;
        };
        if !metadata.is_dir() {
            return if metadata.is_file() && std::fs::remove_file(path).is_ok() { metadata.len() } else { 0 };
        }
        let Ok(entries) = std::fs::read_dir(path) else { return 0; };
        let mut freed = 0u64;
        for entry in entries.flatten() {
            freed = freed.saturating_add(Self::free_owned_tree(&entry.path(), root, preserve));
        }
        // Keeping a lock also keeps its directory. Other directories are removed
        // only once empty; no recursive API can follow a late redirected child.
        if !preserve.is_some_and(|lock| lock.parent() == Some(path)) {
            let _ = std::fs::remove_dir(path);
        }
        freed
    }

    /// Free one rung, PRESERVING the lock this eviction is holding.
    ///
    /// @Astra, #3910 review: `remove_dir_all(debug)` unlinks `debug/.cargo-lock` —
    /// the exact file whose lock is our claim to be here. On Unix the unlink
    /// SUCCEEDS while we hold the descriptor: our fd keeps the old inode alive, the
    /// PATH is gone, and another cargo is then free to create and lock a brand-new
    /// `debug/.cargo-lock` while we are still deleting the tree underneath it. The
    /// lock stops being an ownership boundary at the moment we delete the thing
    /// being locked.
    ///
    /// So the `debug` rung removes debug's CHILDREN and leaves the lock file and its
    /// parent standing. Everything under it is still reclaimed; the boundary
    /// survives the reclaim. `debug/.cargo-lock` is a zero-length lock file — the
    /// bytes we decline to free by keeping it are not bytes.
    fn free_rung(rung: &Path, root: &Path) -> u64 {
        if rung == root.join("release") {
            use continuum_cli_lifecycle::cargo_cache_custody as custody;
            // Cargo's build/<unit>/out layout locates declarations; unit names
            // grant no ownership. Each producer must explicitly certify its
            // static-only children. Generic OUT_DIR data and deps may be runtime
            // inputs of programs outside this profile and remain untouched.
            let Ok(units) = std::fs::read_dir(rung.join("build")) else { return 0; };
            let mut freed = 0u64;
            for unit in units.flatten() {
                let out = unit.path().join("out");
                let receipt = out.join(custody::RECEIPT);
                if Self::validate_tree(&out, root).is_err() || !custody::declared_static_inputs(&out) {
                    continue;
                }
                // Cargo's own captured producer output is its freshness input.
                // Invalidate it FIRST: removing only an OUT_DIR file does not
                // rerun build.rs, while watching a self-written receipt loops.
                let output = unit.path().join("output");
                let output_bytes = Self::free_owned_tree(&output, root, None);
                if output.exists() || output_bytes == 0 { continue; }
                freed = freed.saturating_add(output_bytes);
                let receipt_bytes = Self::free_owned_tree(&receipt, root, None);
                if receipt.exists() || receipt_bytes == 0 { continue; }
                freed = freed.saturating_add(receipt_bytes);
                for child in custody::CHILDREN {
                    freed = freed.saturating_add(Self::free_owned_tree(&out.join(child), root, None));
                }
            }
            return freed;
        }
        let debug = root.join("debug");
        let lock = debug.join(".cargo-lock");
        Self::free_owned_tree(rung, root, (rung == debug).then_some(lock.as_path()))
    }
}

impl ResourcePool for CargoTargetPool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_for_volume((self.volume_sample)(self.tracked.path()).as_ref())
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// A CACHE ON A CRITICAL VOLUME IS A CRITICAL POOL. The broker relieves a pool
    /// only when ITS pressure crosses `act_above`; this pool's own pressure is
    /// usage over a budget derived from the volume's TOTAL, so it cannot see a
    /// volume filled by other classes. Measured 2026-09-18 on the 5090 the moment
    /// the disk daemon could see at all: volume `critical` (95 %+, 92 GB of
    /// 1.9 TB free), cargo-target 0.34 of budget, cargo-target-wt 0.65 — both
    /// below the act line, nothing evicted, the disk-root signal pool alerting
    /// "hot and stuck" every tick. A derived artifact cache is exactly what a
    /// critical volume should shed first, so the volume's pressure is this pool's
    /// floor: over budget OR on a hot volume, either one brings the broker.
    fn pressure(&self) -> f64 {
        self.pressure_for_volume((self.volume_sample)(self.tracked.path()).as_ref())
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        let citizen_custody = self.tracked.eviction_custody();
        let _hands = if let Some(peer) = *citizen_custody {
            let Some(hands) = HandsLease::try_take(peer) else {
                return self.declined(DeclinedEviction::CitizenBusy("hands are in use".into()));
            };
            let home = match crate::paths::continuum_home() {
                Ok(home) => home,
                Err(error) => return self.declined(DeclinedEviction::CitizenBusy(error.to_string())),
            };
            if let Err(error) = (self.citizen_quiescence)(&home) {
                return self.declined(DeclinedEviction::CitizenBusy(error));
            }
            Some((hands, home))
        } else { None };
        if !root.exists() {
            return self.declined(DeclinedEviction::NoSuchDirectory);
        }
        // Validate lock locations before opening them: a profile or lock symlink
        // must not redirect custody to an unrelated file outside this cache.
        for path in [&root, &root.join(".cargo-lock"), &root.join("debug"),
            &root.join("debug/.cargo-lock"), &root.join("release"), &root.join("release/.cargo-lock")] {
            match std::fs::symlink_metadata(path) {
                Ok(_) => if let Err(error) = Self::owned_metadata(path, &root) {
                    return self.declined(DeclinedEviction::UnsafePath(error));
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return self.declined(DeclinedEviction::UnsafePath(error.to_string())),
            }
        }
        // Safety invariant 1: hold cargo's lock files exclusively for the
        // whole eviction, or do nothing. Guards held in scope until return.
        let _root_lock = match try_exclusive_flock_detailed(&root.join(".cargo-lock")) {
            LockAttempt::Acquired(lock) => lock,
            LockAttempt::HeldByAnother => {
                return self.declined(DeclinedEviction::BuildHoldsRootLock)
            }
            LockAttempt::Unavailable(why) => {
                return self.declined(DeclinedEviction::LockUnavailable(why))
            }
        };
        let mut profile_locks = Vec::new();
        for (profile, held_reason) in [
            ("debug", DeclinedEviction::BuildHoldsDebugLock),
            ("release", DeclinedEviction::BuildHoldsReleaseLock),
        ] {
            let lock_path = root.join(profile).join(".cargo-lock");
            if lock_path.parent().is_some_and(Path::exists) {
                match try_exclusive_flock_detailed(&lock_path) {
                    LockAttempt::Acquired(lock) => profile_locks.push(lock),
                    LockAttempt::HeldByAnother => return self.declined(held_reason),
                    LockAttempt::Unavailable(why) => return self.declined(DeclinedEviction::LockUnavailable(why)),
                }
            }
        }

        let mut freed = 0u64;
        let debug = root.join("debug");
        let release = root.join("release");
        for rung in Self::ladder(&root) {
            if let Some((_, home)) = &_hands {
                if let Err(error) = (self.citizen_quiescence)(home) {
                    self.declined(DeclinedEviction::CitizenBusy(error));
                    break;
                }
            }
            if freed >= want_bytes.max(1) {
                break;
            }
            if !rung.exists() {
                continue;
            }
            let observation = self.tracked.observation();
            if rung == release && self.release_observation.load(std::sync::atomic::Ordering::Relaxed) == observation {
                self.declined(DeclinedEviction::AwaitingReleaseMeasurement);
                continue;
            }
            if let Err(error) = Self::validate_tree(&rung, &root) {
                self.declined(DeclinedEviction::UnsafePath(error));
                continue;
            }
            // Cargo releases its locks before running the produced image. Protect
            // the whole profile, including native build outputs/dependencies, while
            // any image from it executes; the same rule applies to both profiles.
            if rung == debug || rung == release {
                if let Some((pid, exe)) = process_executing_under(&rung) {
                    self.declined(DeclinedEviction::ProfileInUse { pid, exe });
                    continue;
                }
            }
            if self.active_artifacts.protects(&rung) {
                self.declined(DeclinedEviction::ActiveArtifact { path: rung });
                continue;
            }
            if rung == release {
                self.release_observation.store(observation, std::sync::atomic::Ordering::Relaxed);
            }
            freed = freed.saturating_add(Self::free_rung(&rung, &root));
            if rung == release && freed < want_bytes.max(1) {
                let bytes = dir_size_bytes(&release);
                if bytes > 0 { self.declined(DeclinedEviction::ReleaseInputsRetained { bytes }); }
            }
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 cargo-target eviction freed {} GB (budget {} GB) — derived artifacts only; next build recreates them",
                freed / (1024 * 1024 * 1024),
                self.capacity_bytes() / (1024 * 1024 * 1024)
            );
            self.note_eviction();
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

/// Governed reserve on the serving-tier volume: the slice of the drive the
/// serving tier must never claim, so the OS + working set never starve
/// behind model artifacts. DERIVED from the volume (#287-style), never a
/// user-tuned knob: 10% of the volume, floored at 32 GiB (a small SSD
/// still keeps a real working margin; a 4 TB NVMe reserves 400 GB — the
/// tier is generous exactly where the drive is).
pub fn serving_tier_reserve_bytes(volume_total_bytes: u64) -> u64 {
    (volume_total_bytes / 10).max(32 * 1024 * 1024 * 1024)
}

/// The serving tier's byte budget on its volume: total − governed reserve.
/// Saturating: a volume smaller than the reserve yields 0 capacity (the
/// tier is not offered there — resolution degrades, never gates).
pub fn serving_tier_capacity_bytes(volume_total_bytes: u64) -> u64 {
    volume_total_bytes.saturating_sub(serving_tier_reserve_bytes(volume_total_bytes))
}

/// The set of artifact paths serving is ACTIVELY paging per-token (the
/// resident model's GGUF, its expert container dir, device-fit overrides).
/// The eviction pool consults this before every migration — the #302
/// safety invariant is that an actively-paged artifact is NEVER moved out
/// from under the engine. Serving's reconcile (her half's
/// `ensure_hot_resident`) registers paths on spawn and releases on lane
/// teardown; registration is path-prefix aware so marking a model DIR
/// protects everything inside it.
#[derive(Default)]
pub struct ActiveArtifactSet {
    paths: RwLock<std::collections::HashMap<PathBuf, Option<PathBuf>>>,
}

impl ActiveArtifactSet {
    /// Raw identities without physical proof cannot authorize any reclamation.
    pub fn register(&self, path: PathBuf) {
        if let Ok(mut set) = self.paths.write() {
            set.insert(path, None);
        }
    }

    /// Resolve before the synchronous serving admission, through the shared bounded
    /// path worker. Failed resolution is published as unresolved custody, never guessed.
    pub async fn resolve(path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() { return None; }
        match super::disk_reporters::resolve_existing_storage_path(path).await {
            Ok(physical) => Some(physical),
            Err(error) => {
                crate::clog_warn!("active artifact identity unresolved: {}", error);
                None
            }
        }
    }

    pub fn register_resolved(&self, path: PathBuf, physical: Option<PathBuf>) {
        if let Ok(mut set) = self.paths.write() {
            let physical = physical.filter(|resolved| path.is_absolute() && resolved.is_absolute());
            set.insert(path, physical);
        }
    }

    pub fn replace(&self, old: Option<&Path>, new: Option<(PathBuf, Option<PathBuf>)>) {
        if let Ok(mut set) = self.paths.write() {
            if let Some(old) = old { set.remove(old); }
            if let Some((raw, physical)) = new {
                let physical = physical.filter(|resolved| raw.is_absolute() && resolved.is_absolute());
                set.insert(raw, physical);
            }
        }
    }

    pub fn release(&self, path: &Path) {
        if let Ok(mut set) = self.paths.write() {
            set.remove(path);
        }
    }

    /// `candidate` must be resolved by its eviction owner before this lock-only check.
    /// Is `candidate` (an eviction target) protected? True when any
    /// registered active path IS the candidate, lives UNDER it, or the
    /// candidate lives under a registered dir — prefix containment both
    /// directions, so neither "marked the dir, evicting a file inside"
    /// nor "marked the file, evicting its parent dir" can slip through.
    pub fn protects(&self, candidate: &Path) -> bool {
        let Ok(set) = self.paths.read() else {
            // A poisoned lock means a panic mid-update — fail SAFE:
            // treat everything as protected rather than migrate blind.
            return true;
        };
        set.values().any(|physical| match physical {
            None => true,
            Some(active) => !active.is_absolute() || !candidate.is_absolute()
                || crate::paths::has_path_prefix(active, candidate)
                || crate::paths::has_path_prefix(candidate, active),
        })
    }
}

/// Process-wide active-artifact registry, one per process (same singleton
/// shape as `install_tracked_dirs`). Serving marks residency here; the
/// broker-registered pool reads it. Lazily created so tests and tools get
/// a working (empty) set without boot wiring.
static SERVING_ACTIVE_ARTIFACTS: std::sync::OnceLock<Arc<ActiveArtifactSet>> =
    std::sync::OnceLock::new();

pub fn serving_active_artifacts() -> Arc<ActiveArtifactSet> {
    SERVING_ACTIVE_ARTIFACTS
        .get_or_init(|| Arc::new(ActiveArtifactSet::default()))
        .clone()
}

/// NVMe serving-tier eviction owner (#302) — the decided story for the
/// `genome-models` cache class. The class holds the HOT serving set:
/// served GGUFs, expert containers, device-fit overrides — the artifacts
/// the engine pages per-token, which is why this pool MIGRATES to the
/// COLD/frozen drive instead of deleting (a model is hours of download /
/// forge work, not a derived artifact like cargo-target), and why it
/// refuses to touch anything in [`ActiveArtifactSet`].
///
/// ## Safety invariants (each pinned by a test)
///
/// 1. **Never the actively-paged artifact.** Anything `protects()` says
///    serving holds is skipped, even if it is the coldest entry.
/// 2. **Migrate, never blind-delete.** An entry leaves the hot tier only
///    after a byte-verified copy exists on the cold drive (copy → fsync →
///    verify → delete). A digest-verified twin already on cold = pure
///    drop of the hot copy. Verify failure removes the partial COPY,
///    never the source.
/// 3. **No cold drive ⇒ free nothing.** With nowhere safe to migrate,
///    the pool reports 0 and logs loudly — pressure stays visible to the
///    broker/operator instead of being "relieved" by destroying models.
///    (Composes with device-fit one tier down: Unfittable routes to the
///    grid, LOUD — never a silent HDD stream. See
///    docs/architecture/STORAGE-SERVING-TIER-GOVERNOR.md.)
pub struct NvmeServingTierPool {
    tracked: Arc<TrackedDir>,
    capacity_bytes: u64,
    cold_root: Option<PathBuf>,
    active: Arc<ActiveArtifactSet>,
}

impl NvmeServingTierPool {
    /// Whole-container migration uses the artifact owner's durable resolver
    /// transaction, never this pool's child-entry migration loop. The caller
    /// must retain real reader/producer/spawn custody; no legacy runtime is
    /// enrolled automatically merely because a container directory is tracked.
    pub fn freeze_expert_container(
        root: &Path,
        custody: &dyn crate::model_registry::artifacts::expert_placement::ContainerMigrationCustody,
    ) -> Result<u64, String> {
        crate::model_registry::artifacts::expert_placement::freeze(root, custody)
    }

    /// `volume_total_bytes` is the TOTAL size of the volume holding the
    /// hot tier (capacity derives from it — never a hand-tuned budget).
    /// `cold_root` is the migration target directory on the COLD drive
    /// (`None` ⇒ this box has no cold tier; eviction refuses, invariant 3).
    pub fn new(
        tracked: Arc<TrackedDir>,
        volume_total_bytes: u64,
        cold_root: Option<PathBuf>,
        active: Arc<ActiveArtifactSet>,
    ) -> Self {
        Self {
            tracked,
            capacity_bytes: serving_tier_capacity_bytes(volume_total_bytes).max(1),
            cold_root,
            active,
        }
    }

    /// Eviction candidates: top-level entries of the hot root (a served
    /// GGUF file or a per-model directory), coldest-first by mtime.
    /// Artifact granularity is the top-level entry — a model's dir moves
    /// as a unit, never half its files.
    fn candidates_coldest_first(root: &Path) -> Vec<(PathBuf, u64)> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut list: Vec<(PathBuf, u64, std::time::SystemTime)> = entries
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if meta.is_symlink() {
                    return None;
                }
                let path = e.path();
                let size = if meta.is_dir() {
                    dir_size_bytes(&path)
                } else {
                    meta.len()
                };
                let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                Some((path, size, mtime))
            })
            .collect();
        list.sort_by_key(|(_, _, mtime)| *mtime);
        list.into_iter().map(|(p, s, _)| (p, s)).collect()
    }
}

/// Byte-level equality of two files, streamed — the "digest-verified"
/// primitive without a hash dependency (a full byte compare is strictly
/// as strong as comparing digests of both sides).
fn files_identical(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(fa), Ok(fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let mut ra = std::io::BufReader::new(fa);
    let mut rb = std::io::BufReader::new(fb);
    let mut ba = [0u8; 64 * 1024];
    let mut bb = [0u8; 64 * 1024];
    loop {
        let na = match ra.read(&mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let nb = match rb.read(&mut bb) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// Recursive equality: files byte-compare; dirs compare entry sets then
/// recurse. Any unreadable piece = NOT identical (never "verified" on a
/// guess).
fn entries_identical(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    match (ma.is_dir(), mb.is_dir()) {
        (false, false) => files_identical(a, b),
        (true, true) => {
            let Ok(entries) = std::fs::read_dir(a) else {
                return false;
            };
            let names_a: Vec<std::ffi::OsString> =
                entries.flatten().map(|e| e.file_name()).collect();
            let Ok(entries_b) = std::fs::read_dir(b) else {
                return false;
            };
            let names_b: HashSet<std::ffi::OsString> =
                entries_b.flatten().map(|e| e.file_name()).collect();
            names_a.len() == names_b.len()
                && names_a
                    .iter()
                    .all(|n| names_b.contains(n) && entries_identical(&a.join(n), &b.join(n)))
        }
        _ => false,
    }
}

/// Copy `src` (file or tree) to `dst`, fsyncing every file — the durable
/// half of migrate-then-delete. Cross-device safe (plain read/write copy,
/// no rename tricks).
fn copy_entry_durable(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            copy_entry_durable(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        // Reopen for WRITE to fsync. `File::open` yields a read-only
        // handle, and on Windows `FlushFileBuffers` against one fails
        // with ERROR_ACCESS_DENIED — so every real migration errored
        // here, the caller deleted the partial cold copy, kept the hot
        // copy, and logged "failed verification". Net effect on Windows:
        // the serving tier could only ever evict an artifact that ALREADY
        // had a verified twin on cold (the pure-drop path, which never
        // calls this function). Under genuine disk pressure the pool
        // freed nothing while reporting that it tried. POSIX permits
        // fsync on a read-only fd, which is why this survived on
        // macOS/Linux — [[dir-opened-as-file-windows-only]] is the same
        // family: a file-API assumption that only one platform enforces.
        std::fs::OpenOptions::new()
            .write(true)
            .open(dst)?
            .sync_all()?;
    }
    Ok(())
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    if std::fs::metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

impl ResourcePool for NvmeServingTierPool {
    fn tier_name(&self) -> &str {
        "disk-serving-tier"
    }

    fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        // Invariant 3: no cold tier ⇒ refuse loudly, never blind-delete.
        let Some(cold_root) = self.cold_root.as_deref() else {
            crate::clog_warn!(
                "💾 serving-tier over budget but this box has NO cold drive — refusing to \
                 delete model artifacts; resolve by adding a cold tier or dropping models \
                 explicitly (models are re-fetch-hours, not derived artifacts)"
            );
            return 0;
        };

        let mut freed = 0u64;
        for (path, size) in Self::candidates_coldest_first(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            // Invariant 1: never the actively-paged artifact.
            let Ok(physical) = std::fs::canonicalize(&path) else { continue };
            if self.active.protects(&physical) {
                continue;
            }
            let Some(name) = path.file_name() else {
                continue;
            };
            let dest = cold_root.join(name);

            if dest.exists() {
                if entries_identical(&path, &dest) {
                    // Verified twin already frozen on cold: pure drop.
                    if remove_entry(&path).is_ok() {
                        freed = freed.saturating_add(size);
                    }
                } else {
                    // Name collision with DIFFERENT content — never
                    // clobber a cold artifact; skip and say so.
                    crate::clog_warn!(
                        "💾 serving-tier migrate skipped {:?}: cold copy exists with \
                         different content — refusing to overwrite",
                        name
                    );
                }
                continue;
            }

            // Invariant 2: copy → fsync → verify → delete source.
            if std::fs::create_dir_all(cold_root).is_err() {
                crate::clog_warn!(
                    "💾 serving-tier migrate failed: cannot create cold root {:?}",
                    cold_root
                );
                break;
            }
            match copy_entry_durable(&path, &dest) {
                Ok(()) if entries_identical(&path, &dest) => {
                    if remove_entry(&path).is_ok() {
                        freed = freed.saturating_add(size);
                    }
                }
                _ => {
                    // Verify failed or copy errored: remove the PARTIAL
                    // COPY, never the source.
                    let _ = remove_entry(&dest);
                    crate::clog_warn!(
                        "💾 serving-tier migrate of {:?} failed verification — hot copy \
                         kept, partial cold copy removed",
                        name
                    );
                }
            }
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 serving-tier migrated {} GB of frozen artifacts to cold storage \
                 (hot capacity {} GB) — nothing deleted without a verified cold copy",
                freed / (1024 * 1024 * 1024),
                self.capacity_bytes / (1024 * 1024 * 1024)
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

// The tests gate themselves (`#![cfg(test)]` in tests.rs), so the scanner reads the file as test code.
mod tests;
// The cache-class eviction decisions the tests check (`#![cfg(test)]` in the file).
mod eviction_decisions;
