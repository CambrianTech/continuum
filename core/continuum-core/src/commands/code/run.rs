//! `code/run` — execute a snippet and return what actually happened.
//!
//! The persona's first real HAND (ACTING-ORGANISM.md, outlier-A). A mind that can
//! only TALK about code writes it blind; a mind with this hand can RUN its solution,
//! read the traceback, and correct — the difference between an automaton emitting
//! plausible prose and a citizen who knows whether her code works. The deliberation
//! faculty reaches for it via `Decision::Act { calls: [code/run …] }`; the act→observe
//! driver (step 3) runs it and re-admits the result as an Episodic engram, closing
//! the causal loop (act → observe → re-perceive). Standalone it is also directly
//! callable via `cu code/run` and every SDK — one file, zero wiring, AiSafe.
//!
//! SAFETY: runs the given code in a fresh temp dir under a wall-clock timeout (a
//! safety bound on a runaway process — NOT a clamp on the model). This is the
//! pragmatic floor for an OWNER's local dev machine, the same shape `cognition`'s
//! test-grader uses; it is explicitly NOT a sandbox. Before untrusted/public code
//! runs through this, it MUST move into a real sandbox (container/seccomp) — a P1
//! requirement, tracked, not silently assumed away.

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
    /// Language to run. Slice 1 = `python` (alias `py`). Any other value fails loud
    /// rather than guessing an interpreter.
    pub lang: String,
    /// The raw source to execute. Passed through verbatim — no markdown-fence
    /// stripping, no rewriting: the command runs exactly what it is given. (Cleaning
    /// up model formatting is the deliberation layer's job, never the hand's — a hand
    /// that second-guesses its input is a heuristic steering cognition.)
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
    const DESCRIPTION: &'static str =
        "Execute a code snippet and return its stdout, stderr, exit code, and \
         duration. Use this to actually RUN and test your own code instead of \
         guessing whether it works. Slice 1 supports python.";
    type Params = CodeRunParams;
    type Output = CodeRunResult;

    async fn run(&self, _ctx: &Ctx, params: CodeRunParams) -> Result<CodeRunResult, CommandError> {
        // Resolve the interpreter. Unknown language fails loud — never guess.
        let (ext, cmd) = match params.lang.as_str() {
            "python" | "py" => ("py", "python3"),
            other => {
                return Err(CommandError::Invalid(format!(
                    "code/run: unsupported lang '{other}' (slice 1 supports 'python' only)"
                )))
            }
        };

        let timeout = std::time::Duration::from_secs(
            params.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, MAX_TIMEOUT_SECS),
        );

        // Fresh temp dir per run, removed afterward. Writing to a file (vs `-c`)
        // avoids arg-length and shell-quoting hazards and matches the test-grader.
        let dir = std::env::temp_dir().join(format!("cu-coderun-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir)
            .map_err(|e| CommandError::Internal(format!("code/run: temp dir create failed: {e}")))?;
        let file = dir.join(format!("snippet.{ext}"));
        let write = std::fs::write(&file, &params.code);

        let started = std::time::Instant::now();
        let result = match write {
            Err(e) => Err(CommandError::Internal(format!("code/run: temp write failed: {e}"))),
            Ok(()) => {
                match tokio::time::timeout(
                    timeout,
                    // kill_on_drop is load-bearing: when the safety timeout fires,
                    // tokio::time::timeout drops the output() future. Without this,
                    // the spawned interpreter is NOT killed — it orphans to init and
                    // burns a core forever (observed: 6h+ runaway snippet.py at 100%).
                    // Dropping the Child with kill_on_drop set sends it SIGKILL.
                    tokio::process::Command::new(cmd)
                        .arg(&file)
                        .kill_on_drop(true)
                        .output(),
                )
                .await
                {
                    // Process ran to completion (clean or with a nonzero exit/traceback).
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
                    // Failed to spawn the interpreter at all (e.g. python3 absent).
                    Ok(Err(e)) => Err(CommandError::Internal(format!(
                        "code/run: failed to spawn '{cmd}': {e}"
                    ))),
                    // Safety timeout fired — report it as a result, not an error: a
                    // hung run IS information the mind should observe and react to.
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
        };

        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}
crate::register_stateless_command!(CodeRun);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the happy path — code that runs clean reports ok=true,
    // exit 0, and the actual stdout. This is the hand working: she can see what her
    // code printed instead of guessing.
    #[tokio::test]
    async fn runs_python_and_captures_stdout() {
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "python".into(),
                    code: "print(sum(range(5)))".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("ok");
        assert!(out.ok, "clean run is ok");
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "10", "captured the real stdout");
        assert!(!out.timed_out);
    }

    // what this catches: a traceback is RETURNED, not swallowed — exit nonzero, the
    // error text in stderr. This is the whole point: the mind must SEE the failure to
    // self-correct (the blind-coder fix). A regression that hid stderr would put her
    // back to coding blind.
    #[tokio::test]
    async fn surfaces_traceback_on_error() {
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "python".into(),
                    code: "raise ValueError('boom')".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("command itself succeeds even when the code fails");
        assert!(!out.ok, "code that raises is not ok");
        assert_ne!(out.exit_code, Some(0));
        assert!(out.stderr.contains("ValueError"), "the traceback is visible: {}", out.stderr);
        assert!(out.stderr.contains("boom"));
    }

    // what this catches: the safety timeout ACTUALLY kills the child process — not
    // just reports timed_out. Regression for the orphan leak: tokio::time::timeout
    // only drops the output() future; without kill_on_drop the interpreter survives,
    // orphans to init, and burns a core forever (observed: 6h+ runaway snippet.py at
    // 100% CPU). The old version of this test asserted only the return value, which
    // was already true WITH the leak — so it never caught the bug. This version
    // records the child PID and proves it is gone (or a reaped zombie) after the run.
    #[cfg(unix)]
    #[tokio::test]
    async fn safety_timeout_actually_kills_the_child() {
        let pidfile = std::env::temp_dir().join(format!("cu-coderun-pid-{}", uuid::Uuid::new_v4()));
        let code = format!(
            "import os\nwith open(r'{}', 'w') as f:\n    f.write(str(os.getpid()))\nwhile True:\n    pass\n",
            pidfile.display()
        );
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams { lang: "python".into(), code, timeout_secs: Some(1) },
            )
            .await
            .expect("timeout is a result, not an error");
        assert!(out.timed_out, "runaway was reported as timed out");
        assert!(!out.ok);
        assert_eq!(out.exit_code, None, "killed → no clean exit code");

        // The snippet wrote its PID before looping; the run took >=1s, so it exists.
        let pid = std::fs::read_to_string(&pidfile)
            .expect("snippet recorded its pid before the loop")
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

    // what this catches: unknown language fails LOUD (an error naming the cause),
    // never silently guesses an interpreter — the fail-loud doctrine at the hand.
    #[tokio::test]
    async fn unknown_lang_fails_loud() {
        let err = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "brainfuck".into(),
                    code: "++++".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect_err("must reject, not guess");
        assert!(format!("{err:?}").contains("unsupported lang"), "names the cause: {err:?}");
    }
}
