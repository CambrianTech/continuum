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
