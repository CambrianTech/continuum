//! CargoModule — `cargo/build` and `cargo/test` with structured output.
//!
//! Per [PERSONA-AS-DEVELOPER-GAP.md](../../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md)
//! Priority 2: Rust toolchain wrappers with structured envelopes,
//! closing the iteration-loop seam so a persona can build/test its
//! own scaffolded modules with the same feedback density a human
//! gets from `npm run build:ts` or `cargo test`.
//!
//! # What this module does
//!
//! Wraps cargo invocations with `--message-format=json` (for builds)
//! and parses the canonical JSON stream into typed
//! [`CargoMessage`](types::CargoMessage) diagnostics. For tests,
//! invokes cargo and parses libtest's human-readable output for
//! pass/fail/ignored counts plus failing test names.
//!
//! # Composability with the grid
//!
//! Both result types serialize to flat camelCase JSON envelopes. A
//! persona on machine A can call `cargo/test` against a module a
//! persona on machine B just authored — the result envelope routes
//! back over airc's grid without any cargo-specific protocol. The
//! grid substrate already handles the routing; this module makes
//! the wire shape grid-friendly. See
//! [[alignment-via-substrate-economics]].
//!
//! # What this module does NOT do
//!
//! - **Does NOT manage per-persona workspaces.** Takes optional
//!   `working_dir` (default: process cwd). The "self-improving
//!   Continuum" scenario (persona modifies repo → builds repo →
//!   tests repo) doesn't need per-persona workspaces; that's an
//!   orthogonal layer added later when multiple personas work on
//!   isolated worktrees.
//! - **Does NOT stream output line-by-line.** Returns a single
//!   envelope at the end. Streaming + `events/command-completed`
//!   are PERSONA-AS-DEVELOPER-GAP.md priorities 3+4 — separate
//!   PRs once the Stream cell shape implementation lands.
//! - **Does NOT cap cargo's own concurrency.** cargo manages its
//!   own target-dir lock; concurrent invocations against the same
//!   target dir serialize at cargo's level. Different target dirs
//!   stay fully parallel.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::Instant;

use crate::runtime::{
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModuleContext, ModulePriority,
    ServiceModule,
};

pub mod types;

use types::{
    CargoBuildParams, CargoBuildResult, CargoMessage, CargoSpan, CargoTestParams, CargoTestResult,
    BUILD_DEFAULT_TIMEOUT_MS, BUILD_MAX_TIMEOUT_MS, TEST_DEFAULT_TIMEOUT_MS, TEST_MAX_TIMEOUT_MS,
};

/// The cargo module. Stateless — every invocation is independent.
///
/// No per-resource locks: cargo handles its own target-dir locking
/// internally (multiple concurrent `cargo build` invocations against
/// the same target dir serialize at cargo's level; different target
/// dirs stay parallel). Per [field manual §4.1](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)
/// — when correctness lives below the module (cargo itself), the
/// module-level lock is unnecessary.
pub struct CargoModule {}

impl CargoModule {
    pub fn new() -> Self {
        Self {}
    }

    /// Run `cargo build` with `--message-format=json` and parse the
    /// JSON stream into structured diagnostics. Returns a typed
    /// envelope regardless of cargo's exit status — callers get
    /// errors/warnings even when build fails.
    pub async fn build(&self, params: CargoBuildParams) -> CargoBuildResult {
        let timeout = clamp_timeout(
            params.timeout_ms,
            BUILD_DEFAULT_TIMEOUT_MS,
            BUILD_MAX_TIMEOUT_MS,
        );
        let start = Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.arg("build").arg("--message-format=json");
        if let Some(pkg) = &params.package {
            cmd.arg("--package").arg(pkg);
        }
        if let Some(features) = &params.features {
            cmd.arg("--features").arg(features);
        }
        if params.release {
            cmd.arg("--release");
        }
        if let Some(dir) = &params.working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        match run_with_timeout(cmd, timeout).await {
            Ok((exit, stdout, _stderr)) => {
                let (errors, warnings) = parse_build_messages(&stdout);
                CargoBuildResult {
                    success: exit.map(|c| c == 0).unwrap_or(false) && errors.is_empty(),
                    errors,
                    warnings,
                    exit_code: exit,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: None,
                }
            }
            Err(e) => CargoBuildResult {
                success: false,
                errors: vec![],
                warnings: vec![],
                exit_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(e),
            },
        }
    }

