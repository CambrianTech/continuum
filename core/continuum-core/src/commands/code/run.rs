//! `code/run` — compile and run a complete Rust program, return what actually happened.
//!
//! The persona's first real HAND (ACTING-ORGANISM.md, outlier-A). A mind that can
//! only TALK about code writes it blind; a mind with this hand can RUN its solution,
//! read the compiler errors or the panic, and correct — the difference between an
//! automaton emitting plausible prose and a citizen who knows whether her code works.
//! The deliberation faculty reaches for it via `Decision::Act { calls: [code/run …] }`;
//! the act→observe driver (step 3) runs it and re-admits the result as an Episodic
//! engram, closing the causal loop (act → observe → re-perceive). Standalone it is also
//! directly callable via `cu code/run` and every SDK — one file, zero wiring, AiSafe.
//!
//! RUST ONLY. This organism builds the Rust substrate it runs on; its throwaway-snippet
//! hand is `rustc`, never an interpreter. The persona hands a complete program (with its
//! own `fn main`); we `rustc` it and run the binary. For workspace-scoped grading use
//! `code/cargo/{check,test}`; this hand is the standalone "does this little program do
//! what I think?" probe, the same `rustc` shape `cognition/eval.rs`'s gym grader uses.
//!
//! SAFETY: compiles and runs in a fresh temp dir under a per-step wall-clock timeout (a
//! safety bound on a runaway process — NOT a clamp on the model). This is the pragmatic
//! floor for an OWNER's local dev machine; it is explicitly NOT a sandbox. Before
//! untrusted/public code runs through this, it MUST move into a real sandbox
//! (container/seccomp) — a P1 requirement, tracked, not silently assumed away.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// A safety bound on a runaway child process — not a limit on the model's thinking
/// or output. Overridable per call within reason; the command caps the override so
/// a typo can't wedge a process for an hour on the owner's machine.
const DEFAULT_TIMEOUT_SECS: u64 = 10;
const MAX_TIMEOUT_SECS: u64 = 60;

/// Params for `code/run`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/code/CodeRunParams.ts")]
pub struct CodeRunParams {
    /// Language to run. `rust` (alias `rs`) only — any other value fails loud rather
    /// than guessing a toolchain. This is a Rust organism; its exec hand is `rustc`.
    pub lang: String,
    /// A COMPLETE Rust program (with its own `fn main`) to compile and run. Passed
    /// through verbatim — no markdown-fence stripping, no wrapping: the command runs
    /// exactly what it is given. (Cleaning up model formatting is the deliberation
    /// layer's job, never the hand's — a hand that second-guesses its input is a
    /// heuristic steering cognition.)
    pub code: String,
    /// Optional wall-clock safety timeout in seconds (default 10, hard cap 60).
    #[serde(default)]
    #[ts(optional)]
    pub timeout_secs: Option<u64>,
}

/// Result of `code/run` — the ground truth of what running the code produced.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/code/CodeRunResult.ts")]
pub struct CodeRunResult {
    /// Process exit code; `None` if the process was killed (timeout / signal).
    #[ts(optional)]
    pub exit_code: Option<i32>,
    /// Whether exit_code == Some(0) — the at-a-glance "did it run clean?".
    pub ok: bool,
    /// Captured stdout (full — the model owns how much it reads, we don't truncate).
    pub stdout: String,
    /// Captured stderr (the traceback she needs to see to self-correct).
    pub stderr: String,
    /// Wall-clock duration of the run in milliseconds.
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// True if the run was killed by the safety timeout rather than exiting on its own.
    pub timed_out: bool,
}

/// `code/run` — run a snippet, report stdout/stderr/exit/duration. Stateless, AiSafe.
#[derive(Default)]
pub struct CodeRun;

