//! Whether a Python-process trainer (MLX, CUDA) can actually run here (card 6f05b89d).
//!
//! An interpreter file that exists proves nothing: a venv missing `mlx_lm`, or the CUDA
//! trainer's packages, would still be selected, and advertised to the grid as trainable, for
//! jobs it cannot run. So the trainer owner probes its own imports once, when it is built:
//! one `python -c "import ..."` per process, never per beacon or selection. The probe runs
//! under the substrate's owned-process runner ([`run_owned`]): its whole process tree is
//! ended and reaped if the probe is dropped or outlives its bound, and its output is capped.
//! Until the probe answers, the trainer is not ready. A probe that cannot answer (it timed
//! out, could not start, or its end is unconfirmed) leaves readiness UNAVAILABLE, never
//! "missing dependencies". Dependencies installed later take effect at the next core start.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use continuum_process::{run_owned, Ended};

/// How long the import probe may run before it is ended and readiness is unavailable.
/// Measured: the CUDA trainer's imports (torch, transformers, peft, accelerate, bitsandbytes,
/// safetensors) took 28.3 s in a fresh process on the 5090 (Windows, its cuda-training venv,
/// under serving load, OS cache state unproven), 2026-10-24T08:25Z, Codex. The bound is about
/// 4x that observed sample, not a worst case. Exceeding it reads as unavailable, never as
/// missing dependencies. MLX's `import mlx_lm` on the M5 is still to be measured.
const IMPORT_PROBE_BOUND: Option<Duration> = Some(Duration::from_secs(120));

/// Python run before an import probe's statement: the interpreter's stdout and stderr go to
/// the null device at the file-descriptor level, so nothing the imported packages print (native
/// libraries included) is captured. The probe's output is bounded at zero bytes; its answer is
/// its exit status.
pub(crate) const SILENCE_ALL: &str =
    "import os; _null = os.open(os.devnull, os.O_WRONLY); os.dup2(_null, 1); os.dup2(_null, 2); ";

/// What one question to a trainer's interpreter came back with.
pub(crate) enum Answer {
    /// It exited and was reaped; `cut` names a stream that could not be read whole.
    Exited { output: std::process::Output, cut: Option<String> },
    /// No answer: it could not start, outlived its bound, or its end is unconfirmed.
    Unavailable(String),
}

/// Ask a trainer's interpreter one question: run `python` with `args` under the owned runner,
/// within `bound`. Dropping the returned future ends the process and everything it started.
/// The one place the genome's trainers spawn their interpreter to ask (the readiness import
/// probe, MLX's chat-template probe); the training runs themselves are their own.
pub(crate) async fn ask_python<S: AsRef<OsStr>>(python: &Path, args: &[S], bound: Option<Duration>) -> Answer {
    let mut command = tokio::process::Command::new(python);
    command.args(args).stdin(std::process::Stdio::null());
    match run_owned(command, bound, "trainer-python", Arc::default()).await {
        Err(error) => Answer::Unavailable(format!("could not start {}: {error}", python.display())),
        Ok(out) => {
            let cut = out.capture_note();
            match out.ended {
                Ended::Exited(status) => Answer::Exited {
                    output: std::process::Output { status, stdout: out.stdout.bytes, stderr: out.stderr.bytes },
                    cut,
                },
                Ended::TimedOut { unconfirmed } if unconfirmed.is_empty() => {
                    Answer::Unavailable("outlived its bound and was ended".into())
                }
                Ended::TimedOut { unconfirmed } => {
                    Answer::Unavailable(format!("outlived its bound; not confirmed gone: {unconfirmed:?}"))
                }
            }
        }
    }
}

/// The probe's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// The interpreter imported everything the trainer needs.
    Ready,
    /// No interpreter, or it ran and its imports failed.
    Missing,
    /// The probe could not answer; this says nothing about the dependencies.
    Unavailable(String),
}

/// The trainer owner's cached readiness, and custody of the probe that answers it.
pub(crate) struct PythonTrainerReadiness {
    verdict: Arc<OnceLock<Readiness>>,
    probe: Option<tokio::task::JoinHandle<()>>,
}

impl PythonTrainerReadiness {
    /// Probe `python` for `imports` (a Python statement, e.g. `import mlx_lm`).
    pub(crate) fn probe(python: Option<PathBuf>, imports: &'static str) -> Self {
        Self::probe_within(python, imports, IMPORT_PROBE_BOUND)
    }

    fn probe_within(python: Option<PathBuf>, imports: &'static str, bound: Option<Duration>) -> Self {
        let verdict = Arc::new(OnceLock::new());
        let Some(python) = python.filter(|path| path.is_file()) else {
            let _ = verdict.set(Readiness::Missing);
            return Self { verdict, probe: None };
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            let _ = verdict.set(Readiness::Unavailable("built outside an async runtime: nothing to probe on".into()));
            return Self { verdict, probe: None };
        };
        let answer = Arc::clone(&verdict);
        let probe = runtime.spawn(async move {
            let statement = format!("{SILENCE_ALL}{imports}");
            let found = match ask_python(&python, &["-c", statement.as_str()], bound).await {
                Answer::Exited { output, .. } if output.status.success() => Readiness::Ready,
                Answer::Exited { .. } => Readiness::Missing,
                Answer::Unavailable(why) => Readiness::Unavailable(why),
            };
            continuum_base::probe!(
                class = "genome.trainer.readiness",
                python = %python.display(),
                imports,
                verdict = ?found,
                "a Python trainer probed its own imports; only a ready one is selected or advertised"
            );
            let _ = answer.set(found);
        });
        Self { verdict, probe: Some(probe) }
    }

