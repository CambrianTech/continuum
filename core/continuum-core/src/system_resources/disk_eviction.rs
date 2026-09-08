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
//!    `<root>/debug/.cargo-lock`) before touching anything; any lock we
//!    can't get means a build is in flight → free 0 bytes and let the
//!    broker retry next tick.
//! 2. **Never leave the root.** The eviction ladder is a fixed list of
//!    `root.join(...)` subpaths — no pattern matching, no following
//!    symlinks out of the tree.
//! 3. **Derived artifacts only.** The ladder deletes incremental state,
//!    test binaries, then the debug tree — all reproducible by the next
//!    `cargo build`. `release/` is never touched here (it is small and
//!    holds the binaries ops copies into `~/.continuum/bin`).

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
/// The code did exactly what it was told; it was told the wrong number. Every
/// build on that machine started near-cold, which is where the day's
/// "Windows rustc is flaky" and "~70 minutes to rebuild" reports came from.
///
/// It survives as the FLOOR rather than the budget: a small laptop still gets
/// exactly the behaviour it had before, and nothing regresses anywhere.
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
/// So the signal is the GAP between consecutive evictions. This threshold is the
/// gap below which a build cache is being emptied faster than a build can use it:
/// the measured defect ran at a 13.5-minute median, and a full
/// `cargo test -p continuum-core` on a warm cache is comfortably longer than 30
/// minutes on the machines that hit this. An eviction arriving inside this window
/// means the previous build's cache did not survive to be reused.
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
/// | 2 TB workstation | 1863 GiB | 186 GiB | no longer thrashes |
/// | 4 TB | 3725 GiB | 372 GiB | headroom to match |
///
/// The floor is what keeps this safe: every machine at or below ~537 GB behaves
/// exactly as it does today, so this can regress nobody. It also means the fix
/// does NOTHING for small machines — if a laptop is thrashing, this is not the
/// change that helps it, and that is worth knowing rather than assuming.
pub fn cargo_target_budget_bytes(volume_total_bytes: u64) -> u64 {
    (volume_total_bytes / 10).max(DEFAULT_CARGO_TARGET_BUDGET_BYTES)
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
}

impl CargoTargetPool {
    pub fn new(tracked: Arc<TrackedDir>, budget_bytes: u64) -> Self {
        Self {
            // "cargo-target" keeps its historical pool name so nothing that reads
            // the ledger by that string breaks; every other class gets its own.
            tier: format!("disk-{}", tracked.class()),
            last_decline: std::sync::atomic::AtomicU8::new(u8::MAX),
            decline_repeats: std::sync::atomic::AtomicU32::new(0),
            last_eviction_ms: std::sync::atomic::AtomicU64::new(0),
            fast_gaps: std::sync::atomic::AtomicU32::new(0),
            tracked,
            budget_bytes: budget_bytes.max(1),
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            // A clock we cannot read must not manufacture a "fast gap" — 0 makes the
            // first comparison look ancient, which is the quiet side.
            .unwrap_or(0);
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
            self.budget_bytes / (1024 * 1024 * 1024)
        );
    }

    /// The rungs, freed in order. The last one is `debug` ITSELF, which is why it
    /// is not a plain `remove_dir_all` — see [`Self::free_rung`].
    fn ladder(root: &Path) -> [PathBuf; 3] {
        [
            root.join("debug/incremental"),
            root.join("tests"),
            root.join("debug"),
        ]
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
        let debug = root.join("debug");
        if rung != debug {
            let size = dir_size_bytes(rung);
            return if std::fs::remove_dir_all(rung).is_ok() {
                size
            } else {
                0
            };
        }
        let lock = debug.join(".cargo-lock");
        let mut freed = 0u64;
        let Ok(entries) = std::fs::read_dir(&debug) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == lock {
                continue;
            }
            // `dir_size_bytes` READS A DIRECTORY — it returns 0 for a file, so
            // sizing children with it silently under-counts every loose artifact
            // in `debug/` (`lib.rlib` and friends). The existing ladder test caught
            // this the moment the rung stopped being one `remove_dir_all`.
            let is_dir = path.is_dir();
            let size = if is_dir {
                dir_size_bytes(&path)
            } else {
                std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            };
            let removed = if is_dir {
                std::fs::remove_dir_all(&path).is_ok()
            } else {
                std::fs::remove_file(&path).is_ok()
            };
            if removed {
                freed = freed.saturating_add(size);
            }
        }
        freed
    }
}

