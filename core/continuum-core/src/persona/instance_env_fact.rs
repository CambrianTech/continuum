//! The environment a held checkout runs in, as a working-memory fact.
//!
//! The grading path prepares one environment per SWE instance
//! (`swe_bench::ensure_env` → `<swe cache>/envs/<instance>`), with the repo's
//! era-pinned dependencies installed. A holder's work turn never named it, so
//! she reached for whatever interpreter she could guess and re-installed the
//! repo into it — measured 2026-09-07: twelve `pip install -e .` runs in one
//! checkout, 21 acts, 0 edits. One fact per turn closes that: the interpreter
//! when the env exists, and the honest absence when it does not.

use std::path::{Path, PathBuf};

/// The prepared environment's interpreter for the instance a checkout belongs
/// to, if the grader has built one. The checkout's directory name IS the
/// instance id (`…/swe/<instance>`), the same key `ensure_env` uses.
pub fn instance_python(checkout: &Path) -> Option<PathBuf> {
    let instance = checkout.file_name()?.to_str()?;
    let env_dir = crate::cognition::swe_bench::swe_cache_dir()
        .join("envs")
        .join(instance);
    env_python_in(&env_dir)
}

fn env_python_in(env_dir: &Path) -> Option<PathBuf> {
    let unix = env_dir.join("bin").join("python");
    if unix.is_file() {
        return Some(unix);
    }
    let windows = env_dir.join("Scripts").join("python.exe");
    windows.is_file().then_some(windows)
}

/// The fact line for her working memory. Never a guess: an env that exists is
/// named by path; one that does not is said to be missing, with the one
/// command that prepares it, so the turn does not open with an install.
pub fn instance_env_fact(checkout: &Path) -> String {
    match instance_python(checkout) {
        Some(py) => format!(
            "[env] This checkout's dependencies are installed in a prepared environment: \
             run Python and tests with `{}` (e.g. `{} -m pytest <path>`). Do not pip-install \
             the repo or its dependencies — they are already there.",
            py.display(),
            py.display()
        ),
        None => "[env] No prepared environment exists for this checkout yet — the grader \
                 builds one at verdict time. Do not spend acts installing the repo; read and \
                 edit the code, and run only what needs no installed dependencies."
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: the fact guessing an interpreter. A present env names
    /// its python by path; an absent one is said to be absent and forbids the
    /// install spiral — never a stand-in path.
    #[test]
    fn the_env_fact_names_the_interpreter_or_the_absence() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let env = tmp.path().join("envs").join("django__django-1");
        std::fs::create_dir_all(env.join("bin")).expect("mkdir");
        std::fs::write(env.join("bin").join("python"), b"#!/bin/sh\n").expect("py");
        let py = env_python_in(&env).expect("present env names its python");
        assert!(py.ends_with("bin/python"));
        assert!(env_python_in(&tmp.path().join("envs").join("absent")).is_none());
        let absent = instance_env_fact(&tmp.path().join("swe").join("absent"));
        assert!(absent.contains("No prepared environment"));
        assert!(!absent.contains("swe-venv"), "never a guessed interpreter");
    }
}