    /// Run `cargo test` and parse libtest's human-readable output
    /// for pass/fail/ignored counts plus failing test names.
    ///
    /// We use the cargo-level `--message-format=json` for compile
    /// errors (those land in `build_errors`), then parse the inner
    /// libtest output text-style. `libtest`'s structured JSON
    /// requires nightly + `-Z unstable-options`, which the
    /// substrate doesn't depend on — regex parsing the stable
    /// human output is V1 sufficient.
    pub async fn test(&self, params: CargoTestParams) -> CargoTestResult {
        let timeout = clamp_timeout(
            params.timeout_ms,
            TEST_DEFAULT_TIMEOUT_MS,
            TEST_MAX_TIMEOUT_MS,
        );
        let start = Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.arg("test").arg("--message-format=json");
        if let Some(pkg) = &params.package {
            cmd.arg("--package").arg(pkg);
        }
        if params.lib_only {
            cmd.arg("--lib");
        }
        if let Some(features) = &params.features {
            cmd.arg("--features").arg(features);
        }
        if params.release {
            cmd.arg("--release");
        }
        // Filter goes AFTER `--` so libtest sees it.
        if let Some(filter) = &params.filter {
            cmd.arg("--").arg(filter);
        }
        if let Some(dir) = &params.working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        match run_with_timeout(cmd, timeout).await {
            Ok((exit, stdout, stderr)) => {
                let (build_errors, _build_warnings) = parse_build_messages(&stdout);
                let mut result = parse_test_output(&stdout, &stderr);
                result.build_errors = build_errors;
                result.exit_code = exit;
                result.duration_ms = start.elapsed().as_millis() as u64;
                // libtest's verdict: success iff cargo exited 0 AND no failures.
                // Build errors automatically give failed > 0 OR exit != 0.
                result.success = result.failed == 0
                    && result.build_errors.is_empty()
                    && exit.map(|c| c == 0).unwrap_or(false);
                result
            }
            Err(e) => CargoTestResult {
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(e),
                ..CargoTestResult::default()
            },
        }
    }
}

impl Default for CargoModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for CargoModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "cargo",
            priority: ModulePriority::Normal,
            command_prefixes: &["cargo/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "cargo/build" => {
                let req = CommandRequest::<CargoBuildParams>::from_value(params)?;
                let result = self.build(req.params).await;
                CommandResponse::ok(result).into_command_result()
            }
            "cargo/test" => {
                let req = CommandRequest::<CargoTestParams>::from_value(params)?;
                let result = self.test(req.params).await;
                CommandResponse::ok(result).into_command_result()
            }
            other => Err(format!(
                "{other}: not handled by cargo module — known commands are cargo/build, cargo/test"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn clamp_timeout(requested: Option<u64>, default: u64, max: u64) -> Duration {
    let ms = requested.unwrap_or(default).min(max);
    Duration::from_millis(ms)
}

/// Spawn `cmd`, wait with timeout, return `(exit_code, stdout_bytes,
/// stderr_bytes)`. Kills the child on timeout. Returns Err on spawn
/// failure or timeout — the typed envelope's `error` field surfaces
/// these to the caller.
async fn run_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<(Option<i32>, String, String), String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cargo spawn failed: {e}"))?;

    // Capture stdout + stderr concurrently with the wait.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut p) = stdout_pipe {
            let _ = p.read_to_end(&mut buf).await;
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut p) = stderr_pipe {
            let _ = p.read_to_end(&mut buf).await;
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("cargo wait failed: {e}")),
        Err(_) => {
            // Timeout — kill and report.
            let _ = child.kill().await;
            return Err(format!(
                "cargo timed out after {}ms",
                timeout.as_millis()
            ));
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    Ok((status.code(), stdout, stderr))
}

/// Parse cargo's `--message-format=json` stream. One JSON object per
/// line; we look for `"reason":"compiler-message"` entries and lift
/// their `message` payload into [`CargoMessage`].
pub(crate) fn parse_build_messages(stdout: &str) -> (Vec<CargoMessage>, Vec<CargoMessage>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        let envelope: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // tolerate non-JSON lines from cargo (rare but possible)
        };
        if envelope.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(diag) = envelope.get("message") else {
            continue;
        };

        let level = diag
            .get("level")
            .and_then(|l| l.as_str())
            .unwrap_or("")
            .to_string();
        let message = diag
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let code = diag
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(String::from);
        let rendered = diag
            .get("rendered")
            .and_then(|r| r.as_str())
            .map(String::from);

        // Primary span is the first span in `spans` with
        // `is_primary: true`. Spans without one are diagnostics
        // without a single anchor (linker errors etc.).
        let primary_span = diag
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|spans| {
                spans.iter().find(|s| {
                    s.get("is_primary")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
            })
            .map(parse_span);

        let msg = CargoMessage {
            level: level.clone(),
            message,
            code,
            primary_span,
            rendered,
        };
        match level.as_str() {
            "error" | "error: internal compiler error" => errors.push(msg),
            "warning" => warnings.push(msg),
            _ => {} // notes / help / unknown — skip
        }
    }
    (errors, warnings)
}

