//! lane_pidfile.rs — the canonical-port RECLAIM primitive for the live serving lane.
//!
//! ## The bug this exists to kill
//!
//! The live persona lane ([`crate::inference::llama_server`]) is the host's ONE
//! `llama-server`, pinned to the canonical port. Its `Drop` kills the child — but
//! `Drop` runs only on a GRACEFUL exit. A SIGKILLed / panicked / power-cut core
//! never runs `Drop`, so its `llama-server` child SURVIVES as an orphan still
//! holding the canonical port and still resident on the GPU.
//!
//! The old behavior made that catastrophic: the fresh core's port resolver
//! *scanned past* the held canonical port to the next free one, so (a) it spawned
//! a SECOND model on the one GPU → Metal decode-time OOM → every deliberation
//! 500s and the persona silently abstains, AND (b) its own `/v1` probe pointed at
//! the empty scanned port, blinding it to the perfectly-good orphan it could have
//! adopted.
//!
//! ## The fix
//!
//! The live lane now PINS the canonical port (it never flees). On a clean boot
//! the port is free and it binds. When a crashed predecessor's orphan holds it,
//! [`reclaim`] runs on the live lane's first FRESH claim — inside
//! [`crate::inference::llama_server::LlamaServerControl::serve`], gated on not
//! already owning a child — and reaps that orphan so the bind succeeds. It runs
//! there, not at daemon init, on purpose: a HEALTHY orphan serving the right model
//! is adopted for free by the reconcile upstream (`AlreadyServing`, zero reload)
//! and never reaches `serve`, so the reap only fires when reconcile already
//! decided the orphan is unusable (wrong model/genome, or unreachable). The
//! pidfile is how we know WHICH pid to reap without OS-specific port→pid mapping:
//! the live lane records its child's pid here on every spawn, and clears it on
//! graceful teardown.
//!
//! ## Safety: never blind-kill
//!
//! A recorded pid can be STALE (the orphan already died and the OS reused its pid
//! for an unrelated process). So [`reclaim`] verifies the live pid is actually a
//! `llama-server` (via `ps -p <pid> -o comm=`) BEFORE sending `SIGKILL`. If it is
//! not — or if we cannot verify — we do NOT kill it; we drop the stale pidfile and
//! let the canonical-port bind fail loud downstream if a true unknown squatter
//! remains ([[fallbacks-are-illegal-fail-loud]]). We never silently flee to a
//! competitor port again.
//!
//! ## Test isolation
//!
//! The core operations are PURE and path-taking (`*_at`), so tests drive them
//! against a unique temp file and never read, write, or remove the real
//! `~/.continuum/run/llama-lane.pid` of a live core (the #7 `$HOME`-pollution
//! class of test bug). The public wrappers resolve the one canonical path and
//! delegate.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long [`reclaim`] waits for the canonical port to release after killing the
/// orphan before giving up and letting the downstream bind fail loud. Generous
/// enough for the OS to tear down a Metal-resident `llama-server` and release the
/// socket, bounded so a wedged kill can't hang serving-daemon init forever.
const PORT_RELEASE_BUDGET: Duration = Duration::from_secs(5);

/// The canonical pidfile path: `~/.continuum/run/llama-lane.pid`. Under
/// `~/.continuum/run` so it lives with the core's other per-user runtime state
/// (not `/tmp`, which other users and other tools share). `None` only if there is
/// no home directory — a degenerate environment where reclaim is simply disarmed.
pub fn pidfile_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("run").join("llama-lane.pid"))
}

/// Record the live lane's `llama-server` child pid at the canonical path so a
/// crashed-core successor can reclaim the port. Resolves the path then delegates
/// to the pure [`write_at`].
pub fn write(pid: u32) -> io::Result<()> {
    let path = pidfile_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home dir for lane pidfile"))?;
    write_at(&path, pid)
}

/// Remove the canonical pidfile (graceful teardown). Idempotent — a missing file
/// is success.
pub fn clear() {
    if let Some(path) = pidfile_path() {
        clear_at(&path);
    }
}

/// Reclaim the canonical `port` from a crashed predecessor's orphaned
/// `llama-server`, if the pidfile names one. Resolves the canonical path then
/// delegates to the pure [`reclaim_at`].
pub async fn reclaim(port: u16) -> ReclaimOutcome {
    match pidfile_path() {
        Some(path) => reclaim_at(&path, port).await,
        None => ReclaimOutcome::NoPidfile,
    }
}

/// What a [`reclaim`] pass did — exhaustive so the daemon can log every branch and
/// a new state can't be silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReclaimOutcome {
    /// No pidfile (or it was unparseable garbage, now removed). The normal
    /// first-run / graceful-prior-shutdown state — NOT a fallback.
    NoPidfile,
    /// The pidfile named a pid that is no longer alive — stale file removed.
    AlreadyGone { pid: u32 },
    /// The pid is alive but is NOT a `llama-server` (reused pid, or unverifiable).
    /// We did NOT kill it; the stale pidfile was removed. A true unknown squatter
    /// on the port — if any — surfaces at bind time, loud.
    NotOurProcess { pid: u32, comm: String },
    /// Killed our orphaned `llama-server` and the canonical port is free again.
    Reclaimed { pid: u32 },
    /// Killed it but the port had not released within [`PORT_RELEASE_BUDGET`].
    /// Surfaced so the downstream bind failure has context (still fail-loud).
    KilledButPortBusy { pid: u32 },
}

/// Write `pid` to `path`, creating the parent directory if needed. Pure: the
/// caller owns the path so tests never touch the real pidfile.
fn write_at(path: &Path, pid: u32) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, pid.to_string())
}

