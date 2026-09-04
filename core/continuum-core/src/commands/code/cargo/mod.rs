//! `code/cargo/<verb>` — the persona's RUST hands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one verb per file.
//!
//! This is the affirmative answer to "get the persona coding well, in Rust": her
//! execution surface is `cargo`, not a scripting-language snippet runner. `cargo`
//! is BOTH the hand and the grader — she writes Rust, `code/cargo/check` hands her
//! the compiler's structured diagnostics (file, line, level, rendered block), she
//! fixes, `code/cargo/test` hands her pass/fail. The organic loop closes against
//! the real toolchain on the real repo, never a synthetic gym.
//!
//! ## Long-running and killable (the two facts that shaped the runner)
//!
//! Unlike the git hands (fast, blocking shell-outs), a cargo invocation runs for
//! seconds-to-minutes, so [`run_cargo`] is async (`tokio::process`) under a
//! wall-clock timeout, and — load-bearing — sets `kill_on_drop(true)` so a run the
//! timeout abandons is actually SIGKILLed, not orphaned to init burning a core
//! (the exact footgun that bit `code/run`). It also pins `CARGO_TARGET_DIR` to the
//! ONE shared cache so persona-driven builds never balloon ghost `target/` dirs.
//!
//! ## Identity + workspace
//!
//! Identity is the AUTHENTICATED caller (`ctx.caller.peer_id`); the run happens in
//! that caller's resolved workspace root (shared with the git hands via
//! [`workspace_root_for`](crate::commands::code::git::workspace_root_for)), the
//! `DashMap` guard dropped before the blocking-ish I/O.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::code::CodeState;
use crate::sdk_codegen::{CommandError, DynCommand};

pub mod check;
pub mod test;

use check::CargoCheck;
use test::CargoTest;

/// Default wall-clock budget for a cargo run, in seconds. Generous because a cold
/// build legitimately takes minutes; the persona can override per call.
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 180;
/// Hard cap on the wall-clock budget (30 min) — a runaway build can't wedge forever.
pub(crate) const MAX_TIMEOUT_SECS: u64 = 1800;

/// One compiler diagnostic, flattened from cargo's `--message-format=json` stream
/// into the shape a mind actually acts on: what went wrong, and where.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CargoDiagnostic.ts"
)]
pub struct CargoDiagnostic {
    /// `"error"` or `"warning"` (notes/help are folded into `rendered`, not surfaced
    /// as standalone diagnostics).
    pub level: String,
    /// The primary message line, e.g. ``cannot find value `x` in this scope``.
    pub message: String,
    /// The primary span's file, repo-relative as cargo reports it. `None` for a
    /// crate-level diagnostic with no span.
    #[ts(optional)]
    pub file: Option<String>,
    /// 1-based line of the primary span.
    #[ts(optional)]
    #[ts(type = "number")]
    pub line: Option<u32>,
    /// The full rendered block exactly as `rustc` would print it — the caret
    /// diagnostic the persona reads to self-correct.
    pub rendered: String,
}

/// Raw outcome of a single cargo invocation, before per-verb interpretation.
pub(crate) struct CargoRun {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// Pin cargo's artifact dir to the ONE shared cache (`$HOME/.continuum/cache/cargo-target`)
/// so persona-driven builds never scatter ghost `target/` dirs. `None` if `$HOME` is
/// unset — then cargo falls back to its own default rather than us guessing a path.
fn shared_target_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|h| format!("{h}/.continuum/cache/cargo-target"))
}

/// Run `cargo <args>` in `root` under a wall-clock timeout, capturing output.
///
/// `kill_on_drop(true)` is load-bearing: when the timeout fires, the output()
/// future is dropped and tokio SIGKILLs the child — without it a runaway build
/// orphans to init and burns a core. A timeout is reported as `timed_out: true`,
/// NOT an error: a hung build IS information the mind should observe and react to.
pub(crate) async fn run_cargo(
    root: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<CargoRun, CommandError> {
    let started = Instant::now();
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args(args)
        .current_dir(root)
        .env("CARGO_TERM_COLOR", "never")
        .kill_on_drop(true);
    if let Some(target) = shared_target_dir() {
        cmd.env("CARGO_TARGET_DIR", target);
    }
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(out)) => Ok(CargoRun {
            exit_code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            timed_out: false,
            duration_ms: started.elapsed().as_millis() as u64,
        }),
        Ok(Err(e)) => Err(CommandError::Internal(format!(
            "code/cargo: failed to spawn cargo (is it on PATH?): {e}"
        ))),
        Err(_) => Ok(CargoRun {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            duration_ms: started.elapsed().as_millis() as u64,
        }),
    }
}

/// Parse cargo's `--message-format=json` stdout into the error/warning diagnostics a
/// persona acts on. Each stdout line is one JSON object; only `compiler-message`
/// lines at error/warning level become diagnostics — `build-finished`, `artifact`,
/// and note/help lines are dropped so the surface is signal, not stream noise.
pub(crate) fn parse_diagnostics(stdout: &str) -> Vec<CargoDiagnostic> {
    let mut diags = Vec::new();
    for line in stdout.lines() {
        let v: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue, // not a JSON line (shouldn't happen on stdout, but be tolerant)
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let level = msg
            .get("level")
            .and_then(|l| l.as_str())
            .unwrap_or("")
            .to_string();
        if level != "error" && level != "warning" {
            continue;
        }
        let message = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        let rendered = msg
            .get("rendered")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        let (file, line_no) = msg
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|spans| {
                spans
                    .iter()
                    .find(|sp| {
                        sp.get("is_primary")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false)
                    })
                    .or_else(|| spans.first())
            })
            .map(|sp| {
                (
                    sp.get("file_name")
                        .and_then(|f| f.as_str())
                        .map(String::from),
                    sp.get("line_start")
                        .and_then(|l| l.as_u64())
                        .map(|n| n as u32),
                )
            })
            .unwrap_or((None, None));
        diags.push(CargoDiagnostic {
            level,
            message,
            file,
            line: line_no,
            rendered,
        });
    }
    diags
}

