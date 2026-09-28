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
/// and the disk governor tracks and evicts (`system_resources::disk_reporters`). The SAME rule
/// `start.ps1` applies before it builds, so the governor and the builds agree:
/// 1. `CARGO_TARGET_DIR`, from the process env or `~/.continuum/config.env`;
/// 2. else `<CONTINUUM_STORAGE_PATH>/cargo-target`, the cold-storage installer's routing (the
///    5090's `D:\continuum-cold\cargo-target`), from the env or `config.env`;
/// 3. else `<home>/.continuum/cache/cargo-target`.
///
/// `config.env` matters because the core often runs as a service (a Windows scheduled task)
/// that inherits no user or shell environment (Fable on #4549). Deriving the home default while
/// the build writes elsewhere leaves the real tree unreported and unowned (the 2026-07-13
/// shape), and a second cache recompiles the whole graph (#4548). `None` only with no home and
/// nothing configured: never a guessed path such as `target/`.
pub fn shared_cargo_target_dir() -> Option<std::path::PathBuf> {
    resolve_cargo_target(configured("CARGO_TARGET_DIR"), configured("CONTINUUM_STORAGE_PATH"), home_dir())
}

/// [`shared_cargo_target_dir`] with `home` as the default's root (the disk registry's own home).
pub(crate) fn shared_cargo_target_dir_under(home: &std::path::Path) -> std::path::PathBuf {
    resolve_cargo_target(configured("CARGO_TARGET_DIR"), configured("CONTINUUM_STORAGE_PATH"), Some(home.to_path_buf()))
        .unwrap_or_else(|| home.join(".continuum").join("cache").join("cargo-target")) // unwrap_or_else: unreachable, a home is given
}

/// A setting from the process env, else `~/.continuum/config.env`; blank is unset.
fn configured(key: &str) -> Option<std::path::PathBuf> {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| crate::config_env::read(key).filter(|v| !v.trim().is_empty()))
        .map(std::path::PathBuf::from)
}

/// PURE: [`shared_cargo_target_dir`] from its inputs (an empty value is unset).
pub(crate) fn resolve_cargo_target(
    target: Option<std::path::PathBuf>,
    storage: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let set = |p: &std::path::PathBuf| !p.as_os_str().is_empty();
    target
        .filter(set)
        .or_else(|| storage.filter(set).map(|s| s.join("cargo-target")))
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
    // while the build writes elsewhere (the 5090: CONTINUUM_STORAGE_PATH routes it to D:), so
    // the disk governor tracked an empty tree and builds wrote a second cache; the rule
    // drifting from start.ps1's order; and an empty or missing value becoming a guessed path.
    #[test]
    fn the_cargo_target_follows_start_ps1_order() {
        use std::path::PathBuf;
        let home = PathBuf::from("/home/u");
        let default = home.join(".continuum").join("cache").join("cargo-target");
        let explicit = PathBuf::from("/fast/target");
        let storage = PathBuf::from("/cold");
        assert_eq!(resolve_cargo_target(Some(explicit.clone()), Some(storage.clone()), Some(home.clone())), Some(explicit), "CARGO_TARGET_DIR wins");
        assert_eq!(resolve_cargo_target(None, Some(storage.clone()), Some(home.clone())), Some(storage.join("cargo-target")), "cold-storage routing");
        assert_eq!(resolve_cargo_target(None, None, Some(home.clone())), Some(default.clone()));
        assert_eq!(resolve_cargo_target(Some(PathBuf::new()), Some(PathBuf::new()), Some(home)), Some(default), "blank is unset");
        assert_eq!(resolve_cargo_target(None, None, None), None, "nothing known: no guessed path");
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
