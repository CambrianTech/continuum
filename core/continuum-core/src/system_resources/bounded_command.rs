//! A subprocess probe that ALWAYS returns, and always says which way it went.
//!
//! Four separate hangs on this grid in one night (2026-09-04/05) were the same
//! defect: a `std::process::Command::output()` on a boot-critical path with no
//! upper bound.
//!
//!   1. `install-llama-server.sh`'s verify awaited `--version` unbounded — a
//!      Metal-linked binary on a dual-GPU Intel Mac went to state `U` and
//!      survived `kill -9`; the node hosted zero citizens for an evening
//!      (card cd8f0bc7 / #3729).
//!   2. the serving debug-build gate awaited the same `--version` unbounded
//!      (#3719).
//!   3. `deploy-verify`'s ping, unbounded (#3718).
//!   4. `gpu::memory_manager::detect_gpu()` shelling `nvidia-smi`, then
//!      `vulkaninfo`, unbounded, PRE-BIND — the only one whose blast radius is
//!      the entire boot (#3732, this file's reason to exist).
//!
//! The first three were patched individually. A fourth point-patch would have
//! been the wrong shape, so the bound lives here once and the callers ask for
//! it by name.
//!
//! # Why this is synchronous
//!
//! [`crate::inference::llama_server`] bounds its probe with
//! `tokio::time::timeout` + `tokio::process`, which is the right tool INSIDE
//! the async runtime. `detect_gpu()` runs on the boot thread before the module
//! loop, where there is no runtime to await on — so this is a plain blocking
//! poll with a deadline. Same contract, different tier; do not "unify" them by
//! dragging tokio onto the pre-bind path.
//!
//! # Scope: small-output probes only
//!
//! Output is piped and read AFTER the child exits, so a child that writes more
//! than the OS pipe buffer (~64 KiB) before exiting would block itself and be
//! killed at the deadline rather than deadlocking us. That is the correct
//! outcome for a probe and the wrong tool for a command that streams. Probes
//! answer in a line or two — `nvidia-smi --query-gpu=…` prints one, and
//! `vulkaninfo --summary` a few dozen. Anything that streams belongs on the
//! async path with a drained reader, not here.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How often the deadline is checked while the child runs. Small enough that a
/// fast probe is not measurably delayed, large enough not to spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// What happened to a bounded probe. Every variant is a RECEIPT — there is no
/// "we don't know", because not knowing is what cost the four hangs above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probed {
    /// The child exited on its own before the deadline. Carries merged stdout.
    /// `success` is the child's own exit verdict — a program that runs and
    /// fails (no GPU present, say) is ABSENT, not broken, and the caller
    /// decides which it cares about.
    Exited { stdout: String, success: bool },
    /// The child was still running at the deadline and has been killed.
    TimedOut,
    /// The child could not be started at all — not installed, not executable,
    /// not on PATH. Distinct from `TimedOut` because it means something
    /// completely different about the host.
    Unstartable { error: String },
}

impl Probed {
    /// The single word that goes in a probe's `outcome` field.
    pub fn outcome(&self) -> &'static str {
        match self {
            Probed::Exited { success: true, .. } => "ok",
            Probed::Exited { success: false, .. } => "absent",
            Probed::TimedOut => "timed_out",
            Probed::Unstartable { .. } => "unstartable",
        }
    }

    /// Stdout if the child ran to completion successfully, else `None`. The
    /// ergonomic path for a caller that only wants the answer and treats every
    /// failure the same way.
    pub fn stdout_if_ok(&self) -> Option<&str> {
        match self {
            Probed::Exited {
                stdout,
                success: true,
            } => Some(stdout),
            _ => None,
        }
    }
}

/// What a [`capture`] saw: the exit code and both streams, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// Exited before the deadline. `code` is `None` when a signal ended it. The streams
    /// are a snapshot of at most [`CAPTURE_LIMIT`] bytes each, taken at exit;
    /// `truncated` says a stream held more (or a descendant was still writing).
    Exited { code: Option<i32>, stdout: String, stderr: String, truncated: bool },
    /// Still running at the deadline; it and, on Unix, its process group were killed
    /// and reaped.
    TimedOut,
    /// Could not be started, terminated or reaped; the error says which.
    Unstartable { error: String },
}

/// The most each [`capture`] stream keeps. Probes answer in a few lines; anything
/// past this is reported as `truncated`, never read without bound.
pub const CAPTURE_LIMIT: usize = 64 * 1024;

