//! `code/run` — compile and run a complete Rust program, return what actually happened.
//!
//! The persona's first real HAND (ACTING-ORGANISM.md, outlier-A). A mind that can
//! only TALK about code writes it blind; a mind with this hand can RUN its solution,
//! read the compiler errors or the panic, and correct — the difference between an
//! automaton emitting plausible prose and a citizen who knows whether her code works.
//! The deliberation faculty reaches for it via `Decision::Act { calls: [code/run …] }`;
//! the act→observe driver (step 3) runs it and re-admits the result as an Episodic
//! engram, closing the causal loop (act → observe → re-perceive). Standalone it is also
//! directly callable via `uu code/run` and every SDK — one file, zero wiring, AiSafe.
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CodeRunParams.ts"
)]
pub struct CodeRunParams {
    /// Language to run: `rust` (alias `rs`) → `rustc` a complete program, or
    /// `python` (alias `py`/`python3`) → run a Python script. SWE-bench and most repos
    /// are Python — use `python`, never `rust` for Python code. Any other value fails
    /// loud naming both supported paths (use code/shell for anything else).
    pub lang: String,
    /// A COMPLETE program in the chosen `lang` (Rust needs its own `fn main`; Python is a
    /// plain script). Passed
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CodeRunResult.ts"
)]
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
    /// The interpreter / toolchain that ran the code, as a path or PATH name — so a
    /// "no module named numpy" is read against the environment it actually ran in
    /// (card 533c2d78: a held checkout's prepared env python when there is one, else
    /// the PATH `python3`; `rustc` for Rust).
    #[serde(default)]
    pub interpreter: String,
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
        "Run code and return its stdout, stderr, exit code, and duration. lang=\"rust\" \
         compiles a complete program (needs its own `fn main`); lang=\"python\" runs a \
         Python script (use this for Python tasks/repos — do NOT pass Python as lang=rust). \
         A compile error or traceback comes back as the result (ok=false) in stderr. Use it \
         to actually RUN and test your own code instead of guessing. For workspace-scoped \
         Rust grading use code/cargo/check and code/cargo/test. Commands (git, pytest, \
         find) go through code/shell, never a Python subprocess wrapper here.";
    type Params = CodeRunParams;
    type Output = CodeRunResult;

    async fn run(&self, ctx: &Ctx, params: CodeRunParams) -> Result<CodeRunResult, CommandError> {
        let timeout = std::time::Duration::from_secs(
            params
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .clamp(1, MAX_TIMEOUT_SECS),
        );
        // Rust + Python. Glass-boxed 2026-08-24 from her own capture stream: she
        // reached for Python TWENTY-THREE times in one night — every analysis
        // scratch-tool she builds is Python — and the Rust-only refusal burned
        // each of those acts before she found the bash-heredoc workaround. A
        // hand that refuses the language its owner thinks in is harness
        // friction, not principle; every serious harness runs both. Unknown
        // languages still fail loud, now naming BOTH supported paths.
        match params.lang.as_str() {
            "rust" | "rs" => {}
            "python" | "python3" | "py" => {
                let dir =
                    std::env::temp_dir().join(format!("cu-coderun-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&dir).map_err(|e| {
                    CommandError::Internal(format!("code/run: temp dir create failed: {e}"))
                })?;
                // WHERE IT RUNS: her held checkout when she holds one (the same root her
                // shell and file hands use), else the scratch dir. Measured on the M5
                // 11:38–14:00Z (Joel: "rust > cpp > nodejs > python > shell"): 47 of 90 acts
                // were code/run, and every snippet walked the tree with ABSOLUTE paths
                // (`os.walk("/Users/…/workspace")`, `subprocess.run(["grep", …])`) — because
                // the snippet ran in a temp dir, relative paths meant nothing, and some
                // walked OTHER citizens' workspaces. Python is the right scratch language;
                // it must run where her hands stand.
                let cwd = crate::modules::code_commands::held_root_for(ctx).await;
                // THE HELD CHECKOUT'S ENVIRONMENT, NOT THE PATH's python3 (card 533c2d78,
                // finder Mara 2026-09-15): two citizens spent ~79-act loops on
                // matplotlib-24177 whose repro scripts died on "no module named numpy" —
                // the interpreter, not the bug. code/shell already runs in the prepared
                // env; the snippet runner ran the system python. Same resolver as the
                // [env] fact: the caller's rooted card checkout → its prepared env.
                let interpreter = crate::modules::code_commands::held_env_python_for(ctx)
                    .await
                    .unwrap_or_else(|| std::path::PathBuf::from("python3")); // unwrap_or_else: no held checkout or no prepared env = the PATH interpreter, named in the result
                let shape = script_shape(&params.code);
                crate::probe!(
                    class = "code.run.shape",
                    shape = shape.unwrap_or("program"),
                    rooted = cwd.is_some(),
                    chars = params.code.chars().count() as u64,
                    "what a python snippet is — a program, or the tree walked / a shell called from python"
                );
                let result = run_python(&dir, cwd.as_deref(), &params.code, timeout, &interpreter).await;
                let _ = std::fs::remove_dir_all(&dir);
                // THE FASTER HAND, NAMED IN THE RESULT (never a refusal — Joel 2026-09-15:
                // python over shell for anything cross-OS; the Rust hands over both). A
                // snippet that shells out or walks the tree is exploration, and the
                // in-process Rust hands do that in one act, rooted, through the repeat
                // guard: code/search (text/symbols), code/read (a file/range), code/list.
                return result.map(|mut r| {
                    if let Some(m) = shape {
                        r.stderr = format!(
                            "[hands] this snippet used `{m}` to explore the tree — your Rust hands do that in \
                             ONE act, rooted at your repo: code/search (text or a symbol), code/read (a file or \
                             range), code/list (a directory), code/shell (git, pytest). Keep code/run for a \
                             program you wrote: a repro, a small test.\n{}",
                            r.stderr
                        );
                    }
                    r
                });
            }
            other => return Err(CommandError::Invalid(format!(
                "code/run: unsupported lang '{other}' — supported: rust, python. \
                 For anything else use code/shell (any command; long runs hand back a handle)"
            ))),
        }

        // Fresh temp dir per run, removed afterward. The code is written verbatim — no
        // fence-stripping, no wrapping: the persona hands a complete program and we run
        // exactly that (cleaning model formatting is the deliberation layer's job, never
        // the hand's — a hand that second-guesses its input is a heuristic steering
        // cognition).
        let dir = std::env::temp_dir().join(format!("cu-coderun-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).map_err(|e| {
            CommandError::Internal(format!("code/run: temp dir create failed: {e}"))
        })?;

        let result = compile_and_run_rust(&dir, &params.code, timeout).await;
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

/// What a bounded child run produced — including everything it wrote BEFORE a
/// timeout killed it. `tokio::process::Child::wait_with_output` under
/// `tokio::time::timeout` discards the pipes on expiry, so a killed `find` sweep
/// came back as bare "timedOut" (QA from Joaquin, 2026-09-13, card ba846f38: the
/// longest dead-air windows of her session gave her nothing to act on). The
/// pipes are drained concurrently with the wait; on expiry the child is killed
/// and the drained bytes are returned with the verdict.
pub(crate) struct BoundedOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// `None` when the timeout killed the child.
    pub status: Option<std::process::ExitStatus>,
}

