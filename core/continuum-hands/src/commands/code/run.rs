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
    /// the resolved system Python executable; `rustc` for Rust).
    #[serde(default)]
    pub interpreter: String,
}

/// `code/run` — run a snippet, report stdout/stderr/exit/duration. AiSafe; holds the code state for the caller's trust (the executor's policy).
pub struct CodeRun {
    pub state: std::sync::Arc<crate::modules::code::CodeState>,
}

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
                let dir = std::env::temp_dir().join(format!("cu-coderun-{}", uuid::Uuid::new_v4()));
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
                let cwd = crate::modules::code_commands::held_root_for(&self.state, ctx);
                // THE HELD CHECKOUT'S ENVIRONMENT, NOT THE PATH's python3 (card 533c2d78,
                // finder Mara 2026-09-15): two citizens spent ~79-act loops on
                // matplotlib-24177 whose repro scripts died on "no module named numpy" —
                // the interpreter, not the bug. code/shell already runs in the prepared
                // env; the snippet runner ran the system python. Same resolver as the
                // [env] fact: the caller's rooted card checkout → its prepared env.
                let interpreter =
                    match crate::modules::code_commands::held_env_python_for(&self.state, ctx) {
                        Some(interpreter) => Ok(interpreter),
                        None => crate::modules::python_adapter::find_python_async()
                            .await
                            .map_err(|error| CommandError::Internal(format!("code/run: {error}"))),
                    };
                let shape = script_shape(&params.code);
                continuum_base::probe!(
                    class = "code.run.shape",
                    shape = shape.unwrap_or("program"),
                    rooted = cwd.is_some(),
                    chars = params.code.chars().count() as u64,
                    "what a python snippet is — a program, or the tree walked / a shell called from python"
                );
                let hands = Hands::of(&self.state, ctx, cwd.as_deref().unwrap_or(&dir))?;
                let result = match interpreter {
                    Ok(interpreter) => {
                        run_python(&dir, cwd.as_deref(), &params.code, timeout, &interpreter, &hands).await
                    }
                    Err(error) => Err(error),
                };
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
            other => {
                return Err(CommandError::Invalid(format!(
                    "code/run: unsupported lang '{other}' — supported: rust, python. \
                 For anything else use code/shell (any command; long runs hand back a handle)"
                )))
            }
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

        let hands = Hands::of(&self.state, ctx, &dir)?;
        let result = compile_and_run_rust(&dir, &params.code, timeout, &hands).await;
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

/// `stderr` with everything the run's own output can't say about itself, on EVERY path (Codex
/// root on #5030 at 6c7bd0d82): a timeout and what it couldn't confirm gone, and how each stream
/// was read. A timeout's output is what was READ before the kill, never claimed as everything
/// it produced; a read that failed, or one a process it started held past the bound, is named
/// beside the verdict, never hidden behind it.
fn with_run_notes(mut stderr: String, out: &continuum_process::OwnedOutput, secs: u64, hint: &str) -> String {
    use continuum_process::{CaptureEnd, Ended};
    let mut note = |line: String| {
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            stderr.push('\n');
        }
        stderr.push_str(&line);
    };
    let timed_out = matches!(out.ended, Ended::TimedOut { .. });
    if let Ended::TimedOut { unconfirmed } = &out.ended {
        note(format!("[killed: exceeded the {secs}s run timeout{hint} — the output above is what was read before the kill]"));
        if !unconfirmed.is_empty() {
            note(format!("[not confirmed gone: {unconfirmed:?}]"));
        }
    }
    for (name, read) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
        match &read.end {
            CaptureEnd::Complete => {}
            CaptureEnd::Cut(_) if timed_out => {} // the kill's own cut, said above
            CaptureEnd::Cut(why) => note(format!("[{name} was cut: {why}]")),
            CaptureEnd::Failed(why) => note(format!("[{name} could not be read whole: {why}]")),
        }
    }
    stderr
}

