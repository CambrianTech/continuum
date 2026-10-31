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

pub mod huggingface;
pub mod docker;
pub mod cargo;
pub mod artifacts;
pub mod native;
mod layout;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_home;
pub use layout::ContinuumLayout;

/// Configured storage tier, without inventing a machine-specific destination.
/// Writers still resolve its physical volume and perform capacity admission.
pub fn configured_storage_root() -> Option<std::path::PathBuf> {
    configured("CONTINUUM_STORAGE_PATH")
}

/// Component-prefix comparison without filesystem IO. Windows normal and
/// verbatim drive/UNC prefixes name the same spelling; other components remain
/// exact. This does not resolve junctions, symlinks or relative paths: callers
/// retain their existing physical-path custody contract.
pub fn has_path_prefix(path: &std::path::Path, base: &std::path::Path) -> bool {
    let mut components = path.components();
    base.components().all(|expected| components.next().is_some_and(|actual| {
        #[cfg(windows)]
        if let (std::path::Component::Prefix(a), std::path::Component::Prefix(b)) = (actual, expected) {
            use std::path::Prefix::{Disk, UNC, VerbatimDisk, VerbatimUNC};
            return match (a.kind(), b.kind()) {
                (Disk(a) | VerbatimDisk(a), Disk(b) | VerbatimDisk(b)) => a.eq_ignore_ascii_case(&b),
                (UNC(a, share_a) | VerbatimUNC(a, share_a), UNC(b, share_b) | VerbatimUNC(b, share_b)) => a == b && share_a == share_b,
                (a, b) => a == b,
            };
        }
        actual == expected
    }))
}

/// The continuum home dir (`$CONTINUUM_HOME`, else a test build's home, else `~/.continuum`).
/// The rule is `continuum_base::home`, the one owner `config_env` and `secrets` also use; this
/// adds only the test-build homes, so a test never reaches the operator's state.
pub fn continuum_home() -> Result<std::path::PathBuf, String> {
    // A test's per-thread native home wins over process-global env (card 3c8b03ae): the
    // captures and the recorder resolve here now, and their tests isolate per thread.
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(native) = NativeHomeOverride::current() {
        return Ok(native.join(".continuum"));
    }
    continuum_base::home::continuum_home_with(test_home)
}

/// The substrate root, infallibly: the ONE continuum home, [`continuum_home`] (`$CONTINUUM_HOME`,
/// else a test build's per-process home, else `~/.continuum`). It was a second owner reading
/// `$CONTINUUM_ROOT` and then the real home, so its 39 callers' tests could write the operator's
/// state (line 7 of the plan: one owner per fact).
pub fn continuum_root() -> std::path::PathBuf {
    continuum_home().expect("a home directory is required to resolve the continuum home") // JUSTIFIED: no home means no persistent state at all: boot-fatal by design; its callers need the infallible PathBuf
}

/// A TEST BINARY NEVER WRITES THE OPERATOR'S RECORDS (2026-09-20, the M5): a local
/// `cargo test` run of the serving daemon settled its harness lane and `save`d the
/// fixture's "coder-14b" into ~/.continuum/state/served-window.json; the next boot
/// remembered that model, found no decode knee for it, planned 8 lanes unclamped on a box
/// whose knee is 2, and swapped 4.6 GB. Every store resolves its path through this
/// function, so in any test build — when no `$CONTINUUM_HOME` is declared (an explicit one is
/// always honoured, by design) — the home is a
/// per-process directory under the system temp dir. Not configurable, not a fixture to
/// remember: a test cannot reach the operator's state by forgetting.
///
/// "Any test build" means `cfg(test)` OR `test-fixtures`. `cfg(test)` alone covers only the
/// kernel's OWN tests: once the kernel was cut out of core, core's tests (the serving daemon
/// of the incident above) and every other crate's tests built the kernel WITHOUT it and
/// resolved the real home again (card d11d9827). Every crate's test run turns on
/// `continuum-kernel/test-fixtures` (core and the cut crates through their dev-dependencies or
/// their own `test-fixtures`); resolver 2 never applies it to a production build.
#[cfg(any(test, feature = "test-fixtures"))]
fn test_home() -> Option<std::path::PathBuf> {
    static HOME: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
        let dir = std::env::temp_dir().join(format!("continuum-test-home-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    });
    Some(HOME.clone())
}
#[cfg(not(any(test, feature = "test-fixtures")))]
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
pub fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(any(test, feature = "test-fixtures"))]
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

/// A setting from the process env, else `~/.continuum/config.env`; blank is unset.
fn configured(key: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| continuum_base::config_env::read(key).filter(|v| !v.is_empty()).map(Into::into))
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

#[cfg(any(test, feature = "test-fixtures"))]
thread_local! {
    // The recorder's existing per-thread fixture-root seam, shared with the
    // other readers/writers using this policy. Models HOME absence and a native
    // home without mutating process-global environment or touching real data.
    static TEST_NATIVE_HOME: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(any(test, feature = "test-fixtures"))]
pub struct NativeHomeOverride(Option<std::path::PathBuf>);

#[cfg(any(test, feature = "test-fixtures"))]
impl NativeHomeOverride {
    pub fn install(path: &std::path::Path) -> Self {
        Self(TEST_NATIVE_HOME.with(|root| root.replace(Some(path.to_path_buf()))))
    }

    /// The override installed on THIS thread, for a test-scoped fixture to carry into
    /// a thread it spawns (the seam writes residents on their own threads, #4414):
    /// a thread-local that is not carried is a real home written from a test.
    pub fn current() -> Option<std::path::PathBuf> {
        TEST_NATIVE_HOME.with(|root| root.borrow().clone())
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
impl Drop for NativeHomeOverride {
    fn drop(&mut self) {
        TEST_NATIVE_HOME.with(|root| *root.borrow_mut() = self.0.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (line 7): two owners of the continuum home. The infallible root IS the
    // home, so a test home (or $CONTINUUM_HOME) governs every caller of either.
    #[test]
    fn the_root_is_the_one_continuum_home() {
        let _home = crate::paths::test_home::HomeGuard::preserve_blocking(); // reads the process home a guarded fixture may be swapping (card cb3221a5)
        assert_eq!(continuum_root(), continuum_home().expect("a test build always has a home")); // JUSTIFIED: test asserts the owner
    }

    // what this catches (Codex on #5420): the root escaping a test's isolation. Under a thread's
    // native home override the root is that home's .continuum, never the operator's.
    #[test]
    fn the_root_follows_a_tests_native_home() {
        let native = tempfile::tempdir().expect("native home"); // JUSTIFIED: test fixture
        let _override = NativeHomeOverride::install(native.path());
        assert_eq!(continuum_root(), native.path().join(".continuum"));
    }

    // what this catches (2026-09-20): a test binary resolving the OPERATOR's home — a
    // harness lane's settle wrote a fixture model into the M5's served-window record and
    // the next boot planned 8 lanes on it. Under test the home is a per-process temp dir.
    #[test]
    fn a_test_binary_never_resolves_the_operators_home() {
        let _home = crate::paths::test_home::HomeGuard::preserve_blocking(); // reads the process home a guarded fixture may be swapping (card cb3221a5)
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
