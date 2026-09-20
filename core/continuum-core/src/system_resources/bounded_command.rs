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

/// Run `program args…`, and return within `timeout` NO MATTER WHAT.
///
/// A child still alive at the deadline is killed and reaped. The return value
/// always names which of the three things happened; there is no path that
/// blocks forever and none that loses the distinction between "answered no",
/// "never answered", and "was never there".
pub fn probe(program: &str, args: &[&str], timeout: Duration) -> Probed {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Probed::Unstartable {
                error: e.to_string(),
            }
        }
    };

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The child is gone; reading the pipe to EOF cannot block on it.
                let stdout = match child.wait_with_output() {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
                    // The child exited and we have its status; losing the bytes
                    // is a degraded answer, not a hang. Report what we know.
                    Err(_) => String::new(),
                };
                return Probed::Exited {
                    stdout,
                    success: status.success(),
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Best-effort: if kill fails the child is already dying, and
                    // either way WE are no longer waiting on it.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Probed::TimedOut;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                return Probed::Unstartable {
                    error: e.to_string(),
                }
            }
        }
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
