//! Whether a Python-process trainer (MLX, CUDA) can actually run here (card 6f05b89d).
//!
//! An interpreter file that exists proves nothing: a venv missing `mlx_lm`, or the CUDA
//! trainer's packages, would still be selected, and advertised to the grid as trainable, for
//! jobs it cannot run. So the trainer owner probes its own imports once, in the background,
//! when it is built: one `python -c "import ..."` per process, never per beacon or selection.
//! Until the probe answers, the trainer is not ready (no guessed timeout decides it). A hung
//! import keeps it unadvertised, and that is the truth. Dependencies installed later take effect
//! at the next core start.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// The cached answer to "can this trainer's interpreter import what it needs".
pub(crate) struct PythonTrainerReadiness {
    ready: Arc<OnceLock<bool>>,
    #[cfg(test)]
    probe: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl PythonTrainerReadiness {
    /// Probe `python` for `imports` (a Python statement, e.g. `import mlx_lm`) on a background
    /// thread. With no interpreter file there is nothing to run, so it is never ready.
    pub(crate) fn probe(python: Option<PathBuf>, imports: &'static str) -> Self {
        let ready = Arc::new(OnceLock::new());
        let probe = match python.filter(|path| path.is_file()) {
            None => {
                let _ = ready.set(false);
                None
            }
            Some(python) => {
                let answer = Arc::clone(&ready);
                std::thread::Builder::new()
                    .name("trainer-readiness".into())
                    .spawn(move || {
                        let imported = std::process::Command::new(&python)
                            .args(["-c", imports])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .is_ok_and(|status| status.success());
                        continuum_base::probe!(
                            class = "genome.trainer.readiness",
                            python = %python.display(),
                            imports,
                            ready = imported,
                            "a Python trainer probed its own imports; only a ready one is selected or advertised"
                        );
                        let _ = answer.set(imported);
                    })
                    .ok()
            }
        };
        #[cfg(not(test))]
        drop(probe);
        Self {
            ready,
            #[cfg(test)]
            probe: std::sync::Mutex::new(probe),
        }
    }

    /// Ready only once the probe has answered yes.
    pub(crate) fn is_ready(&self) -> bool {
        self.ready.get().copied().unwrap_or(false) // not yet answered = not ready
    }

    /// Whether this trainer can take `base` here: ready, and the base's registry row names the
    /// trainable HF source it would load. The MLX and CUDA trainers answer `serves_base` with
    /// this, so neither is selected, nor advertised to the grid, for a job it would refuse.
    pub(crate) fn serves(&self, base: &str) -> bool {
        self.is_ready() && crate::model_registry::artifacts::resolve_hf_source_for_model_id(base).is_ok()
    }

    /// Wait for the probe to answer.
    #[cfg(test)]
    fn settle(&self) {
        let handle = self.probe.lock().expect("readiness probe handle").take(); // JUSTIFIED: test-only, a poisoned lock is a failed test
        if let Some(handle) = handle {
            handle.join().expect("the probe thread does not panic"); // JUSTIFIED: test-only
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 6f05b89d, Codex's review of #5356): an interpreter that exists
    // but lacks the trainer's packages was treated as a usable trainer, selected and advertised
    // for jobs it cannot run. Present-but-failing-imports is not ready; a missing interpreter
    // is never ready; only a probe that succeeds is.
    #[cfg(unix)]
    #[test]
    fn an_interpreter_missing_its_dependencies_is_not_a_ready_trainer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("scratch dir");
        let fake = |name: &str, exit: u8| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\nexit {exit}\n")).expect("write fake python");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod fake python");
            path
        };

        let missing_deps = PythonTrainerReadiness::probe(Some(fake("no-deps", 1)), "import mlx_lm");
        missing_deps.settle();
        assert!(!missing_deps.is_ready(), "the interpreter exists, its imports fail");

        let none = PythonTrainerReadiness::probe(Some(dir.path().join("absent")), "import mlx_lm");
        assert!(!none.is_ready(), "no interpreter, never ready");
        assert!(!PythonTrainerReadiness::probe(None, "import mlx_lm").is_ready());

        let ready = PythonTrainerReadiness::probe(Some(fake("deps", 0)), "import mlx_lm");
        ready.settle();
        assert!(ready.is_ready());
        assert!(!ready.serves("no-such-registry-row"), "ready, but this base has no trainable source");
    }
}