/// How long a killed child gets to be reaped before the caller is told its exit is
/// unconfirmed. A kill does not wait without bound (an uninterruptible exit can hang).
const REAP_GRACE: Duration = Duration::from_secs(2);

/// Wait for `child` until `timeout`; at the deadline kill it and reap it, so it can
/// never act after its caller has given up on it. `Ok(None)` = it was killed and its
/// exit observed. Every failure, including a status that cannot be read, still kills
/// and tries to reap first; an exit not observed within [`REAP_GRACE`] is an error.
pub fn wait_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => return kill_and_reap(child, false).map(|()| None),
            Err(e) => {
                let _ = kill_and_reap(child, false);
                return Err(e);
            }
        }
    }
}

/// Terminate, then observe the exit within [`REAP_GRACE`]. `group` kills the child's
/// whole process group on Unix (the child must lead one). Never a blocking `wait()`:
/// a child whose exit is not observed in time is reported, not waited on forever.
fn kill_and_reap(child: &mut std::process::Child, group: bool) -> std::io::Result<()> {
    let killed = if group { kill_group(child) } else { child.kill() };
    let deadline = Instant::now() + REAP_GRACE;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                match killed {
                    Err(e) => format!("could not terminate the child: {e}"),
                    Ok(()) => format!("killed, but its exit was not observed within {}s", REAP_GRACE.as_secs()),
                },
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn kill_group(child: &mut std::process::Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: killpg takes a process group id and a signal; no memory preconditions.
        if unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) } == 0 {
            return Ok(());
        }
        Err(std::io::Error::last_os_error())
    }
    #[cfg(not(unix))]
    {
        child.kill()
    }
}

/// Run `program args…` within ONE deadline and keep its exit code, stdout and stderr,
/// for a caller that must tell one failure from another by the program's own words.
///
/// Output goes to files this call owns, never pipes: nothing stalls on a full pipe, a
/// descendant that inherits the streams cannot hold the call past its deadline, and
/// there is no reader thread to leak. On Unix the child leads its own process group:
/// a timeout kills everything it started, and so does a normal exit, so no descendant
/// outlives the call. (Windows stops the child only; a probe there must not fork.)
pub fn capture(program: &str, args: &[&str], timeout: Duration) -> Captured {
    let deadline = Instant::now() + timeout;
    let (out, err) = match (CaptureFile::new(), CaptureFile::new()) {
        (Ok(out), Ok(err)) => (out, err),
        (Err(e), _) | (_, Err(e)) => return Captured::Unstartable { error: format!("capture files: {e}") },
    };
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    match (out.stdio(), err.stdio()) {
        (Ok(o), Ok(e)) => {
            command.stdout(o).stderr(e);
        }
        (Err(e), _) | (_, Err(e)) => return Captured::Unstartable { error: format!("capture files: {e}") },
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return Captured::Unstartable { error: e.to_string() },
    };
    let waited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => break kill_and_reap(&mut child, true).map(|()| None),
            Err(e) => {
                let _ = kill_and_reap(&mut child, true);
                break Err(e);
            }
        }
    };
    match waited {
        Ok(Some(status)) => {
            // A capture owns everything its child started (its process group on Unix):
            // a descendant left behind is stopped here, so nothing writes after return.
            #[cfg(unix)]
            let _ = kill_group(&mut child);
            let (stdout, out_more) = out.snapshot();
            let (stderr, err_more) = err.snapshot();
            Captured::Exited { code: status.code(), stdout, stderr, truncated: out_more || err_more }
        }
        Ok(None) => Captured::TimedOut,
        Err(e) => Captured::Unstartable { error: e.to_string() },
    }
}

/// A temp file one [`capture`] owns. On Unix it is unlinked at once (the open handle
/// keeps it); elsewhere it is removed when dropped.
struct CaptureFile {
    file: std::fs::File,
    #[cfg_attr(unix, allow(dead_code))]
    path: std::path::PathBuf,
}