/// THE one projection of an owned run into `code/run`'s result, for every arm (python, a
/// rustc failure or timeout, the compiled binary), so no arm can drop what was read (Codex root
/// on #5030: the rustc timeout returned an empty stdout it had captured). A Windows process
/// writes CRLF to a pipe; the rows she reads back are lines, so every result carries one shape
/// on every host (BigMama's note on #4427).
fn result_of(out: &continuum_process::OwnedOutput, interpreter: &str, duration_ms: u64, secs: u64, hint: &str) -> CodeRunResult {
    use continuum_process::Ended;
    let text = |read: &continuum_process::Captured| String::from_utf8_lossy(&read.bytes).replace("\r\n", "\n");
    let (exit_code, exited_clean, timed_out) = match &out.ended {
        Ended::Exited(status) => (status.code(), status.success(), false),
        Ended::TimedOut { .. } => (None, false, true),
    };
    CodeRunResult {
        exit_code,
        ok: exited_clean && read_whole(out),
        stdout: text(&out.stdout),
        stderr: with_run_notes(text(&out.stderr), out, secs, hint),
        duration_ms,
        timed_out,
        interpreter: interpreter.to_string(),
    }
}

/// Whether both streams were read to their end: a run whose output isn't whole is not `ok`,
/// whatever its exit status (its result can't be read as complete).
fn read_whole(out: &continuum_process::OwnedOutput) -> bool {
    use continuum_process::CaptureEnd;
    out.stdout.end == CaptureEnd::Complete && out.stderr.end == CaptureEnd::Complete
}

/// Compile the given complete Rust program with `rustc`, then run the produced
/// binary — each step under `timeout`, owned (`continuum_process::run_owned`). Returns the ground
/// truth of what happened: a COMPILE error is reported as the run result (ok=false,
/// rustc's stderr), not hidden — the persona reads the compiler's errors and
/// self-corrects exactly as she would a runtime panic. `Err` is reserved for a
/// failure to spawn the toolchain at all (e.g. `rustc` absent).
/// Run a complete Python program under the same wall-clock + ownership
/// contract as the Rust path. Same ground-truth shape: a traceback is the run
/// result (ok=false), never hidden; `Err` is reserved for a missing
/// interpreter. The caller supplies the managed environment or resolved system Python.
/// The marker that makes a python snippet tree exploration rather than a program, if
/// any: `subprocess` / `os.system` / `os.popen` (a shell called from python), `os.walk`
/// / `os.listdir` / `os.scandir` / `glob.glob` / `os.path.exists` / pathlib's
/// `rglob` / `iterdir` (walking the tree). Reading a file with `open()` is a program
/// (a repro reads its fixture). Informational: the result names the faster hand.
pub(crate) fn script_shape(code: &str) -> Option<&'static str> {
    const MARKERS: [&str; 10] = [
        "subprocess",
        "os.system(",
        "os.popen(",
        "os.walk(",
        "os.listdir(",
        "os.scandir(",
        "glob.glob(",
        "os.path.exists(",
        ".rglob(",
        ".iterdir(",
    ];
    MARKERS.iter().copied().find(|m| code.contains(m))
}

async fn run_python(
    dir: &std::path::Path,
    cwd: Option<&std::path::Path>,
    code: &str,
    timeout: std::time::Duration,
    interpreter: &std::path::Path,
    hands: &Hands,
) -> Result<CodeRunResult, CommandError> {
    let src = dir.join("main.py");
    std::fs::write(&src, code)
        .map_err(|e| CommandError::Internal(format!("code/run: write failed: {e}")))?;
    let interpreter_label = interpreter.display().to_string();
    let mut cmd = hands.command(interpreter, &[src.clone().into()], Some(cwd.unwrap_or(dir)));
    crate::code::shell_session::strip_secret_env(&mut cmd);
    crate::code::shell_session::utf8_text_env(&mut cmd);
    let started = std::time::Instant::now();
    let out = continuum_process::run_owned(cmd, Some(timeout), "code-run", std::sync::Arc::default())
        .await
        .map_err(|e| CommandError::Internal(format!("code/run: {interpreter_label} spawn failed: {e}")))?;
    let hint = format!(" (raise timeout_secs up to {MAX_TIMEOUT_SECS}s, or use code/shell for long-running work)");
    Ok(result_of(&out, &interpreter_label, started.elapsed().as_millis() as u64, timeout.as_secs(), &hint))
}

/// Whose hands a snippet runs with: the caller's confinement (a citizen's) or none (the
/// operator's), plus the name the spawn probe carries. One value threaded through both
/// interpreters so a snippet is confined exactly as her shell is (card d598c806).
pub(crate) struct Hands {
    confinement: Option<crate::system_resources::confinement::Confinement>,
    who: String,
}

