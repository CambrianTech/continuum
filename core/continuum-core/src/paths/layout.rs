//! The continuum home's layout: each well-known directory is named once, here.
//!
//! [`super::continuum_home`] owns WHERE the home is; this owns WHAT is under it. Before it,
//! every caller spelled its own subdirectory (`"genome/venv/bin/python3"`,
//! `".continuum/datasets"`, `"genome/models"`), so one directory had as many definitions as
//! callers, and a caller that bypassed the home resolver was invisible to it.

use std::path::PathBuf;

/// A continuum root (the home, or a configured storage tier laid out the same way) and the
/// directories under it. Callers ask for a directory by name instead of joining strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuumLayout {
    root: PathBuf,
}

impl ContinuumLayout {
    /// The layout of this process's continuum home ([`super::continuum_home`]: honours
    /// `$CONTINUUM_HOME`, and a test binary's per-process home).
    pub fn home() -> Result<Self, String> {
        super::continuum_home().map(Self::at)
    }

    /// The same layout under another root: a configured storage tier, or a test's temp dir.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Generated training datasets: `dataset/*`, `genome/teach`, forge's mlx split.
    pub fn datasets(&self) -> PathBuf {
        self.root.join("datasets")
    }

    /// Gene and model state: weights, measured engine footprints, the managed venv.
    pub fn genome(&self) -> PathBuf {
        self.root.join("genome")
    }

    /// Locally cached model weights.
    pub fn genome_models(&self) -> PathBuf {
        self.genome().join("models")
    }

    /// The managed Python interpreter that MLX training and LoRA conversion run under.
    pub fn genome_venv_python(&self) -> PathBuf {
        self.genome().join("venv").join("bin").join("python3")
    }

    /// Run and progress ledgers.
    pub fn progress(&self) -> PathBuf {
        self.root.join("progress")
    }

    /// Installed toolchains (the CUDA training environment, ...).
    pub fn tools(&self) -> PathBuf {
        self.root.join("tools")
    }
}