fn parse_span(v: &Value) -> CargoSpan {
    CargoSpan {
        file_name: v
            .get("file_name")
            .and_then(|f| f.as_str())
            .unwrap_or("")
            .to_string(),
        line_start: v
            .get("line_start")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        line_end: v
            .get("line_end")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        column_start: v
            .get("column_start")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
        column_end: v
            .get("column_end")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32,
    }
}

/// Parse libtest's human-readable output for pass/fail/ignored
/// counts + failing test names.
///
/// libtest's stable output looks like:
/// ```text
/// running 23 tests
/// test foo::bar ... ok
/// test foo::baz ... FAILED
/// ...
/// failures:
///     foo::baz
///
/// test result: ok. 22 passed; 1 failed; 0 ignored; 0 measured
/// ```
///
/// We scan stdout for the summary line + failures block. Multiple
/// "test result:" lines may appear (one per test binary); we
/// aggregate across all of them.
///
/// Inputs come from BOTH stdout AND stderr — libtest writes test
/// output to stdout but cargo writes some diagnostics to stderr.
pub(crate) fn parse_test_output(stdout: &str, stderr: &str) -> CargoTestResult {
    // Combine both streams since either may carry the summary in
    // edge cases (e.g. when cargo redirects). Order preserved:
    // stdout first since that's where libtest writes.
    let combined = format!("{stdout}\n{stderr}");

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut ignored = 0u32;
    let mut measured = 0u32;
    let mut failures: Vec<String> = Vec::new();

    let mut in_failures_block = false;

    for line in combined.lines() {
        let trimmed = line.trim();

        // Summary line: "test result: ok. 22 passed; 1 failed; 0 ignored; 0 measured; ..."
        if let Some(stripped) = trimmed.strip_prefix("test result: ") {
            let (p, f, i, m) = parse_summary_counts(stripped);
            passed += p;
            failed += f;
            ignored += i;
            measured += m;
            in_failures_block = false;
            continue;
        }

        // "failures:" marker enters the failures block. libtest
        // outputs TWO `failures:` blocks per failing binary: first
        // one lists `---- <name> stdout ----` markers + stdout
        // contents; second one lists indented test names alone. The
        // logic below captures from BOTH (deduped later) — test
        // names appear in both forms.
        if trimmed == "failures:" {
            in_failures_block = true;
            continue;
        }

        if in_failures_block {
            // Skip the `---- foo::b stdout ----` decorator lines —
            // we'll catch the bare `foo::b` in the trailing list.
            if trimmed.starts_with("---- ") {
                continue;
            }
            // Skip empty lines (between the two failures blocks +
            // around stdout dumps).
            if trimmed.is_empty() {
                continue;
            }
            // A test name looks like `module::path::name` — single
            // token (no spaces) with at least one `::`. That's the
            // strong filter that rejects panic messages, "note:"
            // lines, and other prose in the block.
            if !trimmed.contains(' ') && trimmed.contains("::") {
                failures.push(trimmed.to_string());
            }
            // Anything else inside the block (panic stdout, etc.)
            // we just skip; the next `test result:` or `failures:`
            // will reset state.
        }
    }

    // Deduplicate failures — libtest sometimes prints the failures
    // block twice (once per binary). Preserve first-seen order.
    let mut seen = std::collections::HashSet::new();
    failures.retain(|f| seen.insert(f.clone()));

    CargoTestResult {
        success: failed == 0,
        passed,
        failed,
        ignored,
        measured,
        failures,
        build_errors: vec![], // populated by caller after parse_build_messages
        exit_code: None,      // populated by caller
        duration_ms: 0,       // populated by caller
        error: None,
    }
}