impl Hands {
    pub(crate) fn of(state: &crate::modules::code::CodeState, ctx: &Ctx, workspace: &std::path::Path) -> Result<Self, CommandError> {
        Ok(Self {
            confinement: crate::modules::code_commands::confinement_of(state, ctx, workspace)?,
            who: crate::modules::code_commands::caller_id(state, ctx),
        })
    }

    #[cfg(test)]
    fn operator() -> Self {
        Self { confinement: None, who: "test".into() }
    }

    fn command(&self, program: &std::path::Path, args: &[std::ffi::OsString], cwd: Option<&std::path::Path>) -> tokio::process::Command {
        crate::system_resources::confinement::command_for(self.confinement.as_ref(), &self.who, program, args, cwd, None)
    }
}

async fn compile_and_run_rust(
    dir: &std::path::Path,
    code: &str,
    timeout: std::time::Duration,
    hands: &Hands,
) -> Result<CodeRunResult, CommandError> {
    let src = dir.join("snippet.rs");
    let bin = dir.join("snippet");
    std::fs::write(&src, code)
        .map_err(|e| CommandError::Internal(format!("code/run: temp write failed: {e}")))?;

    let started = std::time::Instant::now();

    // 1. Compile, owned (continuum_process::run_owned) so a runaway rustc is ended; a non-success
    //    exit is a RESULT (the persona must SEE the compiler errors), only a spawn failure is
    //    an error.
    let mut rustc = hands.command(
        std::path::Path::new("rustc"),
        &["--edition".into(), "2021".into(), "-o".into(), bin.clone().into(), src.clone().into()],
        None,
    );
    crate::code::shell_session::strip_secret_env(&mut rustc);
    let compiled = continuum_process::run_owned(rustc, Some(timeout), "rustc", std::sync::Arc::default())
        .await
        .map_err(|e| CommandError::Internal(format!("code/run: failed to run rustc (is it on PATH?): {e}")))?;
    match &compiled.ended {
        // compiled, and rustc's output read whole — fall through to run. A successful exit whose
        // output a process it started still held (a linker) doesn't vouch for the binary yet.
        continuum_process::Ended::Exited(status) if status.success() && read_whole(&compiled) => {}
        // a failed compile, a timed-out one, or a successful exit whose output isn't whole: the
        // result is rustc's, with everything it wrote
        _ => return Ok(result_of(&compiled, "rustc", started.elapsed().as_millis() as u64, timeout.as_secs(), " (rustc)")),
    }

    // 2. Run the compiled binary, owned (continuum_process::run_owned): at the safety timeout,
    //    or when this command is dropped, it and everything it started are ended, never
    //    orphaned to init burning a core (observed: 6h+ runaway at 100% CPU).
    let mut child = hands.command(&bin, &[], None);
    crate::code::shell_session::strip_secret_env(&mut child);
    let out = continuum_process::run_owned(child, Some(timeout), "code-run", std::sync::Arc::default())
        .await
        .map_err(|e| CommandError::Internal(format!("code/run: failed to run the compiled binary: {e}")))?;
    // ran to completion (clean, or a nonzero exit / panic on stderr), or the safety timeout fired:
    // a RESULT either way, with everything it printed
    Ok(result_of(&out, "rustc", started.elapsed().as_millis() as u64, timeout.as_secs(), ""))
}
/// `code/run` reports its own truth in `CodeRunResult.ok` — "whether exit_code ==
/// Some(0)". Projecting it means a compile failure reads as a FAILURE in the
/// receipt while `stderr` still reaches the model verbatim: the traceback she needs
/// to self-correct is untouched, only the outcome marker changes.
impl crate::sdk_codegen::ProjectsOutcome for CodeRun {
    fn outcome(output: &CodeRunResult) -> crate::sdk_codegen::ToolVerdict {
        if output.ok {
            crate::sdk_codegen::ToolVerdict::Succeeded
        } else {
            crate::sdk_codegen::ToolVerdict::Failed
        }
    }
}

