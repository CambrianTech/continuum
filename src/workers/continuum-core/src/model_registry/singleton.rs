//! Process-wide `Registry` singleton — load once at boot, read everywhere.
//!
//! Continuum-core loads the registry during init (`init_global` called
//! from `main.rs` / `backend_init()`). Adapters and inference code ask
//! `global()` for the live registry and look up models / providers by id.
//!
//! **Why a singleton.** Registry is immutable after load (TOML is read
//! once, no runtime writes), so `&'static Registry` is the natural fit.
//! Threading it through every adapter constructor would be boilerplate
//! without benefit — there's only ever one. The singleton is filled
//! EXACTLY ONCE; subsequent `init_global` calls are no-ops (idempotent
//! by design so tests can re-seed with their own fixture paths).
//!
//! **Why not lazy_static / build-time.** We want explicit control of
//! WHEN load happens (after logging is up, before any adapter touches it)
//! and WHERE load reads from (env override for deployment, crate-dir
//! default for dev/test). A deferred `init_global` keeps that control.

use super::loader::{load_registry, Registry, RegistryError};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static GLOBAL: OnceLock<Registry> = OnceLock::new();

/// Default models/providers TOML paths — `{CARGO_MANIFEST_DIR}/config/*.toml`.
/// These are the checked-in source-of-truth files. Deployment environments
/// can override via `CONTINUUM_MODEL_REGISTRY_DIR` env var pointing at an
/// alternate directory that contains `models.toml` + `providers.toml`.
fn default_paths() -> (PathBuf, PathBuf) {
    let base: PathBuf = std::env::var("CONTINUUM_MODEL_REGISTRY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config"));
    (base.join("models.toml"), base.join("providers.toml"))
}

/// Initialize the process-wide registry. Idempotent: subsequent calls
/// are ignored (the first one wins). Returns the registry reference so
/// callers can do one-liner boot:
///
/// ```no_run
/// let reg = continuum_core::model_registry::init_global()?;
/// println!("{} models loaded", reg.models().count());
/// # Ok::<(), continuum_core::model_registry::RegistryError>(())
/// ```
pub fn init_global() -> Result<&'static Registry, RegistryError> {
    let (models, providers) = default_paths();
    init_global_from(&models, &providers)
}

/// Initialize from explicit paths. Used by tests + any deployment that
/// keeps its config outside `CARGO_MANIFEST_DIR`. Idempotent same as
/// `init_global`.
pub fn init_global_from(
    models: &Path,
    providers: &Path,
) -> Result<&'static Registry, RegistryError> {
    // If GLOBAL is already set, the first-loaded one wins. We don't
    // re-load on subsequent calls — that would break the "load once"
    // guarantee tests rely on. Use `try_init_with_result` pattern.
    if let Some(existing) = GLOBAL.get() {
        return Ok(existing);
    }
    let reg = load_registry(models, providers)?;
    // Race: two threads may hit here simultaneously. OnceLock::set
    // returns Err on the loser thread; we discard its registry and
    // return the winner's.
    match GLOBAL.set(reg) {
        Ok(()) => Ok(GLOBAL.get().expect("GLOBAL just set")),
        Err(_lost) => Ok(GLOBAL.get().expect("GLOBAL already set by race winner")),
    }
}

/// Read the global registry. Panics if `init_global` was never called —
/// this is a PROGRAMMER error (forgot to wire init into boot path), not
/// a config error, so panic is the right shape: loud, pointing at the
/// missing init call. Production init MUST happen in `backend_init()`
/// before any adapter constructor runs.
pub fn global() -> &'static Registry {
    GLOBAL.get().unwrap_or_else(|| {
        panic!(
            "model_registry::global() called before init_global() — \
             add `model_registry::init_global()` to the startup path \
             (continuum-core's backend_init, or the test harness)."
        )
    })
}

/// Non-panicking variant. Returns `None` if the registry hasn't been
/// initialized. Useful when the caller legitimately might run before
/// the registry is up (e.g. pre-init logging).
pub fn try_global() -> Option<&'static Registry> {
    GLOBAL.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::Capability;

    #[test]
    fn init_once_picks_up_seeded_config() {
        // Idempotent init — test isolation is tricky for OnceLock statics;
        // if another test already called init_global, this call reuses
        // that registry. That's still a valid state under our "first
        // caller wins" contract, so the assertion just has to hold
        // regardless of order.
        let reg = init_global().expect("seeded config must load");
        assert!(reg.models().count() > 0);
        assert!(reg.providers().count() > 0);
        // Canonical anchor: Claude Sonnet 4.5 must exist and have Vision.
        let sonnet = reg
            .model("claude-sonnet-4-5-20250929")
            .expect("sonnet in registry");
        assert!(sonnet.has(Capability::Vision));
    }
}
