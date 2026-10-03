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
    /// Exited before the deadline. `code` is `None` when a signal ended it.
    Exited { code: Option<i32>, stdout: String, stderr: String },
    /// Still running at the deadline; killed and reaped.
    TimedOut,
    /// Could not be started.
    Unstartable { error: String },
}

/// Wait for `child` until `timeout`; at the deadline kill it and reap it, so it can
/// never act after its caller has given up on it. `None` = it was killed. The one
/// deadline loop in this module: [`capture`] and [`probe`] are built on it, and a
/// caller that owns a child with its own stdio (a log file, no pipes) uses it directly.
pub fn wait_bounded(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            // Past the deadline, or the status could not be read: either way the child
            // is not ours to leave running. Kill, then reap, and say if either failed.
            outcome => {
                let killed = child.kill();
                let reaped = child.wait();
                outcome?;
                if let Err(e) = reaped {
                    return Err(std::io::Error::new(e.kind(), format!("killed at the deadline but not reaped: {e}")));
                }
                // A kill that failed because the child had already exited is fine; it is reaped.
                drop(killed);
                return Ok(None);
            }
        }
    }
}

/// Run `program args…` within `timeout` and keep its exit code, stdout and stderr:
/// for a caller that must tell one failure from another by the program's own words.
pub fn capture(program: &str, args: &[&str], timeout: Duration) -> Captured {
    let started = Instant::now();
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Captured::Unstartable { error: e.to_string() },
    };
    // Drain both pipes while the child runs: a child that writes more than a pipe
    // holds must not stall into a false timeout, and a descendant that inherited a
    // pipe must not hold this call past its deadline after the child exits.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    match wait_bounded(&mut child, timeout) {
        Ok(Some(status)) => {
            let left = timeout.saturating_sub(started.elapsed());
            Captured::Exited {
                code: status.code(),
                // What a pipe still held open by a descendant has not delivered by the
                // deadline is not waited for; the status is the answer.
                stdout: stdout.recv_timeout(left).unwrap_or_default(),
                stderr: stderr.recv_timeout(left).unwrap_or_default(),
            }
        }
        Ok(None) => Captured::TimedOut,
        Err(e) => Captured::Unstartable { error: e.to_string() },
    }
}

/// Read a pipe to EOF on its own thread; the text arrives on the returned channel.
/// A thread blocked on a pipe a descendant holds ends when that pipe closes; the
/// caller never waits on it past its own deadline.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = tx.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    rx
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

    // what this catches (review of #4672): output was read only after exit, so a child
    // writing more than a pipe holds stalled into a false timeout, and a descendant
    // holding the pipe kept the call past its deadline after the child exited.
    #[test]
    #[cfg(unix)]
    fn capture_drains_large_output_and_ignores_a_descendant_holding_the_pipe() {
        match capture("sh", &["-c", "head -c 300000 /dev/zero | tr '\\0' x"], Duration::from_secs(10)) {
            Captured::Exited { code, stdout, .. } => {
                assert_eq!(code, Some(0));
                assert_eq!(stdout.len(), 300_000, "output beyond a pipe's capacity must be drained, not timed out");
            }
            other => panic!("large output must not read as {other:?}"),
        }
        let started = Instant::now();
        let outcome = capture("sh", &["-c", "sleep 30 & echo parent-done"], Duration::from_secs(2));
        assert!(matches!(outcome, Captured::Exited { code: Some(0), .. }), "{outcome:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a descendant holding the pipe held capture for {:?}",
            started.elapsed()
        );
    }

    // what this catches: telling failures apart by the program's own words needs its
    // exit code and stderr, which `probe` discards.
    #[test]
    #[cfg(unix)]
    fn capture_keeps_the_exit_code_and_stderr() {
        match capture("sh", &["-c", "echo out; echo why >&2; exit 113"], Duration::from_secs(10)) {
            Captured::Exited { code, stdout, stderr } => {
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