impl CaptureFile {
    fn new() -> std::io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("continuum-capture-{}-{n}", std::process::id()));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;
        #[cfg(unix)]
        let _ = std::fs::remove_file(&path);
        Ok(Self { file, path })
    }

    fn stdio(&self) -> std::io::Result<Stdio> {
        self.file.try_clone().map(Stdio::from)
    }

    /// At most [`CAPTURE_LIMIT`] bytes from the start, read by position so the shared
    /// write offset a surviving descendant may still be using is never moved. `true`
    /// when the file held more than that.
    fn snapshot(&self) -> (String, bool) {
        let mut bytes = vec![0u8; CAPTURE_LIMIT + 1];
        let mut filled = 0;
        while filled < bytes.len() {
            #[cfg(unix)]
            let read = std::os::unix::fs::FileExt::read_at(&self.file, &mut bytes[filled..], filled as u64);
            #[cfg(windows)]
            let read = std::os::windows::fs::FileExt::seek_read(&self.file, &mut bytes[filled..], filled as u64);
            match read {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        let truncated = filled > CAPTURE_LIMIT;
        bytes.truncate(filled.min(CAPTURE_LIMIT));
        (String::from_utf8_lossy(&bytes).into_owned(), truncated)
    }
}

#[cfg(not(unix))]
impl Drop for CaptureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Run `program args…`, and return within `timeout` NO MATTER WHAT.
///
/// A child still alive at the deadline is killed and reaped. The return value
/// always names which of the three things happened; there is no path that
/// blocks forever and none that loses the distinction between "answered no",
/// "never answered", and "was never there".
pub fn probe(program: &str, args: &[&str], timeout: Duration) -> Probed {
    match capture(program, args, timeout) {
        Captured::Exited { code, stdout, .. } => Probed::Exited { stdout, success: code == Some(0) },
        Captured::TimedOut => Probed::TimedOut,
        Captured::Unstartable { error } => Probed::Unstartable { error },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the whole reason the file exists — a child that never
    // exits must not hold the caller past the deadline. Regression for #3732;
    // an unbounded `.output()` here hung a boot for 5.5 minutes until a
    // watchdog killed it.
    #[test]
    #[cfg(unix)]
    fn a_child_that_never_exits_returns_at_the_deadline() {
        let started = Instant::now();
        let outcome = probe("sleep", &["30"], Duration::from_millis(300));
        let elapsed = started.elapsed();

        assert_eq!(outcome, Probed::TimedOut);
        assert_eq!(outcome.outcome(), "timed_out");
        // Generous upper bound: proving it does not wait 30 s, not that the
        // timer is precise. A machine under load may overshoot the poll.
        assert!(
            elapsed < Duration::from_secs(5),
            "returned after {elapsed:?} — the deadline did not bound the wait"
        );
    }

    // what this catches: the bound must not cost correctness on the happy path.
    // A probe that always timed out would pass the test above.
    #[test]
    #[cfg(unix)]
    fn a_child_that_answers_returns_its_stdout() {
        let outcome = probe("echo", &["12345, TestGPU"], Duration::from_secs(10));
        match &outcome {
            Probed::Exited { stdout, success } => {
                assert!(*success, "echo should exit 0");
                assert!(
                    stdout.contains("12345, TestGPU"),
                    "stdout was {stdout:?} — the child's answer was lost"
                );
            }
            other => panic!("expected Exited, got {other:?}"),
        }
        assert_eq!(outcome.outcome(), "ok");
        assert_eq!(outcome.stdout_if_ok(), Some("12345, TestGPU\n"));
    }

    // what this catches: "not installed" collapsing into "timed out". On a host
    // with no nvidia-smi the boot must learn ABSENT immediately, not wait out
    // the full timeout — and must not report the same word as a driver hang.
    #[test]
    fn a_program_that_does_not_exist_is_unstartable_not_timed_out() {
        let started = Instant::now();
        let outcome = probe(
            "continuum-no-such-binary-3732",
            &[],
            Duration::from_secs(30),
        );
        assert!(
            matches!(outcome, Probed::Unstartable { .. }),
            "expected Unstartable, got {outcome:?}"
        );
        assert_eq!(outcome.outcome(), "unstartable");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a missing binary must fail fast, not burn the timeout"
        );
    }

    // what this catches: a program that RUNS and reports failure (no GPU on
    // this host) being conflated with one that could not run. detect_gpu must
    // be able to tell "asked, answered no" from "never asked".
    // what this catches (review of continuum #4672): a caller that gave up at its
    // deadline left the child running, so it could still act afterward and overlap the
    // next attempt. A child past the deadline must be killed before the call returns.
    #[test]
    #[cfg(unix)]
    fn a_child_past_its_deadline_is_killed_and_cannot_act_afterward() {
        let dir = std::env::temp_dir().join(format!("bounded-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let marker = dir.join("acted");
        let _ = std::fs::remove_file(&marker);
        let mut child = Command::new("sh")
            .args(["-c", &format!("sleep 1; touch '{}'", marker.display())])
            .spawn()
            .expect("sh runs");
        let waited = wait_bounded(&mut child, Duration::from_millis(100)).expect("wait");
        assert_eq!(waited, None, "the child outlived its deadline and must read as killed");
        std::thread::sleep(Duration::from_millis(1500));
        assert!(!marker.exists(), "a child killed at its deadline acted afterward");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches (reviews of #4672): output read after exit stalled a child that
    // wrote more than a pipe holds; a descendant holding inherited pipes kept the call
    // past its deadline (twice over, one wait per pipe) and leaked a reader thread each
    // time. Output now goes to owned files, so neither can happen, under one strict bound.
    #[test]
    #[cfg(unix)]
    fn capture_holds_one_deadline_with_large_output_or_a_descendant_on_the_streams() {
        match capture("sh", &["-c", "head -c 300000 /dev/zero | tr '\\0' x"], Duration::from_secs(10)) {
            Captured::Exited { code, stdout, truncated, .. } => {
                assert_eq!(code, Some(0));
                assert_eq!(stdout.len(), CAPTURE_LIMIT, "a large stream is kept up to the limit, not timed out");
                assert!(truncated, "the rest is reported, not silently dropped");
            }
            other => panic!("large output must not read as {other:?}"),
        }
        let bound = Duration::from_secs(2);
        let started = Instant::now();
        let outcome = capture("sh", &["-c", "sleep 30 & echo parent-done; echo why >&2"], bound);
        let elapsed = started.elapsed();
        match &outcome {
            Captured::Exited { code: Some(0), stdout, stderr, .. } => {
                assert_eq!(stdout.trim(), "parent-done");
                assert_eq!(stderr.trim(), "why");
            }
            other => panic!("{other:?}"),
        }
        assert!(elapsed < bound, "a descendant on the streams held capture for {elapsed:?} (bound {bound:?})");

        // A descendant that never stops writing BOTH streams: the snapshot is bounded,
        // the call keeps its deadline, and nothing reads without end.
        // The token is made at run time so no other process (an editor, a shell that
        // printed this source) can match it.
        let token = format!("capture-writer-{}-{:?}", std::process::id(), Instant::now());
        let script = format!("(while :; do echo {token}; echo {token} >&2; done) & sleep 0.3; echo parent-done");
        let started = Instant::now();
        let outcome = capture("sh", &["-c", &script], bound);
        let elapsed = started.elapsed();
        match &outcome {
            Captured::Exited { code: Some(0), stdout, stderr, .. } => {
                assert!(stdout.len() <= CAPTURE_LIMIT && stderr.len() <= CAPTURE_LIMIT);
                assert!(stdout.contains(&token) && stderr.contains(&token));
            }
            other => panic!("{other:?}"),
        }
        assert!(elapsed < bound, "a writing descendant held capture for {elapsed:?} (bound {bound:?})");
        std::thread::sleep(Duration::from_millis(200));
        let survivors = Command::new("pgrep").args(["-f", &token]).output().expect("pgrep");
        assert!(
            survivors.stdout.is_empty(),
            "a capture left its writing descendant running: pids {}",
            String::from_utf8_lossy(&survivors.stdout)
        );
    }

    // what this catches (review of #4672): at the deadline only the child was killed,
    // so what it started kept running and could act after the caller gave up.
    #[test]
    #[cfg(unix)]
    fn a_timeout_kills_the_whole_process_group() {
        let dir = std::env::temp_dir().join(format!("bounded-group-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let marker = dir.join("acted");
        let _ = std::fs::remove_file(&marker);
        let script = format!("(sleep 1; touch '{}') & sleep 30", marker.display());
        let outcome = capture("sh", &["-c", &script], Duration::from_millis(300));
        assert_eq!(outcome, Captured::TimedOut);
        std::thread::sleep(Duration::from_millis(1500));
        assert!(!marker.exists(), "a descendant of a timed-out capture acted afterward");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: telling failures apart by the program's own words needs its
    // exit code and stderr, which `probe` discards.
    #[test]
    #[cfg(unix)]
    fn capture_keeps_the_exit_code_and_stderr() {
        match capture("sh", &["-c", "echo out; echo why >&2; exit 113"], Duration::from_secs(10)) {
            Captured::Exited { code, stdout, stderr, .. } => {
                assert_eq!(code, Some(113));
                assert_eq!(stdout.trim(), "out");
                assert_eq!(stderr.trim(), "why");
            }
            other => panic!("expected Exited, got {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_child_that_exits_nonzero_is_absent_not_ok() {
        let outcome = probe("false", &[], Duration::from_secs(10));
        assert_eq!(outcome.outcome(), "absent");
        assert_eq!(
            outcome.stdout_if_ok(),
            None,
            "a failed probe must not hand its caller an answer"
        );
    }
}