/// Parse `"ok. 22 passed; 1 failed; 0 ignored; 0 measured"` or
/// `"FAILED. 22 passed; 1 failed; 0 ignored; 0 measured"` (the
/// entire substring AFTER "test result: "). Returns
/// `(passed, failed, ignored, measured)`.
///
/// The first chunk carries a verdict prefix (`ok.` or `FAILED.`)
/// before the first count — we scan WITHIN each chunk for the
/// `<int> <label>` pair rather than positionally requiring it at
/// indices 0 and 1.
fn parse_summary_counts(s: &str) -> (u32, u32, u32, u32) {
    let mut counts = (0u32, 0u32, 0u32, 0u32);
    for chunk in s.split(';').map(|c| c.trim()) {
        let tokens: Vec<&str> = chunk.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        // Scan for the FIRST integer token followed by a label
        // token. Handles both "22 passed" (tokens 0,1) and
        // "ok. 22 passed" (tokens 1,2).
        for i in 0..tokens.len() - 1 {
            if let Ok(n) = tokens[i].parse::<u32>() {
                let label = tokens[i + 1];
                match label {
                    "passed" => counts.0 = n,
                    "failed" => counts.1 = n,
                    "ignored" => counts.2 = n,
                    "measured" => counts.3 = n,
                    _ => {} // "filtered" etc. — skip
                }
                break; // one count per chunk
            }
        }
    }
    counts
}

