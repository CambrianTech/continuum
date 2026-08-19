//! lane_registry.rs — accounting for EVERY `llama-server` lane this core spawns,
//! so a crashed core never leaves an orphan holding VRAM.
//!
//! ## The gap this closes (why [`crate::inference::lane_pidfile`] isn't enough)
//!
//! `lane_pidfile` tracks exactly ONE pid — the live persona lane on the canonical
//! port — because its job is canonical-PORT contention: reap or adopt whoever
//! holds *that* port. It deliberately excludes the ephemeral eval/serving-lease
//! lanes ([`crate::inference::llama_server::EphemeralServingLane`]), which run on
//! their own scanned ports and must never touch the canonical pidfile.
//!
//! That leaves a hole: an ephemeral lane is killed on graceful `Drop`, but a
//! SIGKILLed / panicked / power-cut core skips `Drop`, and NOTHING recorded the
//! ephemeral pid — so its `llama-server` survives as an orphan (~6 GB resident)
//! that no successor knows to reap. Observed live: three `llama-server` processes
//! where two prior eval lanes had leaked across crashes.
//!
//! The registry fixes it by recording a per-pid file for EVERY lane the core
//! spawns (live and ephemeral), keyed by pid, removed on graceful teardown. On the
//! next boot [`sweep_orphans`] reaps any recorded EPHEMERAL lane still alive (a
//! fresh core has no in-flight eval, so every recorded ephemeral lane is
//! definitionally an orphan), and garbage-collects dead records of any role. The
//! LIVE record is left to `lane_pidfile`'s canonical-port reclaim, which
//! already adopts-or-reaps it — the two mechanisms compose, they don't fight.
//!
//! ## Safety: never blind-kill (shared with [`crate::inference::lane_pidfile`])
//!
//! A recorded pid can be STALE — the process died and the OS reused its number.
//! So a reap fires ONLY after [`crate::inference::lane_process::is_llama_server`]
//! positively identifies the pid as one of our `llama-server` children. A dead or
//! reused-pid record is removed, never signalled ([[fallbacks-are-illegal-fail-loud]]).
//!
//! ## Test isolation
//!
//! Core ops are PURE and directory-taking (`*_in`), so tests drive them against a
//! unique temp dir and never touch the real `~/.continuum/run/lanes/` of a live
//! core (the #7 `$HOME`-pollution class). Public wrappers resolve the one
//! canonical directory and delegate.

use super::lane_process;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a lane is FOR — the axis `sweep_orphans` reaps on. A fresh boot never has
/// an in-flight eval, so every recorded [`LaneRole::Ephemeral`] is an orphan to
/// reap; the [`LaneRole::Live`] record is deferred to `lane_pidfile`'s
/// canonical-port reclaim (adopt-if-healthy, reap-if-not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRole {
    /// THE host's persona lane on the canonical port (also tracked by
    /// `lane_pidfile` for port adoption). Left alone by [`sweep_orphans`].
    Live,
    /// An eval / serving-lease lane on its own scanned port. Always an orphan
    /// across a core restart → reaped by [`sweep_orphans`].
    Ephemeral,
}

/// One recorded lane. Small, human-readable JSON so a `cat ~/.continuum/run/lanes/*`
/// tells an operator exactly what the core believes it spawned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneRecord {
    pub pid: u32,
    pub port: u16,
    pub role: LaneRole,
    /// The base model id the lane serves — for operator legibility in the sweep
    /// log, not a control input.
    #[serde(default)]
    pub model: String,
}

/// WHY a sweep is running — the one axis on which boot and shutdown differ.
///
/// They differ in exactly one judgement: what a still-alive LIVE-role record
/// means. At boot it may be a perfectly good server this core can adopt, so the
/// decision belongs to `lane_pidfile`'s canonical-port reclaim. At shutdown
/// nothing is adoptable by definition — the core is going away — so a survivor is
/// a leak. Encoding that as a MODE on one sweep (rather than a second sweep
/// function) is what stops the two paths from drifting: every other rule —
/// never-blind-kill, GC dead records, drop unparseable garbage — is shared by
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepMode {
    /// A fresh core starting up. Ephemeral records are definitionally orphans;
    /// the LIVE record is left for `lane_pidfile` to adopt-or-reap.
    Boot,
    /// This core is shutting down. EVERY lane it owns must die with it, live
    /// included — `stop` that leaves a server holding VRAM has not stopped.
    Shutdown,
}