pub(crate) async fn run_bounded(
    mut child: tokio::process::Child,
    timeout: std::time::Duration,
) -> std::io::Result<BoundedOutput> {
    use tokio::io::AsyncReadExt as _;
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let drain_out = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf).await;
        }
        buf
    });
    let drain_err = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf).await;
        }
        buf
    });
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => Some(status?),
        Err(_) => {
            // Expired: kill, then let the drains reach EOF so the partial output is whole.
            let _ = child.start_kill();
            let _ = child.wait().await;
            None
        }
    };
    let stdout = drain_out.await.unwrap_or_default();
    let stderr = drain_err.await.unwrap_or_default();
    Ok(BoundedOutput { stdout, stderr, status })
}

/// The timeout verdict, appended AFTER whatever the child managed to write.
fn timeout_note(partial_stderr: &str, secs: u64, hint: &str) -> String {
    let mut s = partial_stderr.to_string();
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&format!("[killed: exceeded the {secs}s run timeout{hint} — the output above is everything it produced before the kill]"));
    s
}

/// Compile the given complete Rust program with `rustc`, then run the produced
/// binary — each step under `timeout` with `kill_on_drop(true)`. Returns the ground
/// truth of what happened: a COMPILE error is reported as the run result (ok=false,
/// rustc's stderr), not hidden — the persona reads the compiler's errors and
/// self-corrects exactly as she would a runtime panic. `Err` is reserved for a
/// failure to spawn the toolchain at all (e.g. `rustc` absent).
/// Run a complete Python program under the same wall-clock + kill_on_drop
/// contract as the Rust path. Same ground-truth shape: a traceback is the run
/// result (ok=false), never hidden; `Err` is reserved for a missing
/// interpreter. python3 resolves from PATH like rustc does.
/// The marker that makes a python snippet tree exploration rather than a program, if
/// any: `subprocess` / `os.system` / `os.popen` (a shell called from python), `os.walk`
/// / `os.listdir` / `os.scandir` / `glob.glob` / `os.path.exists` / pathlib's
/// `rglob` / `iterdir` (walking the tree). Reading a file with `open()` is a program
/// (a repro reads its fixture). Informational: the result names the faster hand.
pub(crate) fn script_shape(code: &str) -> Option<&'static str> {
    const MARKERS: [&str; 10] = [
        "subprocess", "os.system(", "os.popen(", "os.walk(", "os.listdir(", "os.scandir(",
        "glob.glob(", "os.path.exists(", ".rglob(", ".iterdir(",
    ];
    MARKERS.iter().copied().find(|m| code.contains(m))
}

