//! Path policies — single source of truth for resolving filesystem
//! paths the system depends on.
//!
//! Mirrors the TypeScript `system/server/process/ProcessPathPolicy.ts`
//! pattern (codex's #1221) on the Rust side: any module that needs to
//! resolve a "where does X live on disk?" question imports the
//! relevant policy fn here, rather than hardcoding the path inline.
//!
//! Why a dedicated module:
//! - Per-OS path divergence (macOS / Linux / Windows / WSL2) lives in
//!   one place; consumers don't repeat the cfg(target_os) ladder.
//! - Tests can override the policy via env-var injection (a la
//!   ProcessPathPolicy) without touching the consumer code.
//! - The next time we add a tier (HF cache, NVMe pool, etc.) it
//!   slots in here as a sibling module instead of accumulating
//!   inline path logic across the codebase.
//!
//! Sub-modules:
//! - `docker` — Docker Desktop sparse-image + related paths
//! - (future) `hf_cache` — Hugging Face model cache root
//! - (future) `nvme_pool` — LoRA Genome Paging tier

pub mod docker;

/// Preserve a nonempty HOME override, then use the OS-native home discovery.
/// Native Windows embeddings need not inherit a Unix shell environment. Never
/// substitute the checkout/current directory for missing persistent storage.
pub(crate) fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(test)]
    if let Some(native) = TEST_NATIVE_HOME.with(|root| root.borrow().clone()) {
        return resolve_home(None, || Some(native));
    }
    resolve_home(std::env::var_os("HOME").map(Into::into), dirs::home_dir)
}

fn resolve_home(
    explicit: Option<std::path::PathBuf>,
    native: impl FnOnce() -> Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    explicit
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(native)
}

/// THE machine's shared cargo target dir: the one tree every substrate cargo build writes
/// and the disk governor tracks and evicts (`system_resources::disk_reporters`). A configured
/// `CARGO_TARGET_DIR` wins: an operator may keep it on another volume (the 5090's
/// `D:\continuum-cold\cargo-target`), and deriving the home default there instead leaves the
/// real tree unreported and unowned while a second cache recompiles the whole graph (#4548).
/// Otherwise `<home>/.continuum/cache/cargo-target`. `None` only with no home: never a guessed
/// path such as `target/`.
pub fn shared_cargo_target_dir() -> Option<std::path::PathBuf> {
    resolve_cargo_target(std::env::var_os("CARGO_TARGET_DIR").map(Into::into), home_dir())
}

/// PURE: [`shared_cargo_target_dir`] from its inputs (an empty configured value is unset).
pub(crate) fn resolve_cargo_target(
    configured: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    configured
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| home.map(|h| h.join(".continuum").join("cache").join("cargo-target")))
}

#[cfg(test)]
thread_local! {
    // The recorder's existing per-thread fixture-root seam, shared with the
    // other readers/writers using this policy. Models HOME absence and a native
    // home without mutating process-global environment or touching real data.
    static TEST_NATIVE_HOME: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct NativeHomeOverride(Option<std::path::PathBuf>);

#[cfg(test)]
impl NativeHomeOverride {
    pub(crate) fn install(path: &std::path::Path) -> Self {
        Self(TEST_NATIVE_HOME.with(|root| root.replace(Some(path.to_path_buf()))))
    }

    /// The override installed on THIS thread, for a test-scoped fixture to carry into
    /// a thread it spawns (the seam writes residents on their own threads, #4414):
    /// a thread-local that is not carried is a real home written from a test.
    pub(crate) fn current() -> Option<std::path::PathBuf> {
        TEST_NATIVE_HOME.with(|root| root.borrow().clone())
    }
}

#[cfg(test)]
impl Drop for NativeHomeOverride {
    fn drop(&mut self) {
        TEST_NATIVE_HOME.with(|root| *root.borrow_mut() = self.0.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 9d77bc84): the shared cargo cache derived as the home default
    // while the operator configured it elsewhere (the 5090 on D:), so the disk governor
    // tracked an empty tree and builds wrote a second cache; and an empty or missing value
    // turned into a guessed path.
    #[test]
    fn a_configured_cargo_target_wins_and_the_default_is_under_home() {
        let home = std::path::PathBuf::from("/home/u");
        let configured = std::path::PathBuf::from("/cold/cargo-target");
        assert_eq!(resolve_cargo_target(Some(configured.clone()), Some(home.clone())), Some(configured));
        assert_eq!(
            resolve_cargo_target(None, Some(home.clone())),
            Some(home.join(".continuum").join("cache").join("cargo-target"))
        );
        assert_eq!(
            resolve_cargo_target(Some(std::path::PathBuf::new()), Some(home.clone())),
            Some(home.join(".continuum").join("cache").join("cargo-target")),
            "an empty CARGO_TARGET_DIR is unset"
        );
        assert_eq!(resolve_cargo_target(None, None), None, "no home: no guessed path");
    }

    // What this catches (f098571b): native fallback must not replace an explicit
    // HOME override; unresolved storage must not become the current directory.
    #[test]
    fn explicit_home_wins_and_missing_native_home_stays_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_home(Some(dir.path().to_path_buf()), || panic!(
                "HOME takes precedence"
            )),
            Some(dir.path().to_path_buf()),
        );
        let native = dir.path().join("native-home");
        assert_eq!(
            resolve_home(Some(std::path::PathBuf::new()), || Some(native.clone())),
            Some(native),
            "an empty HOME is absent, never a current-directory storage root",
        );
        assert_eq!(resolve_home(Some(std::path::PathBuf::new()), || None), None);
        assert_eq!(resolve_home(None, || None), None);
    }
}
