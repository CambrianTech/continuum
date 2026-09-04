//! `code/cargo/test` — run the caller's Rust tests and hand back a pass/fail grade.
//! This is the second half of the persona's self-correction loop: `code/cargo/check`
//! answers "does it compile?", `code/cargo/test` answers "does it WORK?" — with the
//! count of passes/failures, the names of what broke, and (on a build failure) the
//! same structured compiler diagnostics `check` returns.

use std::sync::Arc;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    parse_diagnostics, parse_test_summary, run_cargo, CargoDiagnostic, DEFAULT_TIMEOUT_SECS,
    MAX_TIMEOUT_SECS,
};
use crate::commands::code::git::workspace_root_for;
use crate::modules::code::CodeState;

/// How much of the combined cargo output to keep as context. Enough to carry the
/// `---- test::name stdout ----` panic blocks the persona needs to diagnose a
/// failure, without returning a multi-megabyte build log.
const OUTPUT_TAIL_BYTES: usize = 4000;

/// Inputs to `code/cargo/test`. All optional — the bare call runs the whole
/// workspace's tests with default features.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CargoTestParams.ts"
)]
pub struct CargoTestParams {
    /// Scope to one workspace package (`cargo test -p <package>`), e.g.
    /// `"continuum-core"`. Omit to test the whole workspace (much slower).
    #[ts(optional)]
    pub package: Option<String>,
    /// Comma-separated cargo features to enable (`--features <features>`), e.g.
    /// `"metal,accelerate"`. Omit for the crate's default features.
    #[ts(optional)]
    pub features: Option<String>,
    /// Name filter passed to the test harness (`cargo test <filter>`): only tests
    /// whose path contains this substring run, e.g. `"cognition::eval"`. Omit to run
    /// every test. Use this to grade just the code you changed — it is far faster.
    #[ts(optional)]
    pub filter: Option<String>,
    /// Wall-clock budget in seconds (default 180, hard-capped at 1800). A run that
    /// exceeds it is killed and returned with `timed_out: true`.
    #[ts(optional)]
    #[ts(type = "number")]
    pub timeout_secs: Option<u64>,
}

/// Result of a `cargo test` run: the verdict plus the tally, the names of failed
/// tests, any build-time compiler diagnostics, and a tail of the output for context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CargoTestResult.ts"
)]
pub struct CargoTestResult {
    /// `true` iff the build succeeded AND every test passed — "does it work?".
    pub ok: bool,
    /// `false` if the workspace failed to BUILD (so no tests ran). Distinguishes a
    /// compile break (read `diagnostics`) from a logic failure (read `failures`).
    pub compiled: bool,
    /// Count of passing tests, summed across every test binary.
    #[ts(type = "number")]
    pub passed: u32,
    /// Count of failing tests, summed across every test binary.
    #[ts(type = "number")]
    pub failed: u32,
    /// Count of ignored tests, summed across every test binary.
    #[ts(type = "number")]
    pub ignored: u32,
    /// Fully-qualified names of the tests that FAILED — what to drill into next.
    pub failures: Vec<String>,
    /// Compiler errors/warnings, populated when the build failed (empty on a clean
    /// build whose tests merely failed). Same shape as `code/cargo/check`.
    pub diagnostics: Vec<CargoDiagnostic>,
    /// Last few KB of combined stdout+stderr — carries the panic/assert blocks from
    /// failing tests so the failure is diagnosable, not just counted.
    pub output_tail: String,
    /// `true` if the run was killed by the safety timeout rather than finishing.
    pub timed_out: bool,
    /// Wall-clock duration of the run in milliseconds.
    #[ts(type = "number")]
    pub duration_ms: u64,
}

/// Keep the trailing `max` bytes of `s`, on a char boundary, prefixing an ellipsis
/// when truncated — the END of a test log holds the failure summary and panics.
fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len() - max;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

crate::action_command! {
    /// Run the Rust tests of the cargo project ON DISK you are editing with `cargo
    /// test` and get back a pass/fail grade: how many passed/failed, the NAMES of the
    /// tests that failed, and a tail of the output with the panic/assert messages. If
    /// the workspace doesn't compile, `compiled` is false and `diagnostics` carries the
    /// compiler errors (like `code/cargo/check`). This runs the tests of the project in
    /// your working directory — it CANNOT test a standalone function or snippet you have
    /// only written in chat; for that, write a `fn main` (or `#[test]` body) around it
    /// and run it with `code/run`. Scope to one package (`package`) and/or a name filter
    /// (`filter`, e.g. your module path) to grade just what you changed — far faster
    /// than the whole suite.
    pub struct CargoTest { state: Arc<CodeState> }
    name: "code/cargo/test",
    access: AiSafe,
    params: CargoTestParams,
    output: CargoTestResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let timeout = Duration::from_secs(
            p.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, MAX_TIMEOUT_SECS),
        );

        // `--message-format=json` gives us structured compiler diagnostics on a build
        // failure; libtest's human result lines stream through the same stdout and are
        // parsed separately. The optional filter is a positional arg to cargo itself.
        let mut args = vec!["test".to_string(), "--message-format=json".to_string()];
        if let Some(pkg) = &p.package {
            args.push("-p".to_string());
            args.push(pkg.clone());
        }
        if let Some(features) = &p.features {
            args.push("--features".to_string());
            args.push(features.clone());
        }
        if let Some(filter) = &p.filter {
            args.push(filter.clone());
        }

        let run = run_cargo(&root, &args, timeout).await?;
        let diagnostics = parse_diagnostics(&run.stdout);
        let build_errors = diagnostics.iter().filter(|d| d.level == "error").count();
        let compiled = build_errors == 0 && !run.timed_out;
        let summary = parse_test_summary(&run.stdout);

        let combined = format!("{}{}", run.stdout, run.stderr);
        let output_tail = tail(&combined, OUTPUT_TAIL_BYTES);

        Ok(CargoTestResult {
            ok: run.exit_code == Some(0) && !run.timed_out && summary.failed == 0,
            compiled,
            passed: summary.passed,
            failed: summary.failed,
            ignored: summary.ignored,
            failures: summary.failures,
            diagnostics,
            output_tail,
            timed_out: run.timed_out,
            duration_ms: run.duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name must mirror the file path so the persona
    // reaches the tool by the name it would guess (`code/cargo/test`).
    #[test]
    fn name_mirrors_path() {
        assert_eq!(CargoTest::NAME, "code/cargo/test");
    }

    // what this catches: the tail keeps the END of the log (where failures print) and
    // stays on a char boundary — a byte-slice mid-codepoint would panic.
    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        let s = "αβγδε".repeat(1000); // multi-byte chars, well over the cap
        let t = tail(&s, 8);
        assert!(t.starts_with('…'));
        assert!(t.ends_with('ε'));
        // round-trips as valid UTF-8 (no mid-codepoint slice)
        assert!(t.chars().count() > 1);
    }

    // what this catches: a short log is returned whole, with no ellipsis.
    #[test]
    fn tail_returns_short_input_unchanged() {
        assert_eq!(tail("ok", 4000), "ok");
    }
}