/// What a sweep did to one record — exhaustive so every branch is
/// loggable and a new state can't be silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepOutcome {
    /// Killed a live ephemeral orphan and removed its record.
    ReapedEphemeral { pid: u32, port: u16 },
    /// Killed the LIVE lane and removed its record. [`SweepMode::Shutdown`] only —
    /// at boot a live survivor is adoptable, at shutdown it is a leak.
    ReapedLive { pid: u32, port: u16 },
    /// Record named a pid that is no longer alive — stale file removed, no kill.
    RemovedDead { pid: u32 },
    /// Pid is alive but NOT a `llama-server` (reused number) — record removed, the
    /// unrelated process left untouched.
    RemovedReused { pid: u32 },
    /// A live-lane record — left for `lane_pidfile` to adopt-or-reap.
    LeftLive { pid: u32 },
    /// A `.lane` file that didn't parse — removed as garbage.
    RemovedUnparseable { path: PathBuf },
}

/// The canonical lanes directory: `~/.continuum/run/lanes/`. `None` only if there
/// is no home directory — a degenerate environment where accounting is disarmed.
pub fn lanes_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("run").join("lanes"))
}

/// Record a spawned lane at the canonical directory. A write failure DISARMS
/// future orphan-reclaim for this lane but must NEVER fail the serve (the server
/// is up; the record is a recovery aid). Returns the error so the caller can probe
/// it loud without aborting.
pub fn record(rec: &LaneRecord) -> std::io::Result<()> {
    let dir = lanes_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home dir for lane registry",
        )
    })?;
    record_in(&dir, rec)
}

/// Remove a lane's record on graceful teardown. Idempotent — a missing file is
/// success.
pub fn remove(pid: u32) {
    if let Some(dir) = lanes_dir() {
        remove_in(&dir, pid);
    }
}

/// Reap every orphaned ephemeral lane recorded by a crashed predecessor and
/// garbage-collect dead records. Resolves the canonical directory then delegates
/// to the pure [`sweep_in`]. Returns what it did, for the caller to log.
pub fn sweep_orphans() -> Vec<SweepOutcome> {
    match lanes_dir() {
        Some(dir) => sweep_in(&dir, SweepMode::Boot),
        None => Vec::new(),
    }
}

/// Reap EVERY lane this install owns — live and ephemeral — for shutdown.
///
/// `stop` reaps cores ([`crate::runtime::core_bind_guard`]) and owned engine
/// orphans, but until this existed it never touched a `llama-server`: the whole
/// registry was swept only on the NEXT boot, so shutting Continuum down left
/// every lane resident. Measured 2026-08-17 on the M5: an ephemeral 27B lane
/// (19 GB) and the live 14B lane were up simultaneously; the planner sized its
/// window against what was left and served citizens a 2,816-token context, which
/// cannot even hold the tool surface. `reboot` could not clear it — reboot is
/// stop + start, and neither half owned lanes.
///
/// Role-blind on purpose. The live/ephemeral split exists to decide ADOPTION, and
/// nothing is adoptable by a core that is exiting.
pub fn sweep_all() -> Vec<SweepOutcome> {
    match lanes_dir() {
        Some(dir) => sweep_in(&dir, SweepMode::Shutdown),
        None => Vec::new(),
    }
}

/// `<pid>.lane` under `dir`.
fn record_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.lane"))
}