/// Summed pass/fail tally across every libtest binary (and doctest) a `cargo test`
/// run produced — the objective grade the persona reads after running her tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TestSummary {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    /// Fully-qualified names of the tests that FAILED, in report order — what the
    /// persona drills into next.
    pub failures: Vec<String>,
}

/// Parse libtest's human output (which streams through cargo's stdout even under
/// `--message-format=json`) into a summed pass/fail tally.
///
/// cargo runs N test binaries; each prints its own `test result: ...` line and its
/// own `test <name> ... FAILED` lines, so we ACCUMULATE across the whole stream
/// rather than reading one line. Counts come from the `test result:` summaries;
/// failed NAMES come from the per-test `... FAILED` lines (the summary only counts).
pub(crate) fn parse_test_summary(stdout: &str) -> TestSummary {
    let mut s = TestSummary::default();
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("test result:") {
            // e.g. " ok. 6 passed; 0 failed; 1 ignored; 0 measured; 4792 filtered out; ..."
            // The "ok." / "FAILED." verdict shares the first clause with the count, so
            // scan token PAIRS (a number followed by its kind) rather than assuming the
            // count is the first token. Kinds carry a trailing ';'/'.' to strip.
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            for pair in tokens.windows(2) {
                if let Ok(n) = pair[0].parse::<u32>() {
                    match pair[1].trim_end_matches([';', '.', ',']) {
                        "passed" => s.passed += n,
                        "failed" => s.failed += n,
                        "ignored" => s.ignored += n,
                        _ => {}
                    }
                }
            }
        } else if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|r| r.strip_suffix(" ... FAILED"))
        {
            // A failed test's name line — distinct from the summary; capture it so the
            // persona is handed WHICH test broke, not just a count.
            s.failures.push(name.trim().to_string());
        }
    }
    s
}

/// The dep-holding cargo command objects [`CodeModule`](crate::modules::code::CodeModule)
/// contributes to the kernel's typed object map, one per verb file, sharing the
/// caller's `Arc<CodeState>`.
pub fn command_objects(state: Arc<CodeState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CargoCheck {
            state: state.clone(),
        }),
        Arc::new(CargoTest { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the grader logic — an error diagnostic is flattened to
    // (level, primary file, primary line, rendered) so the persona sees WHERE to
    // fix. This is the value of the hand; if span extraction drifts she's handed a
    // message with no location.
    #[test]
    fn parses_error_diagnostic_with_primary_span() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"cannot find value `x` in this scope","rendered":"error[E0425]: cannot find value `x`\n --> src/lib.rs:42:5","spans":[{"file_name":"src/lib.rs","line_start":42,"is_primary":true}]}}"#;
        let d = parse_diagnostics(line);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].level, "error");
        assert_eq!(d[0].message, "cannot find value `x` in this scope");
        assert_eq!(d[0].file.as_deref(), Some("src/lib.rs"));
        assert_eq!(d[0].line, Some(42));
        assert!(d[0].rendered.contains("E0425"));
    }

    // what this catches: stream noise must NOT become diagnostics — build-finished,
    // artifact, and note/help lines are not errors. A regression here would flood
    // the persona with non-actionable "diagnostics".
    #[test]
    fn ignores_non_compiler_and_subordinate_lines() {
        let stdout = concat!(
            r#"{"reason":"compiler-artifact","target":{"name":"x"}}"#,
            "\n",
            r#"{"reason":"compiler-message","message":{"level":"note","message":"see issue","rendered":"note: ...","spans":[]}}"#,
            "\n",
            r#"{"reason":"build-finished","success":false}"#,
        );
        assert!(parse_diagnostics(stdout).is_empty());
    }

    // what this catches: counts are SUMMED across multiple `test result:` lines (each
    // test binary prints its own), and failed test NAMES are pulled from the per-test
    // `... FAILED` lines — so the persona gets both the tally and which test broke.
    #[test]
    fn sums_counts_and_collects_failed_names_across_binaries() {
        let stdout = concat!(
            "running 2 tests\n",
            "test math::adds ... ok\n",
            "test math::subtracts ... FAILED\n",
            "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n",
            "running 1 test\n",
            "test util::trims ... ok\n",
            "test result: ok. 1 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
        );
        let s = parse_test_summary(stdout);
        assert_eq!(s.passed, 2);
        assert_eq!(s.failed, 1);
        assert_eq!(s.ignored, 2);
        assert_eq!(s.failures, vec!["math::subtracts".to_string()]);
    }

    // what this catches: an all-green run yields zero failures and no failure names,
    // so `ok` downstream is computed from a clean tally rather than a false positive.
    #[test]
    fn all_passing_run_has_no_failures() {
        let stdout = "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 4792 filtered out; finished in 0.40s\n";
        let s = parse_test_summary(stdout);
        assert_eq!(s.passed, 6);
        assert_eq!(s.failed, 0);
        assert!(s.failures.is_empty());
    }
}