/// Read and parse the pid at `path`. `None` for an absent or unparseable file.
fn read_at(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

/// Remove `path` if present. Idempotent.
fn clear_at(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The pure reclaim decision against an explicit `path`. See [`reclaim`].
async fn reclaim_at(path: &Path, port: u16) -> ReclaimOutcome {
    // A file that exists but doesn't parse is garbage — clear it and treat as
    // "nothing to reclaim" rather than acting on a number we can't trust.
    let Some(pid) = read_at(path) else {
        if path.exists() {
            clear_at(path);
        }
        return ReclaimOutcome::NoPidfile;
    };

    if !super::lane_process::is_alive(pid) {
        clear_at(path);
        return ReclaimOutcome::AlreadyGone { pid };
    }

    // The pid is alive — but is it OUR orphan, or a reused pid? Verify before we
    // ever send a signal. If we cannot positively identify it as a llama-server
    // (different command, or `ps` unavailable), we refuse to kill it.
    match super::lane_process::command_name(pid) {
        Some(comm) if comm.contains("llama-server") => {
            super::lane_process::kill9(pid);
            let freed = super::lane_process::wait_port_free(port, PORT_RELEASE_BUDGET).await;
            clear_at(path);
            if freed {
                ReclaimOutcome::Reclaimed { pid }
            } else {
                ReclaimOutcome::KilledButPortBusy { pid }
            }
        }
        other => {
            clear_at(path);
            ReclaimOutcome::NotOurProcess {
                pid,
                comm: other.unwrap_or_default(),
            }
        }
    }
}

// The unix-process helpers (`is_alive` / `kill9` / `command_name`) live in the
// shared `super::lane_process` module so the canonical-port reclaim here and the
// orphan-registry sweep in `super::lane_registry` obey ONE never-blind-kill
// primitive, not two divergent copies.

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp pidfile path per test, under the system temp dir — NEVER the
    /// real `~/.continuum/run/llama-lane.pid`, so a live core's reclaim is
    /// untouched and parallel tests don't collide (#7 isolation rule).
    fn temp_pidfile(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "continuum-lane-pidfile-test-{tag}-{}.pid",
            std::process::id()
        ))
    }

    // what this catches: write_at → read_at round-trips the pid and creates the
    // parent dir; clear_at removes it and is idempotent. The recovery primitive is
    // useless if the pid it records can't be read back.
    #[test]
    fn write_read_clear_round_trips() {
        let path = temp_pidfile("roundtrip");
        clear_at(&path); // start clean
        assert_eq!(read_at(&path), None, "absent file reads as None");

        write_at(&path, 12345).expect("write");
        assert_eq!(read_at(&path), Some(12345), "pid round-trips");

        clear_at(&path);
        assert_eq!(read_at(&path), None, "cleared file reads as None");
        clear_at(&path); // idempotent — second clear must not panic
    }

    // what this catches: no pidfile → NoPidfile, a clean no-op. Boot with nothing
    // to reclaim must NOT error or act.
    #[tokio::test]
    async fn reclaim_with_no_pidfile_is_noop() {
        let path = temp_pidfile("absent");
        clear_at(&path);
        assert_eq!(reclaim_at(&path, 0).await, ReclaimOutcome::NoPidfile);
    }

    // what this catches: THE safety invariant — a live pid that is NOT a
    // llama-server (here our own test-runner pid) is NEVER killed; reclaim returns
    // NotOurProcess and drops the stale file. A regression to "kill whatever the
    // pidfile names" would SIGKILL an unrelated reused-pid process — the exact
    // blind-kill this guards against. We assert our process is still alive after.
    #[tokio::test]
    async fn reclaim_never_kills_a_non_llama_process() {
        let path = temp_pidfile("reused");
        let me = std::process::id();
        write_at(&path, me).expect("write");

        let outcome = reclaim_at(&path, 0).await;
        match outcome {
            ReclaimOutcome::NotOurProcess { pid, .. } => assert_eq!(pid, me),
            other => panic!("expected NotOurProcess, got {other:?}"),
        }
        // We must still be alive — reclaim must not have signalled us.
        assert!(
            super::super::lane_process::is_alive(me),
            "reclaim must never kill a non-llama pid"
        );
        // Stale pidfile removed.
        assert_eq!(read_at(&path), None, "stale pidfile cleared");
    }

    // what this catches: a pidfile naming a definitely-dead pid → AlreadyGone and
    // the file is cleared, never a kill attempt on a recycled number.
    #[tokio::test]
    async fn reclaim_dead_pid_is_already_gone() {
        let path = temp_pidfile("dead");
        // Spawn a trivial child and reap it so its pid is dead before we reclaim.
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let dead = child.id();
        child.wait().expect("reap");
        write_at(&path, dead).expect("write");

        // (Tiny PID-reuse window between reap and here is acceptable in a unit
        // test; on the off chance it's reused by a non-llama process the outcome
        // is NotOurProcess, also a safe non-kill — so assert on "did not kill +
        // cleared", the actual invariant.)
        let outcome = reclaim_at(&path, 0).await;
        assert!(
            matches!(
                outcome,
                ReclaimOutcome::AlreadyGone { pid } if pid == dead
            ) || matches!(outcome, ReclaimOutcome::NotOurProcess { .. }),
            "dead pid must be AlreadyGone (or a safe NotOurProcess), got {outcome:?}"
        );
        assert_eq!(read_at(&path), None, "stale pidfile cleared");
    }
    // (is_alive-for-self now lives with the primitive in `super::lane_process`.)
}