    /// Ready only once the probe has answered yes.
    pub(crate) fn is_ready(&self) -> bool {
        self.verdict.get() == Some(&Readiness::Ready)
    }

    /// Whether this trainer can take `base` here: ready, and the base's registry row names the
    /// trainable HF source it would load. The MLX and CUDA trainers answer `serves_base` with
    /// this, so neither is selected, nor advertised to the grid, for a job it would refuse.
    pub(crate) fn serves(&self, base: &str) -> bool {
        self.is_ready() && crate::model_registry::artifacts::resolve_hf_source_for_model_id(base).is_ok()
    }

    /// Wait for the probe to answer.
    #[cfg(test)]
    async fn settle(&mut self) -> Option<Readiness> {
        if let Some(probe) = self.probe.take() {
            probe.await.expect("the probe task does not panic"); // JUSTIFIED: test-only
        }
        self.verdict.get().cloned()
    }
}

impl Drop for PythonTrainerReadiness {
    /// The probe ends with its trainer: aborting the task drops the owned run, which ends the
    /// interpreter and everything it started.
    fn drop(&mut self) {
        if let Some(probe) = self.probe.take() {
            probe.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_python(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake python"); // JUSTIFIED: test fixture
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod fake python"); // JUSTIFIED: test fixture
        path
    }

    #[cfg(unix)]
    fn alive(pid: u32) -> bool {
        !matches!(crate::inference::engine_residency::probe_process(pid), crate::inference::engine_residency::ProcessProbe::Absent)
    }

    // what this catches (card 6f05b89d, Codex's reviews of #5356): an interpreter that exists
    // but lacks the trainer's packages was treated as usable; a probe that hangs or floods its
    // output held a process and memory forever; a probe that could not answer was read as
    // "missing". Failing imports are Missing, a missing interpreter is Missing, a noisy success
    // is Ready, a probe past its bound is Unavailable (never Missing) and its process is gone.
    // (The output bound is the interpreter's own redirection: see the next test.)
    #[cfg(unix)]
    #[tokio::test]
    async fn the_import_probe_answers_ready_missing_or_unavailable_and_owns_its_process() {
        let dir = tempfile::tempdir().expect("scratch dir"); // JUSTIFIED: test fixture
        let probe = |name: &str, body: &str, bound| {
            PythonTrainerReadiness::probe_within(Some(fake_python(dir.path(), name, body)), "import mlx_lm", bound)
        };

        assert_eq!(probe("no-deps", "exit 1", None).settle().await, Some(Readiness::Missing));
        assert_eq!(probe("deps", "exit 0", None).settle().await, Some(Readiness::Ready));
        assert_eq!(
            PythonTrainerReadiness::probe_within(Some(dir.path().join("absent")), "import mlx_lm", None).verdict.get(),
            Some(&Readiness::Missing)
        );

        let pidfile = dir.path().join("hung.pid");
        let hung = format!("echo $$ > {}; exec sleep 600", pidfile.display());
        let verdict = probe("hung", &hung, Some(Duration::from_millis(500))).settle().await;
        assert!(matches!(verdict, Some(Readiness::Unavailable(_))), "a probe past its bound says nothing about deps: {verdict:?}");
        let pid: u32 = std::fs::read_to_string(&pidfile).expect("pid").trim().parse().expect("pid"); // JUSTIFIED: test fixture
        assert!(!alive(pid), "the hung probe was ended");
    }

    // what this catches (Codex's review of #5356): the production probe dropped its handle, so a
    // hung import outlived its trainer. Dropping the readiness ends the probe's process.
    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_the_trainer_ends_a_probe_still_running() {
        let dir = tempfile::tempdir().expect("scratch dir"); // JUSTIFIED: test fixture
        let pidfile = dir.path().join("running.pid");
        let body = format!("echo $$ > {}; exec sleep 600", pidfile.display());
        let readiness = PythonTrainerReadiness::probe_within(Some(fake_python(dir.path(), "running", &body)), "import mlx_lm", None);
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&pidfile).ok().and_then(|s| s.trim().parse::<u32>().ok()) {
                break pid;
            }
            tokio::task::yield_now().await;
        };
        assert!(alive(pid));
        assert!(!readiness.is_ready(), "still probing is not ready");
        drop(readiness);
        let gone = async { while alive(pid) { tokio::task::yield_now().await; } };
        tokio::time::timeout(Duration::from_secs(10), gone).await.expect("the probe's process ends with its trainer"); // JUSTIFIED: test assertion
    }

    // what this catches (Codex's review of #5356): a probe that floods its output held that
    // output in memory. With the prelude, a real interpreter printing 50 MB to both streams is
    // captured as zero bytes and still answers by its exit status.
    #[tokio::test]
    async fn a_noisy_probe_is_captured_as_nothing() {
        let python = Path::new("/usr/bin/python3");
        assert!(python.is_file(), "the CI images and macOS ship /usr/bin/python3");
        let flood = format!("{SILENCE_ALL}import sys; sys.stdout.write('x' * 50_000_000); sys.stderr.write('y' * 50_000_000)");
        match ask_python(python, &["-c", flood.as_str()], None).await {
            Answer::Exited { output, .. } => {
                assert!(output.status.success());
                assert_eq!((output.stdout.len(), output.stderr.len()), (0, 0), "nothing captured");
            }
            Answer::Unavailable(why) => panic!("the interpreter answered nothing: {why}"),
        }
    }
}
