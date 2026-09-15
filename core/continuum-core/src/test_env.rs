//! ONE lock over the process-global home for every test that owns it.
//!
//! `HOME`, `USERPROFILE`, `HF_HOME` and `CONTINUUM_HOME` are process globals that
//! production reads at many seams (`dirs::home_dir`, `citizen_layer_path`,
//! `huggingface_cache_root`, `config_env`). A test that points them at a tempdir
//! must hold them EXCLUSIVELY for as long as the tempdir lives — and every such test
//! must hold the SAME lock, or a clone under one test's home is torn down by the
//! other test dropping its tempdir mid-write (the runner's "could not open
//! …/objects/pack/tmp_pack_… No such file", #4062 / #4078 / #4082: three private
//! `ENV_LOCK`s and one unlocked `CONTINUUM_HOME` write in the same crate).
//!
//! This module is the one lock and the one guard. Async tests take
//! [`HomeGuard::set`]; sync tests take [`with_test_home`]. Both pin all four
//! variables to the same home (pin, never remove — removing a var does not isolate a
//! test, it moves the read one layer down onto the real machine) and restore them on
//! drop, panic included.
use std::path::Path;
use std::sync::{Arc, LazyLock};

static HOME_ENV: LazyLock<Arc<tokio::sync::Mutex<()>>> =
    LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));

const VARS: [&str; 4] = ["HOME", "USERPROFILE", "HF_HOME", "CONTINUUM_HOME"];

/// Exclusive ownership of the process home for a test's lifetime.
pub(crate) struct HomeGuard {
    prior: Vec<(&'static str, Option<String>)>,
    _lock: tokio::sync::OwnedMutexGuard<()>,
}

impl HomeGuard {
    /// Take the lock from an async test (held across awaits).
    pub(crate) async fn set(home: &Path) -> Self {
        let lock = HOME_ENV.clone().lock_owned().await;
        Self::pin(home, lock)
    }

    /// Take the lock from a sync test. Panics if called inside an async runtime —
    /// use [`HomeGuard::set`] there.
    pub(crate) fn set_blocking(home: &Path) -> Self {
        let lock = HOME_ENV.clone().blocking_lock_owned();
        Self::pin(home, lock)
    }

    fn pin(home: &Path, lock: tokio::sync::OwnedMutexGuard<()>) -> Self {
        let prior = VARS.iter().map(|v| (*v, std::env::var(v).ok())).collect();
        std::env::set_var("HOME", home);
        std::env::set_var("USERPROFILE", home);
        std::env::set_var("HF_HOME", home.join(".cache").join("huggingface"));
        std::env::set_var("CONTINUUM_HOME", home.join(".continuum"));
        Self { prior, _lock: lock }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        for (var, value) in self.prior.drain(..) {
            match value {
                Some(v) => std::env::set_var(var, v),
                None => std::env::remove_var(var),
            }
        }
    }
}

/// Run `f` with the process home pinned to `home`, under the one lock (sync tests).
pub(crate) fn with_test_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = HomeGuard::set_blocking(home);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the guard pins every home-shaped variable to the test's home
    // and restores all of them on drop — including CONTINUUM_HOME, the one the citizen
    // layer reads first and the one no prior guard pinned.
    #[test]
    fn the_guard_pins_every_home_variable_and_restores_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = HomeGuard::set_blocking(dir.path());
        let prior = first.prior.clone();
        assert_eq!(std::env::var("HOME").ok().as_deref(), dir.path().to_str());
        assert_eq!(
            std::env::var("CONTINUUM_HOME").ok().as_deref(),
            dir.path().join(".continuum").to_str()
        );
        drop(first);
        // Read the restored values UNDER THE LOCK (a second guard captures them as its
        // prior): every guard restores on drop, so whatever ran in between left them as
        // they were.
        let second = HomeGuard::set_blocking(dir.path());
        assert_eq!(second.prior, prior, "every variable restored");
    }
}