/// Write `rec` as JSON to `<pid>.lane`, creating the directory if needed. Pure:
/// the caller owns the dir so tests never touch the real registry.
fn record_in(dir: &Path, rec: &LaneRecord) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    // A LIVE lane is SINGULAR by nature — one persona lane on the canonical port.
    // Recording a new live lane therefore SUPERSEDES any prior live record, which
    // must name a now-dead predecessor (its process was reclaimed on this boot).
    // Clearing it at write-time is what stops a stale live record from lingering:
    // `sweep_orphans` deliberately never reaps a live record (it defers to
    // `lane_pidfile`), so if the OS later recycled that dead pid the stale file
    // would otherwise never be cleaned. Ephemeral records are many-at-once and
    // clear nothing.
    if rec.role == LaneRole::Live {
        clear_live_records_in(dir);
    }
    let json = serde_json::to_string(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(record_path(dir, rec.pid), json)
}

/// Remove every existing LIVE-role record under `dir`. Called only when a fresh
/// live lane records itself, enforcing the one-live-lane invariant. A dir read
/// failure means "nothing to clear" — the create_dir_all in `record_in` runs
/// first, so the directory exists.
fn clear_live_records_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lane") {
            continue;
        }
        let is_live = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
            .is_some_and(|r| r.role == LaneRole::Live);
        if is_live {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Remove `<pid>.lane` under `dir` if present. Idempotent.
fn remove_in(dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(record_path(dir, pid));
}

/// The pure sweep against an explicit `dir`. See [`sweep_orphans`] / [`sweep_all`].
fn sweep_in(dir: &Path, mode: SweepMode) -> Vec<SweepOutcome> {
    let mut outcomes = Vec::new();
    // A missing directory is the normal first-run / all-graceful-prior-shutdown
    // state — nothing to sweep, not a fallback.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return outcomes;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lane") {
            continue;
        }
        let Some(rec) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
        else {
            // Garbage file — remove it and move on, never act on a number we
            // can't trust.
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedUnparseable { path });
            continue;
        };

        // Dead record of ANY role → garbage-collect, never a kill on a recycled
        // number.
        if !lane_process::is_alive(rec.pid) {
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedDead { pid: rec.pid });
            continue;
        }

        // At BOOT a live survivor may be adoptable, so the decision defers to
        // `lane_pidfile`'s canonical-port reclaim. At SHUTDOWN nothing is
        // adoptable — this core is exiting — so every role is reaped.
        let reap = match (rec.role, mode) {
            (LaneRole::Ephemeral, _) => true,
            (LaneRole::Live, SweepMode::Shutdown) => true,
            // The live lane's port is `lane_pidfile`'s job (adopt-or-reap). Leave
            // both the process and its record; if `lane_pidfile` reaps it, the next
            // boot sees a dead pid here and GCs the file.
            (LaneRole::Live, SweepMode::Boot) => false,
        };
        if !reap {
            outcomes.push(SweepOutcome::LeftLive { pid: rec.pid });
            continue;
        }
        if lane_process::is_llama_server(rec.pid) {
            lane_process::kill9(rec.pid);
            let _ = std::fs::remove_file(&path);
            outcomes.push(match rec.role {
                LaneRole::Live => SweepOutcome::ReapedLive {
                    pid: rec.pid,
                    port: rec.port,
                },
                LaneRole::Ephemeral => SweepOutcome::ReapedEphemeral {
                    pid: rec.pid,
                    port: rec.port,
                },
            });
        } else {
            // Alive but not one of ours — a reused pid. Drop the stale
            // record; never signal an unrelated process.
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedReused { pid: rec.pid });
        }
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp lanes dir per test — NEVER the real `~/.continuum/run/lanes/`,
    /// so a live core's registry is untouched and parallel tests don't collide (#7
    /// isolation rule).
    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "continuum-lane-registry-test-{tag}-{}",
            std::process::id()
        ))
    }

    fn rec(pid: u32, port: u16, role: LaneRole) -> LaneRecord {
        LaneRecord {
            pid,
            port,
            role,
            model: "test-model".into(),
        }
    }

    // what this catches: record_in → the file exists and round-trips through JSON;
    // remove_in deletes it and is idempotent. The accounting is useless if a
    // recorded lane can't be read back or cleaned up.
    #[test]
    fn record_round_trips_and_remove_is_idempotent() {
        let dir = temp_dir("roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let r = rec(4242, 58200, LaneRole::Ephemeral);
        record_in(&dir, &r).expect("record");

        let raw = std::fs::read_to_string(record_path(&dir, 4242)).expect("read");
        let back: LaneRecord = serde_json::from_str(&raw).expect("parse");
        assert_eq!(back, r, "record round-trips through JSON");

        remove_in(&dir, 4242);
        assert!(!record_path(&dir, 4242).exists(), "removed");
        remove_in(&dir, 4242); // idempotent — second remove must not panic
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a LIVE lane is singular — recording a new live lane
    // SUPERSEDES a prior live record (the reclaimed predecessor), leaving exactly
    // one live record. Without this, a stale live record naming a dead-then-reused
    // pid would linger forever (sweep never reaps live records). Ephemeral records
    // present at the same time are untouched.
    #[test]
    fn recording_a_live_lane_supersedes_the_prior_live_record() {
        let dir = temp_dir("live-exclusive");
        let _ = std::fs::remove_dir_all(&dir);
        // Prior boot's live lane + an unrelated ephemeral record.
        record_in(&dir, &rec(24784, 58057, LaneRole::Live)).expect("old live");
        record_in(&dir, &rec(30000, 58201, LaneRole::Ephemeral)).expect("ephemeral");
        // New boot's live lane records itself.
        record_in(&dir, &rec(24975, 58057, LaneRole::Live)).expect("new live");

        assert!(
            !record_path(&dir, 24784).exists(),
            "prior live record superseded"
        );
        assert!(record_path(&dir, 24975).exists(), "new live record present");
        assert!(
            record_path(&dir, 30000).exists(),
            "ephemeral record untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: THE safety invariant — an ALIVE ephemeral record whose pid
    // is NOT a llama-server (here our own test-runner pid) is NEVER killed; sweep
    // returns RemovedReused, drops the stale file, and our process survives. A
    // regression to "kill whatever the registry names" would SIGKILL an unrelated
    // reused-pid process — the blind-kill this guards against.
    #[test]
    fn sweep_never_kills_a_non_llama_ephemeral() {
        let dir = temp_dir("reused");
        let _ = std::fs::remove_dir_all(&dir);
        let me = std::process::id();
        record_in(&dir, &rec(me, 58200, LaneRole::Ephemeral)).expect("record");

        let outcomes = sweep_in(&dir, SweepMode::Boot);
        assert_eq!(outcomes, vec![SweepOutcome::RemovedReused { pid: me }]);
        assert!(
            lane_process::is_alive(me),
            "sweep must never kill a non-llama pid"
        );
        assert!(!record_path(&dir, me).exists(), "stale record removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a live LIVE-role record is LEFT for lane_pidfile — sweep
    // must not touch the persona lane's process OR its record (the two mechanisms
    // must compose, not double-reap the living persona's own server).
    #[test]
    fn sweep_leaves_the_live_lane_alone() {
        let dir = temp_dir("live");
        let _ = std::fs::remove_dir_all(&dir);
        let me = std::process::id();
        record_in(&dir, &rec(me, 58057, LaneRole::Live)).expect("record");

        let outcomes = sweep_in(&dir, SweepMode::Boot);
        assert_eq!(outcomes, vec![SweepOutcome::LeftLive { pid: me }]);
        assert!(
            record_path(&dir, me).exists(),
            "live record left in place for lane_pidfile"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a record naming a definitely-dead pid is garbage-collected
    // (RemovedDead), never a kill attempt on a recycled number — regardless of role.
    #[test]
    fn sweep_removes_dead_records() {
        let dir = temp_dir("dead");
        let _ = std::fs::remove_dir_all(&dir);
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        record_in(&dir, &rec(dead, 58200, LaneRole::Ephemeral)).expect("record");

        let outcomes = sweep_in(&dir, SweepMode::Boot);
        // (Tiny PID-reuse window is acceptable in a unit test; assert the invariant
        // "a reaped pid is GC'd or safely treated as reused, never a kill+reap of
        // the living".)
        assert!(
            matches!(outcomes.as_slice(), [SweepOutcome::RemovedDead { pid }] if *pid == dead)
                || matches!(outcomes.as_slice(), [SweepOutcome::RemovedReused { .. }]),
            "dead pid must be RemovedDead (or a safe RemovedReused), got {outcomes:?}"
        );
        assert!(!record_path(&dir, dead).exists(), "stale record removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: the boot/shutdown difference, on the ONE record where they
    // disagree. A LIVE record is LEFT at boot (it may be adoptable) and REAPED at
    // shutdown (nothing is adoptable by a core that is exiting). Regression for the
    // 2026-08-17 M5 incident: `stop` never swept lanes at all, so shutting Continuum
    // down left llama-servers holding VRAM and `reboot` (= stop + start) could not
    // clear them. Uses OUR OWN pid as the recorded lane: it is definitely alive and
    // definitely NOT a llama-server, so the never-blind-kill guard must classify it
    // RemovedReused under Shutdown — proving the shutdown path still refuses to
    // signal a process it cannot positively identify, while boot still returns
    // LeftLive without even looking. Asserting we are still alive afterwards is the
    // real safety claim.
    #[test]
    fn shutdown_reaps_the_live_lane_that_boot_leaves_alone() {
        let me = std::process::id();

        let boot_dir = temp_dir("mode-boot");
        let _ = std::fs::remove_dir_all(&boot_dir);
        record_in(&boot_dir, &rec(me, 58057, LaneRole::Live)).expect("record");
        assert_eq!(
            sweep_in(&boot_dir, SweepMode::Boot),
            vec![SweepOutcome::LeftLive { pid: me }],
            "boot defers the live lane to lane_pidfile's adopt-or-reap"
        );
        assert!(
            record_path(&boot_dir, me).exists(),
            "boot must KEEP the live record so the next pass can still see it"
        );

        let stop_dir = temp_dir("mode-shutdown");
        let _ = std::fs::remove_dir_all(&stop_dir);
        record_in(&stop_dir, &rec(me, 58057, LaneRole::Live)).expect("record");
        let outcomes = sweep_in(&stop_dir, SweepMode::Shutdown);
        assert!(
            !matches!(outcomes.as_slice(), [SweepOutcome::LeftLive { .. }]),
            "shutdown must NEVER leave a live lane running, got {outcomes:?}"
        );
        assert_eq!(
            outcomes,
            vec![SweepOutcome::RemovedReused { pid: me }],
            "our own pid is alive but is not a llama-server — the never-blind-kill \
             guard must hold on the shutdown path too"
        );
        assert!(
            lane_process::is_alive(me),
            "the shutdown sweep must never signal a non-llama process"
        );
        assert!(!record_path(&stop_dir, me).exists(), "record cleared");

        let _ = std::fs::remove_dir_all(&boot_dir);
        let _ = std::fs::remove_dir_all(&stop_dir);
    }

    // what this catches: an unparseable `.lane` file is removed as garbage, never
    // acted on — a corrupt record can't wedge the sweep or trigger a bogus kill.
    #[test]
    fn sweep_removes_unparseable_files() {
        let dir = temp_dir("garbage");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let junk = dir.join("99999.lane");
        std::fs::write(&junk, "not json at all").expect("write junk");

        let outcomes = sweep_in(&dir, SweepMode::Boot);
        assert_eq!(
            outcomes,
            vec![SweepOutcome::RemovedUnparseable { path: junk.clone() }]
        );
        assert!(!junk.exists(), "garbage file removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a missing registry dir is a clean no-op (normal first
    // boot), not an error or a panic.
    #[test]
    fn sweep_missing_dir_is_noop() {
        let dir = temp_dir("absent");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(sweep_in(&dir, SweepMode::Boot).is_empty());
    }
}