#[async_trait]
impl ActionCommand for CodeRun {
    const NAME: &'static str = "code/run";
    const ALIASES: &'static [&'static str] = &["run_code"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Compile and run a complete Rust program (lang \"rust\", code must have its own \
         `fn main`) and return its stdout, stderr, exit code, and duration. A compile \
         error comes back as the result (ok=false) with rustc's errors in stderr. Use \
         this to actually RUN and test your own code instead of guessing whether it \
         works. For workspace-scoped grading use code/cargo/check and code/cargo/test.";
    type Params = CodeRunParams;
    type Output = CodeRunResult;

    async fn run(&self, _ctx: &Ctx, params: CodeRunParams) -> Result<CodeRunResult, CommandError> {
        // Rust only — this is a Rust organism's hand. Unknown language fails loud,
        // never guesses a toolchain.
        match params.lang.as_str() {
            "rust" | "rs" => {}
            other => {
                return Err(CommandError::Invalid(format!(
                    "code/run: unsupported lang '{other}' (Rust only — give a complete Rust program)"
                )))
            }
        }

        let timeout = std::time::Duration::from_secs(
            params.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, MAX_TIMEOUT_SECS),
        );

        // Fresh temp dir per run, removed afterward. The code is written verbatim — no
        // fence-stripping, no wrapping: the persona hands a complete program and we run
        // exactly that (cleaning model formatting is the deliberation layer's job, never
        // the hand's — a hand that second-guesses its input is a heuristic steering
        // cognition).
        let dir = std::env::temp_dir().join(format!("cu-coderun-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir)
            .map_err(|e| CommandError::Internal(format!("code/run: temp dir create failed: {e}")))?;

        let result = compile_and_run_rust(&dir, &params.code, timeout).await;
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

/// Compile the given complete Rust program with `rustc`, then run the produced
/// binary — each step under `timeout` with `kill_on_drop(true)`. Returns the ground
/// truth of what happened: a COMPILE error is reported as the run result (ok=false,
/// rustc's stderr), not hidden — the persona reads the compiler's errors and
/// self-corrects exactly as she would a runtime panic. `Err` is reserved for a
/// failure to spawn the toolchain at all (e.g. `rustc` absent).
async fn compile_and_run_rust(
    dir: &std::path::Path,
    code: &str,
    timeout: std::time::Duration,
) -> Result<CodeRunResult, CommandError> {
    let src = dir.join("snippet.rs");
    let bin = dir.join("snippet");
    std::fs::write(&src, code)
        .map_err(|e| CommandError::Internal(format!("code/run: temp write failed: {e}")))?;

    let started = std::time::Instant::now();

    // 1. Compile. kill_on_drop bounds a runaway rustc; a non-success exit is a RESULT
    //    (the persona must SEE the compiler errors), only a spawn failure is an error.
    let mut rustc = tokio::process::Command::new("rustc");
    rustc
        .arg("--edition")
        .arg("2021")
        .arg("-o")
        .arg(&bin)
        .arg(&src)
        .kill_on_drop(true);
    match tokio::time::timeout(timeout, rustc.output()).await {
        Ok(Ok(out)) if out.status.success() => {} // compiled — fall through to run
        Ok(Ok(out)) => {
            return Ok(CodeRunResult {
                exit_code: out.status.code(),
                ok: false,
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: false,
            })
        }
        Ok(Err(e)) => {
            return Err(CommandError::Internal(format!(
                "code/run: failed to spawn rustc (is it on PATH?): {e}"
            )))
        }
        Err(_) => {
            return Ok(CodeRunResult {
                exit_code: None,
                ok: false,
                stdout: String::new(),
                stderr: format!("rustc killed by safety timeout after {}s", timeout.as_secs()),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: true,
            })
        }
    }

    // 2. Run the compiled binary. kill_on_drop is load-bearing: when the safety
    //    timeout fires, tokio::time::timeout drops the output() future. Without it the
    //    child is NOT killed — it orphans to init and burns a core forever (observed:
    //    6h+ runaway at 100% CPU). Dropping the Child with kill_on_drop sends SIGKILL.
    let mut child = tokio::process::Command::new(&bin);
    child.kill_on_drop(true);
    match tokio::time::timeout(timeout, child.output()).await {
        // Ran to completion (clean, or a nonzero exit / panic on stderr).
        Ok(Ok(out)) => {
            let code = out.status.code();
            Ok(CodeRunResult {
                exit_code: code,
                ok: code == Some(0),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: false,
            })
        }
        Ok(Err(e)) => Err(CommandError::Internal(format!(
            "code/run: failed to spawn compiled binary: {e}"
        ))),
        // Safety timeout fired — a RESULT, not an error: a hung run IS information
        // the mind should observe and react to.
        Err(_) => Ok(CodeRunResult {
            exit_code: None,
            ok: false,
            stdout: String::new(),
            stderr: format!("killed by safety timeout after {}s", timeout.as_secs()),
            duration_ms: started.elapsed().as_millis() as u64,
            timed_out: true,
        }),
    }
}
crate::register_stateless_command!(CodeRun);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the happy path — a complete Rust program that compiles and
    // runs clean reports ok=true, exit 0, and the actual stdout. This is the hand
    // working: she can see what her code printed instead of guessing.
    #[tokio::test]
    async fn compiles_and_runs_rust_capturing_stdout() {
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "rust".into(),
                    code: "fn main() { println!(\"{}\", (0..5).sum::<i32>()); }".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("ok");
        assert!(out.ok, "clean run is ok: {}", out.stderr);
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "10", "captured the real stdout");
        assert!(!out.timed_out);
    }

    // what this catches: a runtime panic is RETURNED, not swallowed — exit nonzero, the
    // panic message in stderr. This is the whole point: the mind must SEE the failure to
    // self-correct (the blind-coder fix). A regression that hid stderr would put her
    // back to coding blind.
    #[tokio::test]
    async fn surfaces_panic_on_runtime_error() {
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "rust".into(),
                    code: "fn main() { panic!(\"boom\"); }".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("command itself succeeds even when the code panics");
        assert!(!out.ok, "code that panics is not ok");
        assert_ne!(out.exit_code, Some(0));
        assert!(out.stderr.contains("panicked"), "the panic is visible: {}", out.stderr);
        assert!(out.stderr.contains("boom"));
    }

    // what this catches: a COMPILE error comes back as a RESULT (ok=false, rustc's
    // errors in stderr), not a CommandError. The persona reads the compiler's message
    // and self-corrects exactly as she would a panic — hiding compile errors behind an
    // Err would make `code/run` unusable as a learning hand.
    #[tokio::test]
    async fn surfaces_compile_error_as_result() {
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "rust".into(),
                    code: "fn main() { let _x: i32 = \"not an int\"; }".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("a compile failure is a result, not a command error");
        assert!(!out.ok, "code that doesn't compile is not ok");
        assert_ne!(out.exit_code, Some(0), "rustc exits nonzero");
        assert!(out.stderr.contains("error"), "rustc's diagnostics are visible: {}", out.stderr);
        assert!(out.stdout.is_empty(), "the binary never ran, so no program stdout");
    }

    // what this catches: the safety timeout ACTUALLY kills the child process — not
    // just reports timed_out. Regression for the orphan leak: tokio::time::timeout
    // only drops the output() future; without kill_on_drop the child survives, orphans
    // to init, and burns a core forever (observed: 6h+ runaway at 100% CPU). The old
    // version of this test asserted only the return value, which was already true WITH
    // the leak — so it never caught the bug. This version records the child PID and
    // proves it is gone (or a reaped zombie) after the run.
    #[cfg(unix)]
    #[tokio::test]
    async fn safety_timeout_actually_kills_the_child() {
        let pidfile = std::env::temp_dir().join(format!("cu-coderun-pid-{}", uuid::Uuid::new_v4()));
        // A complete Rust program that records its PID then spins forever. timeout_secs=2
        // gives rustc room to compile (the loop is what overruns and must be killed).
        let code = format!(
            "use std::io::Write;\nfn main() {{\n    let mut f = std::fs::File::create(r\"{}\").unwrap();\n    write!(f, \"{{}}\", std::process::id()).unwrap();\n    f.flush().unwrap();\n    loop {{ std::hint::spin_loop(); }}\n}}\n",
            pidfile.display()
        );
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams { lang: "rust".into(), code, timeout_secs: Some(2) },
            )
            .await
            .expect("timeout is a result, not an error");
        assert!(out.timed_out, "runaway was reported as timed out");
        assert!(!out.ok);
        assert_eq!(out.exit_code, None, "killed → no clean exit code");

        // The program wrote its PID before looping; the run took >=2s, so it exists.
        let pid = std::fs::read_to_string(&pidfile)
            .expect("program recorded its pid before the loop")
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&pidfile);

        // Poll for the child to die. A LIVE infinite loop reports state 'R'/'S'; a
        // killed child is gone (empty) or a not-yet-reaped zombie ('Z'). With the
        // leak it stays 'R' forever and this loop exhausts its budget → test fails.
        let mut dead = false;
        for _ in 0..40 {
            let state = std::process::Command::new("ps")
                .args(["-o", "state=", "-p", &pid])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            if state.is_empty() || state.starts_with('Z') {
                dead = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(dead, "child pid {pid} survived the safety timeout — orphan leak regressed");
    }

    // what this catches: a non-Rust language fails LOUD (an error naming the cause),
    // never silently guesses a toolchain — the fail-loud doctrine at the hand, and the
    // permanent no-Python-on-the-exec-path rule. `python` must be rejected, not run.
    #[tokio::test]
    async fn non_rust_lang_fails_loud() {
        let err = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "python".into(),
                    code: "print(1)".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect_err("must reject, not guess");
        assert!(format!("{err:?}").contains("unsupported lang"), "names the cause: {err:?}");
    }
}
