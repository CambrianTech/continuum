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

/// The continuum home dir (`$CONTINUUM_HOME` else `~/.continuum`) — the same
/// resolution the dispatch workspace + progress ledger use. `pub(crate)` so the
/// curriculum drain (`genome/teach --from-experience`) resolves the SAME citizen
/// layout as the grader that wrote the stream — one layout, never two.
pub fn continuum_home() -> Result<std::path::PathBuf, String> {
    std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(test_home)
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| "no home dir".to_string())
}

/// A TEST BINARY NEVER WRITES THE OPERATOR'S RECORDS (2026-09-20, the M5): a local
/// `cargo test` run of the serving daemon settled its harness lane and `save`d the
/// fixture's "coder-14b" into ~/.continuum/state/served-window.json; the next boot
/// remembered that model, found no decode knee for it, planned 8 lanes unclamped on a box
/// whose knee is 2, and swapped 4.6 GB. Every store resolves its path through this
/// function, so under `cfg(test)` — when no `$CONTINUUM_HOME` is declared — the home is a
/// per-process directory under the system temp dir. Not configurable, not a fixture to
/// remember: a test cannot reach the operator's state by forgetting.
#[cfg(test)]
fn test_home() -> Option<std::path::PathBuf> {
    static HOME: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
        let dir = std::env::temp_dir().join(format!("continuum-test-home-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    });
    Some(HOME.clone())
}
#[cfg(not(test))]
fn test_home() -> Option<std::path::PathBuf> {
    None
}

/// Durable placement of installed binaries, toolchains and runtime libraries.
/// The public installer owns this record; model/cache placement never changes
/// it implicitly. Missing record means the pre-existing home layout. A broken
/// record or unavailable selected drive is an error, never a second install.
pub fn payload_root(home: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let record = home.join("payload-root");
    let text = match std::fs::read_to_string(&record) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(home.to_path_buf()),
        Err(error) => return Err(format!("{}: {error}", record.display())),
    };
    let value = text.trim_end_matches(['\r', '\n']);
    let root = std::path::PathBuf::from(value);
    if value.is_empty() || value.contains(['\r', '\n']) || !root.is_absolute() {
        return Err(format!(
            "{} must name one absolute payload directory",
            record.display()
        ));
    }
    if !root.is_dir() {
        return Err(format!(
            "selected payload directory {} is unavailable",
            root.display()
        ));
    }
    Ok(root)
}

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

    // what this catches (2026-09-20): a test binary resolving the OPERATOR's home — a
    // harness lane's settle wrote a fixture model into the M5's served-window record and
    // the next boot planned 8 lanes on it. Under test the home is a per-process temp dir.
    #[test]
    fn a_test_binary_never_resolves_the_operators_home() {
        if std::env::var_os("CONTINUUM_HOME").is_some() {
            return; // an explicitly declared home is the test's own choice
        }
        let home = super::continuum_home().expect("a test home always resolves");
        assert!(home.starts_with(std::env::temp_dir()), "{}", home.display());
        assert!(home.to_string_lossy().contains("continuum-test-home-"), "{}", home.display());
        assert_ne!(Some(home.clone()), dirs::home_dir().map(|h| h.join(".continuum")));
        assert_eq!(super::continuum_home().unwrap(), home, "one home per process");
    }


    // What this catches: model-cache routing must not abandon existing engine
    // slots; a missing selected drive must not silently create a second install.
    #[test]
    fn payload_record_preserves_legacy_layout_and_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        let cold = tempfile::tempdir().unwrap();
        let record = home.path().join("payload-root");
        assert_eq!(payload_root(home.path()).unwrap(), home.path());
        std::fs::write(
            home.path().join("config.env"),
            "CONTINUUM_STORAGE_PATH='/another/model/cache'\n",
        )
        .unwrap();
        assert_eq!(payload_root(home.path()).unwrap(), home.path());
        std::fs::write(&record, format!("{}\n", cold.path().display())).unwrap();
        assert_eq!(payload_root(home.path()).unwrap(), cold.path());
        for invalid in [
            String::new(),
            "relative/path\n".into(),
            "one\ntwo\n".into(),
            cold.path().join("missing").display().to_string(),
        ] {
            std::fs::write(&record, invalid).unwrap();
            assert!(payload_root(home.path()).is_err());
        }
    }

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