crate::register_command!(CodeRun);
crate::register_outcome!(CodeRun);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the happy path — a complete Rust program that compiles and
    // runs clean reports ok=true, exit 0, and the actual stdout. This is the hand
    // working: she can see what her code printed instead of guessing.
    #[tokio::test]
    async fn compiles_and_runs_rust_capturing_stdout() {
        let out = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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
        let out = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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
        let out = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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
        let out = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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

    // what this catches (Codex root on #5030 at 6c7bd0d82 and 96d6d27db): code/run told her a
    // timed-out run's output was "everything it produced" when its read had failed; a run whose
    // output a process it started held past the bound read as whole and ok; and the rustc
    // timeout arm returned an empty stdout it had captured. Every arm projects through one
    // result_of: how each stream was read is named, what was read is kept, and an output that
    // isn't whole is never ok.
    #[cfg(unix)]
    #[test]
    fn a_runs_capture_status_and_what_it_read_survive_every_path_and_an_unwhole_run_is_never_ok() {
        use continuum_process::{CaptureEnd, Captured, Ended, OwnedOutput};
        use std::os::unix::process::ExitStatusExt;
        let read = |end: CaptureEnd| Captured { bytes: b"partial".to_vec(), end };
        let timed = OwnedOutput {
            stdout: read(CaptureEnd::Cut("the bound elapsed".into())),
            stderr: read(CaptureEnd::Failed("the pipe broke".into())),
            input: None,
            ended: Ended::TimedOut { unconfirmed: vec![] },
        };
        let note = with_run_notes(String::new(), &timed, 5, "");
        assert!(note.contains("what was read before the kill") && !note.contains("everything it produced"), "{note}");
        assert!(note.contains("stderr could not be read whole: the pipe broke"), "a failed read is named on the timeout path too: {note}");
        assert!(!note.contains("stdout was cut"), "the kill's own cut is said once: {note}");
        // every arm projects through result_of: a timed-out run keeps the stdout it captured
        let projected = result_of(&timed, "rustc", 1, 5, " (rustc)");
        assert_eq!((projected.stdout.as_str(), projected.timed_out, projected.ok, projected.exit_code), ("partial", true, false, None), "the captured stdout survives the timeout arm");
        let held = OwnedOutput {
            stdout: read(CaptureEnd::Cut("a process it started held its output past the bound".into())),
            stderr: read(CaptureEnd::Complete),
            input: None,
            ended: Ended::Exited(std::process::ExitStatus::from_raw(0)),
        };
        assert!(with_run_notes(String::new(), &held, 5, "").contains("stdout was cut: a process it started held"), "an exit's cut is named");
        assert!(!read_whole(&held), "and a run whose output isn't whole is never ok");
    }

    // what this catches: the safety timeout ACTUALLY kills the child process — not
    // just reports timed_out. Regression for the orphan leak: tokio::time::timeout
    // only drops the output() future; without an owner ending it the child survives, orphans
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
        let out = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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
        let err = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
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
    // lang string. The receipt must name the actual executable, including on Windows
    // where python3 may be an unusable App Execution Alias while python works.
    #[tokio::test]
    async fn python_runs_and_returns_ground_truth() {
        let r = CodeRun { state: crate::test_acting::state_with_default_policy(tokio::runtime::Handle::current()) }
            .run(
                &Ctx::default(),
                CodeRunParams {
                    lang: "python".into(),
                    code: "import json, sys\nprint(2+2)\nprint(json.dumps(sys.executable))".into(),
                    timeout_secs: None,
                },
            )
            .await
            .expect("python must run");
        assert!(r.ok, "clean exit: {r:?}");
        let mut lines = r.stdout.lines();
        assert_eq!(lines.next(), Some("4"));
        let actual: std::path::PathBuf =
            serde_json::from_str(lines.next().expect("Python executable receipt")).unwrap();
        assert!(actual.is_absolute());
        assert_eq!(std::path::PathBuf::from(&r.interpreter), actual);
    }
    // what this catches (Kimi, 2026-09-26 20:06Z, op #6929): a printed glyph outside
    // Latin-1 is OUTPUT, never an exit 1. On Windows a piped child Python encodes stdout
    // with the console code page (cp1252) and dies on the first `⚙`; the runner sets
    // PYTHONUTF8=1 so the same program prints the same bytes on every host.
    #[tokio::test]
    async fn a_printed_glyph_outside_latin_1_is_output_never_an_exit_1() {
        let dir = std::env::temp_dir().join(format!("code-run-utf8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let interpreter = crate::modules::python_adapter::find_python_async()
            .await
            .expect("Python 3");
        let out = run_python(
            &dir,
            None,
            "print('\\u2699 code/read \\u2014 \\u2713')\n",
            std::time::Duration::from_secs(20),
            &interpreter,
            &Hands::operator(),
        )
        .await
        .expect("resolved Python");
        assert!(out.ok, "exit {:?}, stderr: {}", out.exit_code, out.stderr);
        assert!(out.stdout.contains("⚙ code/read — ✓"), "stdout: {:?}", out.stdout);
        assert!(!out.stdout.contains('\r'), "a row is a line on every host: {:?}", out.stdout);
    }

    // what this catches: a timed-out run losing everything it printed before the kill
    // (QA from Joaquin, 2026-09-13, card ba846f38 — bare "timedOut" was the longest dead
    // air of her session). The partial stdout must ride with the timeout verdict.
    #[tokio::test]
    async fn a_timed_out_run_returns_what_it_printed_before_the_kill() {
        let dir = std::env::temp_dir().join(format!("code-run-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let interpreter = crate::modules::python_adapter::find_python_async()
            .await
            .expect("Python 3");
        let out = run_python(
            &dir,
            None,
            "import sys, time\nprint('partial-evidence', flush=True)\nsys.stderr.write('warming\\n'); sys.stderr.flush()\ntime.sleep(30)\nprint('never')\n",
            std::time::Duration::from_secs(1),
            &interpreter,
            &Hands::operator(),
        )
        .await
        .expect("resolved Python");
        assert!(out.timed_out, "the run must report the kill");
        assert_eq!(
            out.interpreter,
            interpreter.display().to_string(),
            "the result names the interpreter that ran"
        );
        assert!(
            out.stdout.contains("partial-evidence"),
            "partial stdout survives: {:?}",
            out.stdout
        );
        assert!(!out.stdout.contains("never"));
        assert!(
            out.stderr.contains("warming") && out.stderr.contains("killed: exceeded the 1s"),
            "stderr keeps the partial text AND the verdict: {:?}",
            out.stderr
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches (card 533c2d78, finder Mara): the snippet runner ran the PATH
    // python3 whatever checkout the caller held, so a repro in a held SWE checkout died
    // on "no module named numpy" — the interpreter, not the bug. With an interpreter
    // resolved for the run, THAT program runs and the result names it.
    #[tokio::test]
    #[cfg(unix)]
    async fn a_snippet_runs_under_the_interpreter_it_was_given_and_names_it() {
        let dir = std::env::temp_dir().join(format!("code-run-interp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let fake = dir.join("env-python");
        std::fs::write(&fake, "#!/bin/sh\necho from-the-prepared-env\n").unwrap(); // JUSTIFIED unwrap: test scaffolding
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&fake, permissions).unwrap(); // JUSTIFIED unwrap: test scaffolding
        }
        let out = run_python(
            &dir,
            None,
            "print('never')",
            std::time::Duration::from_secs(5),
            &fake,
            &Hands::operator(),
        )
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
        assert_eq!(
            script_shape("import subprocess\nsubprocess.run(['grep','-r','x','.'])"),
            Some("subprocess")
        );
        assert_eq!(
            script_shape("for r,d,f in os.walk('.'): print(f)"),
            Some("os.walk(")
        );
        assert_eq!(
            script_shape("src = open('a.py').read()\nassert 'def f' in src"),
            None,
            "reading a fixture is a program"
        );
        let root = std::env::temp_dir().join(format!("code-run-root-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap(); // JUSTIFIED unwrap: test scaffolding
        std::fs::write(root.join("marker.txt"), b"here").unwrap(); // JUSTIFIED unwrap: test scaffolding
        let dir = root.join("scratch");
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let interpreter = crate::modules::python_adapter::find_python_async()
            .await
            .expect("Python 3");
        let out = run_python(
            &dir,
            Some(&root),
            "print(open('marker.txt').read())",
            std::time::Duration::from_secs(10),
            &interpreter,
            &Hands::operator(),
        )
        .await
        .expect("resolved Python");
        assert_eq!(
            out.stdout.trim(),
            "here",
            "relative paths resolve at the given root"
        );
        // A broken prepared interpreter must fail explicitly, never run system Python instead.
        let missing = root.join("missing-managed-python");
        let error = run_python(
            &dir,
            Some(&root),
            "print('must not run')",
            std::time::Duration::from_secs(10),
            &missing,
            &Hands::operator(),
        )
        .await
        .expect_err("broken managed interpreter must not fall back");
        assert!(error.to_string().contains("missing-managed-python"));
        assert!(error.to_string().contains("spawn failed"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