async fn run_python(
    dir: &std::path::Path,
    cwd: Option<&std::path::Path>,
    code: &str,
    timeout: std::time::Duration,
    interpreter: &std::path::Path,
) -> Result<CodeRunResult, CommandError> {
    let src = dir.join("main.py");
    std::fs::write(&src, code)
        .map_err(|e| CommandError::Internal(format!("code/run: write failed: {e}")))?;
    let interpreter_label = interpreter.display().to_string();
    let mut cmd = tokio::process::Command::new(interpreter);
    crate::code::shell_session::strip_secret_env(&mut cmd);
    cmd.arg(&src)
        .current_dir(cwd.unwrap_or(dir))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| CommandError::Internal(format!("code/run: {interpreter_label} spawn failed: {e}")))?;
    let started = std::time::Instant::now();
    let out = run_bounded(child, timeout)
        .await
        .map_err(|e| CommandError::Internal(format!("code/run: python wait failed: {e}")))?;
    let duration_ms = started.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    match out.status {
        // Same honest shape as the Rust path: a timeout is a RESULT (she reads it
        // and adjusts), never a hidden kill — and it carries the partial output.
        None => Ok(CodeRunResult {
            ok: false,
            stdout,
            stderr: timeout_note(
                &stderr,
                timeout.as_secs(),
                &format!(
                    " (raise timeout_secs up to {MAX_TIMEOUT_SECS}s, or use code/shell for long-running work)"
                ),
            ),
            exit_code: None,
            duration_ms,
            timed_out: true,
            interpreter: interpreter_label.clone(),
        }),
        Some(status) => Ok(CodeRunResult {
            ok: status.success(),
            stdout,
            stderr,
            exit_code: status.code(),
            duration_ms,
            timed_out: false,
            interpreter: interpreter_label.clone(),
        }),
    }
}

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
    crate::code::shell_session::strip_secret_env(&mut rustc);
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
                interpreter: "rustc".to_string(),
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
                stderr: format!(
                    "rustc killed by safety timeout after {}s",
                    timeout.as_secs()
                ),
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: true,
                interpreter: "rustc".to_string(),
            })
        }
    }

    // 2. Run the compiled binary. kill_on_drop is load-bearing: when the safety
    //    timeout fires, tokio::time::timeout drops the output() future. Without it the
    //    child is NOT killed — it orphans to init and burns a core forever (observed:
    //    6h+ runaway at 100% CPU). Dropping the Child with kill_on_drop sends SIGKILL.
    let mut child = tokio::process::Command::new(&bin);
    crate::code::shell_session::strip_secret_env(&mut child);
    child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = child
        .spawn()
        .map_err(|e| CommandError::Internal(format!("code/run: failed to spawn compiled binary: {e}")))?;
    let out = run_bounded(child, timeout)
        .await
        .map_err(|e| CommandError::Internal(format!("code/run: binary wait failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    match out.status {
        // Ran to completion (clean, or a nonzero exit / panic on stderr).
        Some(status) => {
            let code = status.code();
            Ok(CodeRunResult {
                exit_code: code,
                ok: code == Some(0),
                stdout,
                stderr,
                duration_ms: started.elapsed().as_millis() as u64,
                timed_out: false,
                interpreter: "rustc".to_string(),
            })
        }
        // Safety timeout fired — a RESULT, not an error: a hung run IS information
        // the mind should observe and react to, and so is everything it printed first.
        None => Ok(CodeRunResult {
            exit_code: None,
            ok: false,
            stdout,
            stderr: timeout_note(&stderr, timeout.as_secs(), ""),
            duration_ms: started.elapsed().as_millis() as u64,
            timed_out: true,
            interpreter: "rustc".to_string(),
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

    // what this catches (card f25f4141): the snippet runner inherited the CORE's
    // environment — the core is started with `set -a; . config.env`, so HF_TOKEN was
    // one `std::env::var` away from any snippet, and tool output is quoted into rooms.
    // The same strip that guards the shell (#3786) guards every citizen-facing spawn:
    // a credential-shaped name is gone from the compiled program's environment, an
    // ordinary name is still there.
    #[tokio::test]
    async fn a_snippet_cannot_see_credential_shaped_variables_of_the_core() {
        std::env::set_var("CU_TEST_HF_TOKEN", "never-on-the-wire");
        std::env::set_var("CU_TEST_PLAIN_DIR", "visible");
        let out = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "rust".into(),
                    code: "fn main() { println!(\"{:?} {:?}\", std::env::var(\"CU_TEST_HF_TOKEN\").ok(), std::env::var(\"CU_TEST_PLAIN_DIR\").ok()); }".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("ok");
        assert!(out.ok, "{}", out.stderr);
        assert_eq!(out.stdout.trim(), "None Some(\"visible\")");
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
        assert!(
            out.stderr.contains("panicked"),
            "the panic is visible: {}",
            out.stderr
        );
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
        assert!(
            out.stderr.contains("error"),
            "rustc's diagnostics are visible: {}",
            out.stderr
        );
        assert!(
            out.stdout.is_empty(),
            "the binary never ran, so no program stdout"
        );
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
                CodeRunParams {
                    lang: "rust".into(),
                    code,
                    timeout_secs: Some(2),
                },
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
        assert!(
            dead,
            "child pid {pid} survived the safety timeout — orphan leak regressed"
        );
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
                    // PREMISE CHANGED 2026-08-24: python is now SUPPORTED (she
                    // reached for it 23 times in one night and every refusal
                    // burned an act). A genuinely unknown lang still fails loud.
                    lang: "cobol".into(),
                    code: "DISPLAY '1'.".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect_err("must reject, not guess");
        assert!(
            format!("{err:?}").contains("unsupported lang"),
            "names the cause: {err:?}"
        );
    }

    // what this catches: the python path actually RUNS a program end-to-end and
    // returns ground truth (stdout + ok) — the 2026-08-24 harness-friction fix
    // (23 refused python acts in one night) staying real, not just an accepted
    // lang string.
    #[tokio::test]
    async fn python_runs_and_returns_ground_truth() {
        let r = CodeRun
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "python".into(),
                    code: "print(2+2)".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("python must run");
        assert!(r.ok, "clean exit: {r:?}");
        assert_eq!(r.stdout.trim(), "4");
    }
    // what this catches: a timed-out run losing everything it printed before the kill
    // (QA from Joaquin, 2026-09-13, card ba846f38 — bare "timedOut" was the longest dead
    // air of her session). The partial stdout must ride with the timeout verdict.
    #[tokio::test]
    async fn a_timed_out_run_returns_what_it_printed_before_the_kill() {
        let dir = std::env::temp_dir().join(format!("code-run-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let out = run_python(
            &dir,
            None,
            "import sys, time\nprint('partial-evidence', flush=True)\nsys.stderr.write('warming\\n'); sys.stderr.flush()\ntime.sleep(30)\nprint('never')\n",
            std::time::Duration::from_secs(1),
            std::path::Path::new("python3"),
        )
        .await
        .expect("python3 on PATH");
        assert!(out.timed_out, "the run must report the kill");
        assert_eq!(out.interpreter, "python3", "the result names the interpreter that ran");
        assert!(out.stdout.contains("partial-evidence"), "partial stdout survives: {:?}", out.stdout);
        assert!(!out.stdout.contains("never"));
        assert!(out.stderr.contains("warming") && out.stderr.contains("killed: exceeded the 1s"), "stderr keeps the partial text AND the verdict: {:?}", out.stderr);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches (card 533c2d78, finder Mara): the snippet runner ran the PATH
    // python3 whatever checkout the caller held, so a repro in a held SWE checkout died
    // on "no module named numpy" — the interpreter, not the bug. With an interpreter
    // resolved for the run, THAT program runs and the result names it.
    #[tokio::test]
    async fn a_snippet_runs_under_the_interpreter_it_was_given_and_names_it() {
        let dir = std::env::temp_dir().join(format!("code-run-interp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let fake = dir.join("env-python");
        std::fs::write(&fake, "#!/bin/sh\necho from-the-prepared-env\n").unwrap(); // JUSTIFIED unwrap: test scaffolding
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap(); // JUSTIFIED unwrap: test scaffolding
        }
        let out = run_python(&dir, None, "print('never')", std::time::Duration::from_secs(5), &fake)
            .await
            .expect("fake interpreter spawns");
        assert!(out.ok);
        assert_eq!(out.stdout.trim(), "from-the-prepared-env");
        assert_eq!(out.interpreter, fake.display().to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches (2026-09-15, 47 of 90 acts on the M5): a snippet runs WHERE her
    // hands stand (relative paths resolve in the given cwd), and one that shells out or
    // walks the tree still runs but its result leads with the one-act Rust hand.
    #[tokio::test]
    async fn a_snippet_runs_in_the_given_root_and_tree_walking_is_named_not_refused() {
        assert_eq!(script_shape("import subprocess\nsubprocess.run(['grep','-r','x','.'])"), Some("subprocess"));
        assert_eq!(script_shape("for r,d,f in os.walk('.'): print(f)"), Some("os.walk("));
        assert_eq!(script_shape("src = open('a.py').read()\nassert 'def f' in src"), None, "reading a fixture is a program");
        let root = std::env::temp_dir().join(format!("code-run-root-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap(); // JUSTIFIED unwrap: test scaffolding
        std::fs::write(root.join("marker.txt"), b"here").unwrap(); // JUSTIFIED unwrap: test scaffolding
        let dir = root.join("scratch");
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let out = run_python(&dir, Some(&root), "print(open('marker.txt').read())", std::time::Duration::from_secs(10), std::path::Path::new("python3"))
            .await
            .expect("python3 on PATH");
        assert_eq!(out.stdout.trim(), "here", "relative paths resolve at the given root");
        let _ = std::fs::remove_dir_all(&root);
    }
}