// ════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════
//
// The cargo invocations themselves are slow + environment-dependent;
// the parsers are pure functions that take captured cargo output and
// emit typed envelopes. The substantive coverage lives there — fixture
// strings from real cargo runs exercise every diagnostic shape we
// expect to see.
//
// One end-to-end smoke test invokes `cargo --version` (always
// succeeds, fast) to verify the subprocess plumbing.
//
// The concurrency test fires N parallel `cargo --version`
// invocations through the module and asserts every result is
// internally consistent. Per [field manual §4.2](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md).

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── parse_build_messages ────────────────────────────────────────

    #[test]
    fn parse_build_extracts_errors_with_codes_and_spans() {
        // Realistic cargo --message-format=json line for an E0382.
        let line = json!({
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "message": "use of moved value: `x`",
                "code": { "code": "E0382" },
                "spans": [{
                    "file_name": "src/main.rs",
                    "is_primary": true,
                    "line_start": 5, "line_end": 5,
                    "column_start": 10, "column_end": 11,
                }],
                "rendered": "error[E0382]: use of moved value: `x`\n  --> src/main.rs:5:10\n",
            }
        });
        let stdout = format!("{line}\n");
        let (errors, warnings) = parse_build_messages(&stdout);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
        let e = &errors[0];
        assert_eq!(e.level, "error");
        assert_eq!(e.code.as_deref(), Some("E0382"));
        assert!(e.message.contains("moved value"));
        let span = e.primary_span.as_ref().expect("primary span present");
        assert_eq!(span.file_name, "src/main.rs");
        assert_eq!(span.line_start, 5);
        assert!(e.rendered.as_ref().unwrap().contains("E0382"));
    }

    #[test]
    fn parse_build_separates_warnings_from_errors() {
        let err = json!({
            "reason": "compiler-message",
            "message": { "level": "error", "message": "boom", "spans": [] }
        });
        let warn = json!({
            "reason": "compiler-message",
            "message": { "level": "warning", "message": "unused variable", "spans": [] }
        });
        let stdout = format!("{err}\n{warn}\n");
        let (errors, warnings) = parse_build_messages(&stdout);
        assert_eq!(errors.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(errors[0].level, "error");
        assert_eq!(warnings[0].level, "warning");
    }

    #[test]
    fn parse_build_ignores_non_diagnostic_reasons() {
        // cargo emits many message types — only compiler-message
        // carries diagnostics.
        let stdout = r#"
{"reason":"compiler-artifact","package_id":"foo"}
{"reason":"build-script-executed","package_id":"bar"}
{"reason":"build-finished","success":true}
"#;
        let (errors, warnings) = parse_build_messages(stdout);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn parse_build_tolerates_non_json_lines() {
        let stdout = "warning: some non-json line from cargo\n\n";
        let (errors, warnings) = parse_build_messages(stdout);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn parse_build_handles_diagnostic_without_primary_span() {
        // Some diagnostics (linker errors) have no primary span.
        let line = json!({
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "message": "linker error",
                "spans": [],
            }
        });
        let (errors, _) = parse_build_messages(&format!("{line}\n"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].primary_span.is_none());
    }

    // ── parse_test_output ───────────────────────────────────────────

    #[test]
    fn parse_test_extracts_passing_counts_from_summary() {
        let stdout = r#"
running 5 tests
test foo::a ... ok
test foo::b ... ok
test foo::c ... ok
test foo::d ... ok
test foo::e ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
"#;
        let r = parse_test_output(stdout, "");
        assert_eq!(r.passed, 5);
        assert_eq!(r.failed, 0);
        assert_eq!(r.ignored, 0);
        assert!(r.success);
        assert!(r.failures.is_empty());
    }

    #[test]
    fn parse_test_captures_failure_names_in_order() {
        let stdout = r#"
running 3 tests
test foo::a ... ok
test foo::b ... FAILED
test foo::c ... FAILED

failures:
    foo::b
    foo::c

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
"#;
        let r = parse_test_output(stdout, "");
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 2);
        assert_eq!(r.failures, vec!["foo::b", "foo::c"]);
        assert!(!r.success);
    }

    #[test]
    fn parse_test_aggregates_across_multiple_test_binaries() {
        // When cargo runs multiple test binaries, libtest prints
        // one summary per binary. The aggregate is the sum.
        let stdout = r#"
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured

test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured
"#;
        let r = parse_test_output(stdout, "");
        assert_eq!(r.passed, 10);
        assert_eq!(r.ignored, 1);
    }

    #[test]
    fn parse_test_dedupes_failures_across_repeated_blocks() {
        // Failures block sometimes appears twice (per-binary +
        // global summary). Dedup preserves first-seen order.
        let stdout = r#"
failures:
    foo::a
    foo::b

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured

failures:
    foo::a
    foo::b

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured
"#;
        let r = parse_test_output(stdout, "");
        // Counts aggregate (the summary appears twice) — that's fine,
        // it's a legitimate sum across binaries.
        assert_eq!(r.failed, 4);
        // But failure NAMES dedupe.
        assert_eq!(r.failures, vec!["foo::a", "foo::b"]);
    }

    #[test]
    fn parse_test_empty_output_returns_zero_counts_not_error() {
        let r = parse_test_output("", "");
        assert_eq!(r.passed, 0);
        assert_eq!(r.failed, 0);
        assert!(r.success, "zero failures = success (vacuously)");
    }

    // ── parse_summary_counts (the inner parser) ─────────────────────

    #[test]
    fn summary_counts_handles_filtered_out_field() {
        let (p, f, i, m) = parse_summary_counts("ok. 5 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out");
        assert_eq!((p, f, i, m), (5, 0, 0, 0));
    }

    #[test]
    fn summary_counts_handles_failed_verdict() {
        let (p, f, i, m) =
            parse_summary_counts("FAILED. 22 passed; 1 failed; 3 ignored; 0 measured");
        assert_eq!((p, f, i, m), (22, 1, 3, 0));
    }

    // ── timeout clamping ────────────────────────────────────────────

    #[test]
    fn timeout_uses_default_when_none_provided() {
        let d = clamp_timeout(None, BUILD_DEFAULT_TIMEOUT_MS, BUILD_MAX_TIMEOUT_MS);
        assert_eq!(d.as_millis() as u64, BUILD_DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn timeout_clamps_to_max_when_request_exceeds_it() {
        let d = clamp_timeout(
            Some(BUILD_MAX_TIMEOUT_MS + 1_000_000),
            BUILD_DEFAULT_TIMEOUT_MS,
            BUILD_MAX_TIMEOUT_MS,
        );
        assert_eq!(d.as_millis() as u64, BUILD_MAX_TIMEOUT_MS);
    }

    // ── handle_command dispatch ─────────────────────────────────────

    #[tokio::test]
    async fn handle_command_rejects_unknown_command_loud() {
        let m = CargoModule::new();
        let err = m
            .handle_command("cargo/run", json!({}))
            .await
            .expect_err("unknown cargo command must Err");
        assert!(err.contains("not handled by cargo module"));
        assert!(err.contains("cargo/build") && err.contains("cargo/test"));
    }

    #[test]
    fn config_advertises_cargo_prefix() {
        let m = CargoModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "cargo");
        assert_eq!(cfg.command_prefixes, &["cargo/"]);
    }

    // ── end-to-end smoke test (uses real cargo binary) ──────────────
    //
    // `cargo --version` always succeeds in any reasonable
    // environment + is fast. Use it to verify the subprocess
    // plumbing (spawn, capture, exit code) without relying on a
    // real Rust project being present.

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn end_to_end_subprocess_pipeline_works() {
        // Run `cargo --version` via the timeout helper directly,
        // since the public handlers only do build/test.
        let mut cmd = Command::new("cargo");
        cmd.arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let result = run_with_timeout(cmd, Duration::from_secs(30)).await;
        let (exit, stdout, _stderr) = result.expect("cargo --version must succeed");
        assert_eq!(exit, Some(0), "cargo --version exits 0");
        assert!(
            stdout.starts_with("cargo "),
            "stdout starts with 'cargo X.Y.Z': {stdout}"
        );
    }

    // ── concurrency stress test ─────────────────────────────────────
    //
    // Multi-thread tokio fires N parallel cargo --version invocations
    // through run_with_timeout (the production subprocess path).
    // Asserts every one returns a consistent (exit_code, stdout)
    // pair — no plumbing corruption under concurrent spawn/wait.
    //
    // Per [field manual §4.2](../../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md).

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_cargo_invocations_dont_corrupt_subprocess_pipeline() {
        const PARALLEL: usize = 8;
        let mut tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            tasks.push(tokio::spawn(async {
                let mut cmd = Command::new("cargo");
                cmd.arg("--version")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                run_with_timeout(cmd, Duration::from_secs(30)).await
            }));
        }
        let results: Vec<_> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        for (i, r) in results.iter().enumerate() {
            let (exit, stdout, _stderr) =
                r.as_ref().unwrap_or_else(|e| panic!("invocation {i} failed: {e}"));
            assert_eq!(
                *exit,
                Some(0),
                "concurrent invocation {i}: cargo --version must exit 0"
            );
            assert!(
                stdout.starts_with("cargo "),
                "concurrent invocation {i}: stdout corrupted: {stdout:?}"
            );
        }
    }
}