impl ResourcePool for CargoTargetPool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return self.declined(DeclinedEviction::NoSuchDirectory);
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
        let debug_lock_path = root.join("debug/.cargo-lock");
        let _debug_lock = if debug_lock_path.parent().is_some_and(Path::exists) {
            match try_exclusive_flock_detailed(&debug_lock_path) {
                LockAttempt::Acquired(lock) => Some(lock),
                LockAttempt::HeldByAnother => {
                    return self.declined(DeclinedEviction::BuildHoldsDebugLock)
                }
                LockAttempt::Unavailable(why) => {
                    return self.declined(DeclinedEviction::LockUnavailable(why))
                }
            }
        } else {
            None
        };

        let mut freed = 0u64;
        for rung in Self::ladder(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            if !rung.exists() {
                continue;
            }
            freed = freed.saturating_add(Self::free_rung(&rung, &root));
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 cargo-target eviction freed {} GB (budget {} GB) — derived artifacts only; next build recreates them",
                freed / (1024 * 1024 * 1024),
                self.budget_bytes / (1024 * 1024 * 1024)
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
    paths: RwLock<HashSet<PathBuf>>,
}

impl ActiveArtifactSet {
    pub fn register(&self, path: PathBuf) {
        if let Ok(mut set) = self.paths.write() {
            set.insert(path);
        }
    }

