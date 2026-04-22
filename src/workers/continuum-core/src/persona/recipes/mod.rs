//! Built-in recipes + default registry seeding + global accessor.
//!
//! Hosts that want the standard set call `init_default_global()` to
//! initialize the process-wide registry with all the built-in
//! recipes. The IPC handler looks up recipes via `global()`.
//!
//! Custom hosts (Unreal, Swift, etc.) can either:
//! - Use `init_default_global()` then add their own recipes via
//!   `global_register()`, OR
//! - Skip the global entirely and manage their own RecipeRegistry
//!   directly via `RecipeRegistry::new()` + `register()`. The
//!   `respond_via_recipe()` entry point in persona::response takes
//!   a `&dyn Recipe` directly — global is convenience for the chat
//!   IPC, not the only path.

pub mod chat;

use crate::persona::recipe::{Recipe, RecipeRegistry};
use parking_lot::RwLock;
use std::sync::{Arc, OnceLock};

/// Process-wide registry. Initialized at module startup via
/// `init_default_global()`. The IPC handler (cognition/respond) reads
/// from it on every dispatch.
///
/// Wrapped in `RwLock` so hosts can register additional recipes after
/// init without a re-init dance. Reads are lock-free for the
/// concurrent IPC dispatch case (RwLock allows parallel readers).
static GLOBAL_REGISTRY: OnceLock<RwLock<RecipeRegistry>> = OnceLock::new();

/// Seed the process-wide registry with the built-in recipes. MUST be
/// called once at startup before any IPC handler dispatches. Idempotent
/// re-init logs a warning and replaces the existing registry — useful
/// for tests, problematic in production (would lose host-registered
/// custom recipes).
pub fn init_default_global() {
    let registry = init_default();
    if GLOBAL_REGISTRY.set(RwLock::new(registry)).is_err() {
        // Re-init: replace contents in place. Tests rely on this when
        // they reset state between runs.
        let mut existing = GLOBAL_REGISTRY
            .get()
            .expect("just verified set returned Err meaning value exists")
            .write();
        *existing = init_default();
    }
}

/// Build a fresh registry with built-in recipes — no global side effect.
/// Useful for tests that want their own isolated registry.
pub fn init_default() -> RecipeRegistry {
    let mut reg = RecipeRegistry::new();
    reg.register(Arc::new(chat::ChatRecipe));
    reg
}

/// Look up a recipe by name in the process-wide registry. Returns
/// None if the registry hasn't been initialized OR the name isn't
/// registered. Caller's responsibility to translate None into the
/// appropriate IPC error.
pub fn global_get(name: &str) -> Option<Arc<dyn Recipe>> {
    GLOBAL_REGISTRY.get()?.read().get(name)
}

/// Register an additional recipe in the process-wide registry. Used
/// by custom hosts (Unreal, Swift) to add their own recipes after
/// init_default_global has run. Returns Err if the global registry
/// hasn't been initialized — caller must call init_default_global
/// first (or build a non-global registry).
pub fn global_register(recipe: Arc<dyn Recipe>) -> Result<(), String> {
    let registry = GLOBAL_REGISTRY
        .get()
        .ok_or_else(|| "RecipeRegistry not initialized — call init_default_global first".to_string())?;
    registry.write().register(recipe);
    Ok(())
}

/// All currently-registered recipe names. For observability + the
/// hypothetical future `cognition/list-recipes` IPC. Returns empty
/// vec if the registry isn't initialized.
pub fn global_list() -> Vec<&'static str> {
    GLOBAL_REGISTRY
        .get()
        .map(|r| r.read().list())
        .unwrap_or_default()
}