    pub fn release(&self, path: &Path) {
        if let Ok(mut set) = self.paths.write() {
            set.remove(path);
        }
    }

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
        set.iter()
            .any(|active| active.starts_with(candidate) || candidate.starts_with(active))
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
            if self.active.protects(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn seeded_target(tmp: &Path) -> Arc<TrackedDir> {
        std::fs::create_dir_all(tmp.join("debug/incremental")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("tests")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("release")).expect("mkdir");
        std::fs::write(tmp.join("debug/incremental/a.o"), vec![0u8; 4000]).expect("write");
        std::fs::write(tmp.join("debug/lib.rlib"), vec![0u8; 3000]).expect("write");
        std::fs::write(tmp.join("tests/t.bin"), vec![0u8; 2000]).expect("write");
        std::fs::write(tmp.join("release/keep.bin"), vec![0u8; 1000]).expect("write");
        let tracked = TrackedDir::new("cargo-target", tmp.to_path_buf());
        // Seed the cached size the way the scanner would.
        tracked.set_bytes(dir_size_bytes(tmp));
        tracked
    }

    // what this catches: the eviction ladder frees cheapest-regret first
    // (incremental before the whole debug tree), stops once `want` is met,
    // NEVER touches release/, and decrements the shared TrackedDir so the
    // broker doesn't re-fire against space already freed.
    #[test]
    fn ladder_frees_in_order_and_never_touches_release() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let usage_before = tracked.bytes();
        let pool = CargoTargetPool::new(tracked.clone(), 1);

        // Small want: the incremental rung (4000 B) alone satisfies it.
        let freed = pool.evict_at_least(1000);
        assert_eq!(freed, 4000, "incremental rung is the first and only cut");
        assert!(!tmp.path().join("debug/incremental").exists());
        assert!(tmp.path().join("tests").exists(), "later rung untouched");
        assert!(tmp.path().join("release/keep.bin").exists());
        assert_eq!(
            tracked.bytes(),
            usage_before - 4000,
            "shared measurement reflects the delete immediately"
        );

        // Large want: remaining rungs go (tests 2000 + debug 3000);
        // release survives regardless.
        let freed = pool.evict_at_least(u64::MAX);
        assert_eq!(freed, 5000);
        assert!(!tmp.path().join("tests").exists());
        assert!(tmp.path().join("release/keep.bin").exists());

        // `debug/` SURVIVES NOW, AND THAT IS THE POINT (@Astra, #3910 review).
        // This assertion used to be `!debug.exists()`. Deleting the directory also
        // unlinked `debug/.cargo-lock` — the file whose lock is this eviction's
        // claim to be running at all. On Unix that unlink SUCCEEDS while we hold
        // the descriptor: our fd keeps the old inode alive, the path is free, and
        // another cargo can create and lock a replacement while we are still
        // deleting underneath it. The lock stops being an ownership boundary at the
        // moment we delete the thing being locked.
        //
        // So the contract is: everything under `debug/` is reclaimed, and the lock
        // file and its parent stand. The bytes we decline to free are a zero-length
        // lock file.
        let debug = tmp.path().join("debug");
        assert!(debug.exists(), "the lock's parent must survive the rung");
        assert!(
            debug.join(".cargo-lock").exists(),
            "the lock we hold must still be at its PATH, not merely alive on an              unlinked inode — an unlinked lock guards nothing against the next cargo"
        );
        let survivors: Vec<String> = std::fs::read_dir(&debug)
            .expect("read debug")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            survivors,
            vec![".cargo-lock".to_string()],
            "ONLY the lock survives — anything else means the rung under-reclaimed"
        );
    }

    // what this catches: safety invariant 1 — a held cargo lock (a build
    // in flight) makes eviction a no-op. Deleting deps out from under a
    // running cargo corrupts the build AND can race half-written files.
    #[test]
    fn held_cargo_lock_blocks_eviction_entirely() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let pool = CargoTargetPool::new(tracked, 1);

        let _build_holds_lock = try_exclusive_flock(&tmp.path().join(".cargo-lock"))
            .expect("test takes the lock first");
        assert_eq!(pool.evict_at_least(u64::MAX), 0, "locked ⇒ untouched");
        assert!(tmp.path().join("debug/incremental").exists());
    }

    // what this catches: pressure is usage/BUDGET (policy-bounded), not
    // usage/disk — the pool must go over-budget and trigger the broker
    // while the disk itself is still healthy. An unscanned dir reads as
    // 0 pressure (not under management yet), never NaN.
    #[test]
    fn pressure_is_budget_relative() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        let pool = CargoTargetPool::new(tracked.clone(), 50_000);
        assert_eq!(pool.pressure(), 0.0, "unscanned = not under management");

        tracked.set_bytes(75_000); // scanner measured 1.5× the budget
        assert!(pool.pressure() > 1.4 && pool.pressure() < 1.6);
        assert_eq!(pool.capacity_bytes(), 50_000);
    }

    // what this catches: TrackedDir.record_freed saturates at zero — an
    // over-reported eviction must never wrap the cached size to u64::MAX
    // (which would read as 17 exabytes of pressure and evict forever).
    #[test]
    fn record_freed_saturates() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        tracked.set_bytes(5_000);
        tracked.record_freed(10_000);
        assert_eq!(tracked.bytes(), 0);
    }

    // what this catches: THE CHAIN, not the components. The 2026-07-13
    // incident happened with every unit green — the monitor ran with no
    // reporters and the broker ran with no eviction owner, so 460 GB
    // accumulated while every individual piece "worked." This test runs
    // the real PressureBroker against a real over-budget CargoTargetPool
    // on a real temp tree and asserts bytes actually leave the disk when
    // relieve() fires. If any link (pressure math, tier thresholds,
    // broker act_above, pool registration shape, eviction ladder)
    // regresses, this fails — the health component Joel called critical
    // is guarded end-to-end, not piecewise.
    #[test]
    fn broker_relieve_actually_deletes_from_an_over_budget_pool() {
        use crate::paging::{BrokerConfig, PressureBroker};

        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        // Budget below measured usage → pool pressure > 1.0 → Critical.
        let budget = tracked.bytes() / 2;
        let pool = Arc::new(CargoTargetPool::new(tracked.clone(), budget));

        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool as Arc<dyn ResourcePool>);

        let disk_before = dir_size_bytes(tmp.path());
        let report = broker.relieve();

        assert!(report.triggered, "over-budget pool must trigger relief");
        assert!(report.bytes_freed > 0, "relief must free real bytes");
        assert!(
            dir_size_bytes(tmp.path()) < disk_before,
            "bytes must actually leave the disk, not just the accounting"
        );
        assert!(
            tmp.path().join("release/keep.bin").exists(),
            "release survives even broker-driven eviction"
        );
    }

    // what this catches: an ownerless cache class — the exact shape of
    // the incident. Every class in standard_tracked_dirs must have a
    // DECIDED eviction story: either an owner pool exists, or the class
    // is explicitly listed here as deferred with the task that owns it.
    // Adding a sixth cache class without deciding makes this fail at
    // compile-adjacent cost instead of at a user's full disk.
    #[test]
    fn every_cache_class_has_a_decided_eviction_story() {
        // genome-models graduated from deferred to OWNED (#302): the
        // NvmeServingTierPool is exactly the "reference-aware, never
        // blind-delete a served model" owner the deferred entry demanded —
        // active-set aware, migrate-to-cold, verified-copy-before-delete.
        // logs + probes graduated straight to OWNED on registration
        // (2026-08-06) rather than through the deferred list: their
        // contents are rotation generations the writer has already
        // moved past, so `RotationLogPool` can drop the oldest with no
        // in-flight-set hazard to reason about. Deferring is for
        // classes where blind deletion is UNSAFE (a grading instance,
        // a served model); there is nothing unsafe here to defer for.
        let owned = [
            "cargo-target",
            // Same owner and the same rule as the shared target: derived build output,
            // re-creatable by definition, evicted oldest-artifact-first under pressure.
            "cargo-target-wt",
            "genome-models",
            "logs",
            "probes",
            // Owner: perception::eye_reaper. A profile whose browser is gone is dead
            // weight the instant the browser dies, so eviction is not age-based — the
            // reaper removes every profile no live process names, at boot and on the
            // pressure edge.
            "eye-profiles",
        ];
        let deferred = [
            (
                "hf-hub",
                "#155: hub LRU keyed on last-access — downloads are re-fetchable. Measured \
                 2026-09-06 (M5): 51 GB of `models--*` blobs, every one a staging copy of a \
                 GGUF already promoted into the `models` store; the owner evicts a hub entry \
                 the moment its artifact is present in the store, and the rest by last access",
            ),
            // Registered 2026-09-06, the day the M5 hit zero free with 360 GB here and the
            // governor unable to see it. Two sub-classes: SERVED weights (the catalog's
            // active/pinned/roster tiers — never blind-deleted, same rule as genome-models'
            // NvmeServingTierPool) and UNROSTERED weights (a store entry matching no catalog
            // id: two concluded experiments held 186 GB that day). The owner evicts the
            // second class oldest-access first and must ask the registry resolver, never a
            // name heuristic, whether an entry is on a serving path.
            (
                "models",
                "58c27b0c/#155: unrostered-weights pool — evict store entries the registry \
                 resolves to NO catalog id and no active/pinned lane, oldest access first; \
                 served tiers are the NvmeServingTierPool's, never this pool's",
            ),
            (
                "eval-captures",
                "#155: age-based sweep — every file is a re-creatable diagnostic (kv-diag \
                 snapshots, wire-request jsonl from SERVING_WIRE_CAPTURE_DIR); writers are \
                 opt-in and quiet by default, so the class grows only while an operator is \
                 actively hunting. Owner when built: a capped appender like the log pool",
            ),
            // Measured 2026-09-06 (M5, the day the disk hit zero): 33 workspace copies of the
            // repo (12 resident citizens, 21 dormant), 7–20 GB EACH, none of it hers: a 7.6 GB
            // .git holding the canonical clone's history plus COPIED stale worktree metadata
            // (5 × 462 MB of vendored llama.cpp packs per copy), 4.5 GB of gitignored model
            // caches (tools/models), 2 GB of mobile build output, node_modules. The by-hand
            // eviction that day (alternates to the canonical objects + repack -l, ignored
            // dirs dropped, dormant copies patch-then-dropped) is the pool's spec.
            (
                "citizens",
                "58c27b0c: (1) a workspace is a `--shared` clone of the canonical repo, never \
                 a copy — no history, no ignored caches, no foreign worktree metadata; (2) a \
                 DORMANT citizen's workspace (uuid on no roster, no turn for 7 days) is \
                 patch-then-dropped: dirty checkouts archived as <inst>.patch + base sha, \
                 tree removed; stores are persona MEMORY, never auto-evicted",
            ),
            // Sibling of `citizens` and inherits its rule: a LIVE mind's longterm.db and
            // working-set.json are MEMORY, never auto-evicted. What IS evictable is the
            // GHOST sub-class — a dir whose uuid appears in no roster and which never
            // recorded a turn, left by the spawn name-pool (#437). Measured 2026-08-20:
            // 295 dirs, 286 of them under 100 KB, 2 real citizens. Small in bytes, which is
            // exactly why it went unnoticed for so long — the hazard here is not capacity,
            // it is that ghost identities pollute the roster and the demand ceiling. An
            // owner pool must key on "has this uuid ever completed a turn", never on size.
            (
                "personas",
                "#155/#437: per-persona MEMORY, never blind-LRU'd. Evictable sub-class is \
                 GHOST dirs only — no roster entry AND no recorded turn — which needs a pool \
                 that can ask the roster, not a size heuristic",
            ),
            ("forge", "#155: export trimmer — intermediates only, published artifacts stay"),
            // Registered the day benchmark/swe-* landed, BEFORE a sweep ran. Everything under
            // it is re-creatable — repo clones from git, venvs from uv, the dataset from HF —
            // so eviction is safe by construction; what it must never do is delete an instance
            // dir mid-grade, which is why it wants a pool that knows the in-flight set rather
            // than a blind LRU. Until then it is tracked and REPORTED, so the class can never
            // be the silent one again.
            (
                "benchmarks",
                "#155: LRU over per-instance dirs, skipping the in-flight set — clones/venvs \
                 are re-creatable from git+uv, so only an active grade is at risk. NOT \
                 everything under it is re-creatable: `swe/captures/run-*/attempt-N.patch` \
                 is a citizen's actual diff, deleted from her workspace the moment the next \
                 attempt resets it (#379). An eviction pool here must treat captures as \
                 EVIDENCE — small, and the only thing that can answer what she wrote — and \
                 reclaim the bulky re-creatable clones/venvs instead. Corrected 2026-08-18: \
                 this entry read \"everything under it is re-creatable\", which the 25 \
                 patches already sitting there had falsified since before it was written",
            ),
            // Steady-state owner ALREADY EXISTS in-file: RAII drop on every in-process
            // return path + the provision-time orphan sweep for worlds a killed process
            // leaves behind (an eval run cannot survive its process, so any non-live
            // sibling is debris; everything inside is a CoW clone of the checkout —
            // re-creatable by construction). Deferred only for the broker seam: under
            // real disk pressure the broker cannot yet claw these bytes back BETWEEN
            // provisions — that wants a pool that consults `live_eval_roots()`, never
            // a blind LRU that could delete a mid-exam world.
            (
                "eval-roots",
                "#155: broker-reachable pool over cognition/eval::live_eval_roots(); \
                 sweep + RAII already own the steady state and the crash path",
            ),
            // Steady-state owner ALREADY EXISTS at lane spawn:
            // `inference::llama_server::sweep_stale_page_generations` deletes every
            // sibling geometry the moment a serve's geometry changes — a page whose
            // slot size no longer exists can never be restored, so the sweep is
            // reference-safe by construction, and within a live geometry each
            // activity owns ONE file that saves overwrite in place (count bounded
            // by residents × rooms). Deferred only for the broker seam: under real
            // disk pressure the broker cannot yet claw back the CURRENT
            // generation's pages — that wants a pool that asks the slot pool which
            // activities are resident (their pages are one eviction from being
            // needed) versus departed, never a blind LRU.
            // airc's per-room projection snapshots (operator scope; the personas' live under
            // `citizens`). Size is O(rooms × projection): the board snapshot is the folded
            // board (cards, not events), the wall snapshot is the room's posts (few). Both
            // are accelerators rebuilt from the transcript on any anomaly, so eviction is
            // always safe; what they want is a sweep keyed on the room directory — a
            // snapshot for a room that is archived or no longer subscribed is dead weight.
            (
                "airc-board-cache",
                "1291173d/#155: sweep snapshots of rooms absent from the directory or \
                 archived; rebuilt from the transcript on the next read, so deletion is \
                 always safe",
            ),
            (
                "airc-wall-cache",
                "airc#1390/#155: same sweep as airc-board-cache — one file per room, the \
                 room's posts; rebuilt on the next read",
            ),
            (
                "kv-pages",
                "#155: broker-reachable pool keyed on slot-pool residency; the \
                 spawn-time generation sweep already owns the stale-geometry path",
            ),
        ];
        use super::super::disk_pressure::DiskReporter as _;
        for dir in super::super::disk_reporters::standard_tracked_dirs(Path::new("/h")) {
            let name = dir.report().name;
            let decided =
                owned.contains(&name.as_str()) || deferred.iter().any(|(n, _)| *n == name);
            assert!(
                decided,
                "cache class '{name}' has NO eviction decision — register an owner pool or \
                 add it to the deferred list above with the task that owns it (task #155; \
                 the 2026-07-13 incident was exactly an ownerless class filling the disk)"
            );
        }
    }

    mod serving_tier {
        use super::*;

        /// Set an mtime on a path that may be a FILE or a DIRECTORY.
        ///
        /// Windows will not hand you a handle to a directory through a
        /// plain `File::open` — it returns `PermissionDenied` (code 5)
        /// — so you must ask for `FILE_WRITE_ATTRIBUTES` access with
        /// `FILE_FLAG_BACKUP_SEMANTICS`, the documented way to open a
        /// directory handle. Both work for regular files too, so one
        /// helper covers both shapes on both platforms.
        ///
        /// what this fixes: these three `serving_tier` tests were RED on
        /// Windows and green everywhere else, because the helper opened
        /// `stale-model/` (a directory) as a file. Exactly the defect
        /// that also refused the probe sink's boot the same day — a
        /// directory opened as a file is invisible on macOS/Linux and
        /// fatal on Windows, so it survives review on the platform the
        /// author is using.
        fn set_mtime(path: &Path, t: std::time::SystemTime) {
            #[cfg(windows)]
            let f = {
                use std::os::windows::fs::OpenOptionsExt;
                const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
                const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
                std::fs::OpenOptions::new()
                    .access_mode(FILE_WRITE_ATTRIBUTES)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                    .open(path)
                    .expect("open for mtime")
            };
            #[cfg(not(windows))]
            let f = std::fs::File::open(path).expect("open for mtime");
            f.set_modified(t).expect("mtime");
        }

        /// A hot-tier tree with two frozen model artifacts (one dir-shaped,
        /// one file-shaped) and one actively-served dir. mtimes are staged so
        /// `stale-model/` is coldest, then `old.gguf`, then the active dir.
        fn seeded_hot_tier(hot: &Path) -> Arc<TrackedDir> {
            std::fs::create_dir_all(hot.join("stale-model")).expect("mkdir");
            std::fs::write(hot.join("stale-model/model.gguf"), vec![1u8; 4000]).expect("write");
            std::fs::write(hot.join("old.gguf"), vec![2u8; 3000]).expect("write");
            std::fs::create_dir_all(hot.join("served-model")).expect("mkdir");
            std::fs::write(hot.join("served-model/model.gguf"), vec![3u8; 2000]).expect("write");
            // Stage mtimes: coldest first. filetime not in deps — touch via
            // set_modified (std, stable since 1.75).
            let t0 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
            let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
            set_mtime(&hot.join("stale-model"), t0);
            set_mtime(&hot.join("old.gguf"), t1);
            let tracked = TrackedDir::new("genome-models", hot.to_path_buf());
            tracked.set_bytes(dir_size_bytes(hot));
            tracked
        }

        // what this catches: #302 safety invariant 1 — the actively-paged
        // artifact is NEVER migrated, even when eviction wants unlimited
        // bytes. Losing the resident model's GGUF mid-serve is the engine
        // reading from a deleted file.
        #[test]
        fn actively_paged_artifact_is_never_migrated() {
            let hot = tempfile::tempdir().expect("tempdir");
            let cold = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let active = Arc::new(ActiveArtifactSet::default());
            active.register(hot.path().join("served-model"));
            let pool = NvmeServingTierPool::new(
                tracked,
                1_000_000,
                Some(cold.path().to_path_buf()),
                active,
            );

            pool.evict_at_least(u64::MAX);
            assert!(
                hot.path().join("served-model/model.gguf").exists(),
                "the served model must survive unlimited eviction demand"
            );
            assert!(
                !hot.path().join("stale-model").exists(),
                "frozen artifacts migrate"
            );
            assert!(!hot.path().join("old.gguf").exists());
        }

        // what this catches: #302 safety invariant 2 — migration is
        // copy→verify→delete (the artifact EXISTS on cold before the hot
        // copy dies), coldest-first, and a verified twin already on cold is
        // a pure drop (no second copy). The shared TrackedDir decrements so
        // the broker doesn't re-fire on freed space.
        #[test]
        fn migrates_coldest_first_and_pure_drops_verified_twins() {
            let hot = tempfile::tempdir().expect("tempdir");
            let cold = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let usage_before = tracked.bytes();
            // old.gguf already has a byte-identical frozen twin on cold.
            std::fs::write(cold.path().join("old.gguf"), vec![2u8; 3000]).expect("write");
            let pool = NvmeServingTierPool::new(
                tracked.clone(),
                1_000_000,
                Some(cold.path().to_path_buf()),
                Arc::new(ActiveArtifactSet::default()),
            );

            // Want only the coldest entry's worth: stale-model (4000 B).
            let freed = pool.evict_at_least(1000);
            assert_eq!(freed, 4000, "coldest entry goes first");
            assert!(
                cold.path().join("stale-model/model.gguf").exists(),
                "verified cold copy exists"
            );
            assert!(
                !hot.path().join("stale-model").exists(),
                "hot copy gone after verify"
            );
            assert!(
                hot.path().join("old.gguf").exists(),
                "later candidate untouched"
            );
            assert_eq!(tracked.bytes(), usage_before - 4000);

            // Second round: old.gguf's twin is already frozen — pure drop.
            let freed = pool.evict_at_least(1000);
            assert_eq!(freed, 3000);
            assert!(!hot.path().join("old.gguf").exists());
            assert!(cold.path().join("old.gguf").exists());
        }

        // what this catches: #302 safety invariant 3 — a box with no cold
        // drive frees NOTHING (models are hours of re-fetch, not derived
        // artifacts; pressure must stay visible, never "relieved" by
        // destroying them). Also: a cold-side name collision with DIFFERENT
        // content is never clobbered.
        #[test]
        fn no_cold_drive_frees_nothing_and_collisions_never_clobber() {
            let hot = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let pool = NvmeServingTierPool::new(
                tracked.clone(),
                1_000_000,
                None,
                Arc::new(ActiveArtifactSet::default()),
            );
            assert_eq!(pool.evict_at_least(u64::MAX), 0, "no cold tier ⇒ refuse");
            assert!(hot.path().join("stale-model/model.gguf").exists());

            // Collision case: cold has a DIFFERENT old.gguf.
            let cold = tempfile::tempdir().expect("tempdir");
            std::fs::write(cold.path().join("old.gguf"), vec![9u8; 3000]).expect("write");
            let pool = NvmeServingTierPool::new(
                tracked,
                1_000_000,
                Some(cold.path().to_path_buf()),
                Arc::new(ActiveArtifactSet::default()),
            );
            pool.evict_at_least(u64::MAX);
            assert!(
                hot.path().join("old.gguf").exists(),
                "collision with different content: hot copy kept"
            );
            let cold_bytes = std::fs::read(cold.path().join("old.gguf")).expect("read");
            assert_eq!(
                cold_bytes,
                vec![9u8; 3000],
                "cold artifact never overwritten"
            );
        }

        // what this catches: @Astra, twice. First that `Option<File>` collapsed
        // "a build holds it" and "I could not open it" into one `None`; then that
        // my fix still misread contention ON WINDOWS, where fs2 reports
        // ERROR_LOCK_VIOLATION (33) rather than anything std maps to
        // `ErrorKind::WouldBlock`. Matching on the KIND classified a genuinely held
        // cargo lock as `Unavailable` — the exact misclassification the tri-state
        // exists to prevent, inverted, on the platform I run on.
        //
        // So this exercises REAL lock files rather than asserting the mapping:
        // acquire one, then attempt it again while the first guard is alive, on
        // whatever platform the test is running.
        #[test]
        fn a_held_lock_reads_as_contention_and_not_as_an_io_failure() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let lock = tmp.path().join(".cargo-lock");

            let first = try_exclusive_flock_detailed(&lock);
            let held = match first {
                LockAttempt::Acquired(f) => f,
                other => panic!("an uncontended lock must be Acquired, got {other:?}"),
            };

            // Same process, same file, while the first guard is alive.
            match try_exclusive_flock_detailed(&lock) {
                LockAttempt::HeldByAnother => {}
                LockAttempt::Acquired(_) => {
                    panic!("a held lock must NOT be re-acquirable — the guard would be a no-op")
                }
                LockAttempt::Unavailable(why) => panic!(
                    "a held lock was reported as an IO failure ({why}) — this is the                      Windows misclassification: contention is ERROR_LOCK_VIOLATION,                      which std does not map to WouldBlock"
                ),
            }

            drop(held);
            // And once released it is acquirable again, so the contention above was
            // the lock and not some permanent property of the path.
            assert!(
                matches!(
                    try_exclusive_flock_detailed(&lock),
                    LockAttempt::Acquired(_)
                ),
                "non-degeneracy: if this path could never be acquired, the assertion                  above would pass for the wrong reason"
            );
        }

        // what this catches: the third state must be reachable and distinct — a path
        // that cannot be opened is NOT evidence a build was protected.
        #[test]
        fn an_unopenable_lock_path_is_unavailable_not_contention() {
            let tmp = tempfile::tempdir().expect("tempdir");
            // A lock file whose PARENT does not exist cannot be created or opened.
            let impossible = tmp.path().join("no-such-dir").join(".cargo-lock");
            match try_exclusive_flock_detailed(&impossible) {
                LockAttempt::Unavailable(_) => {}
                other => panic!(
                    "an unopenable path must be Unavailable, never mistaken for a                      live build holding the lock, got {other:?}"
                ),
            }
        }

        // what this catches: a LIVE regression that #3907 shipped to canary, found by
        // S6 via @Astra. `CargoTargetPool::tier_name` returned a hardcoded
        // "disk-cargo-target" for every instance, and `PressureBroker::register`
        // dedups by name (broker.rs: `pools.retain(|p| p.tier_name() != name)`).
        // So when #3907 began registering a SECOND cache in a loop, the second
        // registration REPLACED the first and the shared cargo-target became
        // ungoverned — while two boot log lines claimed both were registered.
        //
        // Two governed caches must be two POOLS, and the only thing that makes them
        // two is a distinct name.
        #[test]
        fn two_build_caches_are_two_pools_and_not_one_replacing_the_other() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let shared = TrackedDir::new("cargo-target", tmp.path().join("a"));
            let worktree = TrackedDir::new("cargo-target-wt", tmp.path().join("b"));

            let a = CargoTargetPool::new(shared, DEFAULT_CARGO_TARGET_BUDGET_BYTES);
            let b = CargoTargetPool::new(worktree, DEFAULT_CARGO_TARGET_BUDGET_BYTES);

            assert_eq!(
                a.tier_name(),
                "disk-cargo-target",
                "the original class keeps its historical pool name, so anything                  reading the ledger by that string still finds it"
            );
            assert_eq!(b.tier_name(), "disk-cargo-target-wt");
            assert_ne!(
                a.tier_name(),
                b.tier_name(),
                "non-degeneracy: identical names are exactly what let the broker's                  dedup silently drop one of the two caches"
            );

            // AND THROUGH THE BROKER, because the getters are not where this broke.
            // @Astra: "getter inequality alone misses the actual integration
            // boundary that failed." The failure was `register`'s dedup-by-name
            // (broker.rs: `pools.retain(|p| p.tier_name() != name)`), so the
            // assertion has to survive that call — a future change that makes the
            // names equal again would pass the getter check above and still lose a
            // cache here.
            let broker = crate::paging::broker::PressureBroker::new(Default::default());
            broker.register(Arc::new(a) as Arc<dyn ResourcePool>);
            broker.register(Arc::new(b) as Arc<dyn ResourcePool>);
            let names: Vec<String> = broker
                .snapshot()
                .pools
                .into_iter()
                .map(|p| p.name)
                .filter(|n| n.starts_with("disk-cargo-target"))
                .collect();
            assert_eq!(
                names.len(),
                2,
                "BOTH build caches must survive registration — one row here is the                  regression: the shared cache silently lost its eviction owner while                  two boot log lines claimed otherwise. Got: {names:?}"
            );
        }

        // what this catches: @Astra and @IntelMac both rejected the first version of
        // this, which warned after 8 evictions in a process lifetime. A COUNT cannot
        // tell 36-in-a-day (the measured defect) from 36-over-a-week (housekeeping),
        // and IntelMac's own box would have been named loudly for the latter. The
        // signal is whether the cache survived long enough to be REUSED, which is a
        // gap, not a tally.
        #[test]
        fn thrash_is_a_cadence_not_a_count() {
            // A build cannot reuse a cache emptied minutes ago.
            assert!(gap_is_thrash(60_000));
            assert!(gap_is_thrash(THRASH_GAP_MS - 1));
            // Hours apart is housekeeping, however many times it has happened.
            assert!(!gap_is_thrash(THRASH_GAP_MS));
            assert!(!gap_is_thrash(6 * 60 * 60 * 1000));

            // One fast gap is a coincidence; a run is a pattern.
            assert!(!thrash_warning_due(1));
            assert!(thrash_warning_due(CONSECUTIVE_FAST_GAPS_BEFORE_WARNING));
            // Then periodic, never per-eviction noise.
            let n = CONSECUTIVE_FAST_GAPS_BEFORE_WARNING;
            assert!(!thrash_warning_due(n + 1));
            assert!(thrash_warning_due(n * 2));
        }

        // what this catches: regression for #3906 — the cargo-target budget was a
        // FLAT 50 GiB, which sits BELOW this workspace's debug tree on a
        // workstation. Measured consequence on a 1.9 TB box: 36 evictions and
        // 356 GB of compilation destroyed in one day, median 13.5 minutes apart,
        // at a cadence no build could outlive, and three agents blamed the
        // toolchain. (Frequency is what was measured; whether any individual build
        // started cold was never observed — see note_eviction.)
        //
        // The derivation must (a) actually raise the budget on a big volume —
        // a fix that returned the floor everywhere would pass a weaker test while
        // changing nothing — and (b) never LOWER it on a small one, or this
        // regresses the machines it was already correct for.
        #[test]
        fn the_cargo_target_budget_is_derived_from_the_volume_and_never_below_the_floor() {
            const GIB: u64 = 1024 * 1024 * 1024;
            let floor = DEFAULT_CARGO_TARGET_BUDGET_BYTES;

            // Small machines: unchanged. This is what makes the change safe to
            // ship everywhere rather than only where it was measured.
            assert_eq!(cargo_target_budget_bytes(256 * GIB), floor);
            // @IntelMac's box: a "500 GB" container is 465 GiB, so 10% = 46.5 GiB
            // and the FLOOR wins — their budget is unchanged by this PR. Asserted
            // in the units drives are actually SOLD in, because my first table
            // said "500 GiB, the crossover", which is true and misleading: nobody
            // owns a 500 GiB drive.
            const GB: u64 = 1_000_000_000;
            assert_eq!(cargo_target_budget_bytes(500 * GB), floor);
            // The crossover in advertised units is ~537 GB; below it, floor.
            assert_eq!(cargo_target_budget_bytes(530 * GB), floor);
            // Above it the derivation takes over.
            assert!(cargo_target_budget_bytes(600 * GB) > floor);
            // A volume smaller than the floor still yields the floor, never 0 and
            // never the volume — a budget of nothing is a permanent `cargo clean`.
            assert_eq!(cargo_target_budget_bytes(8 * GIB), floor);
            assert_eq!(cargo_target_budget_bytes(0), floor);

            // The measured machine: 1.9 TB. This is the assertion that would have
            // failed before the fix, and it is the only one that matters to the
            // defect.
            let bigmama = cargo_target_budget_bytes(1900 * GIB);
            assert_eq!(bigmama, 190 * GIB);
            // And in advertised units, the 2 TB class this PR was measured on.
            assert!(cargo_target_budget_bytes(2000 * GB) > 180 * GIB);
            assert!(
                bigmama > floor,
                "non-degeneracy: on the volume that produced #3906 the derived                  budget MUST exceed the old constant, or nothing changed"
            );

            // Monotonic in the volume — a bigger drive never gets a smaller cache.
            let mut prev = 0;
            for tb in [0u64, 256, 512, 1000, 1900, 4000, 8000] {
                let b = cargo_target_budget_bytes(tb * GIB);
                assert!(b >= prev, "budget must not shrink as the volume grows");
                prev = b;
            }
        }

        // what this catches: the capacity derivation (#287-style) — 10% of
        // the volume floored at 32 GiB, saturating to 0 on a volume smaller
        // than the reserve (the tier degrades, never underflows).
        #[test]
        fn capacity_is_volume_minus_derived_reserve() {
            const GIB: u64 = 1024 * 1024 * 1024;
            // 4 TB NVMe: reserve = 10% = 400 GB-ish (> 32 GiB floor).
            assert_eq!(serving_tier_reserve_bytes(4000 * GIB), 400 * GIB);
            assert_eq!(serving_tier_capacity_bytes(4000 * GIB), 3600 * GIB);
            // 100 GiB SSD: 10% = 10 GiB < floor ⇒ reserve is 32 GiB.
            assert_eq!(serving_tier_reserve_bytes(100 * GIB), 32 * GIB);
            assert_eq!(serving_tier_capacity_bytes(100 * GIB), 68 * GIB);
            // Tiny volume: capacity saturates to 0, never wraps.
            assert_eq!(serving_tier_capacity_bytes(GIB), 0);
        }

        // what this catches: ActiveArtifactSet prefix containment BOTH
        // directions — marking a model dir protects files inside it, and
        // marking a file protects its parent dir from wholesale migration.
        #[test]
        fn active_set_protects_prefix_both_directions() {
            let set = ActiveArtifactSet::default();
            set.register(PathBuf::from("/hot/served-model"));
            assert!(set.protects(Path::new("/hot/served-model")));
            assert!(
                set.protects(Path::new("/hot/served-model/model.gguf")),
                "candidate under active dir is protected"
            );
            assert!(
                set.protects(Path::new("/hot")),
                "parent of active is protected"
            );
            assert!(!set.protects(Path::new("/hot/other-model")));
            set.release(Path::new("/hot/served-model"));
            assert!(!set.protects(Path::new("/hot/served-model")));
        }
    }
}
