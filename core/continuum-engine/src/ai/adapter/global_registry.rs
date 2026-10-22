//! The process's one adapter registry: the engine's table of inference adapters, which the
//! layers above populate (the AI provider module registers its adapters at boot) and every
//! inference seam reads. The registry is an engine fact, so it lives with the adapter types;
//! the populating module calls down to it rather than the engine reaching up.

use std::sync::{Arc, LazyLock};

use tokio::sync::RwLock;

use super::AdapterRegistry;

/// Survives the AI provider module being recreated on a server restart.
static GLOBAL_REGISTRY: LazyLock<Arc<RwLock<AdapterRegistry>>> =
    LazyLock::new(|| Arc::new(RwLock::new(AdapterRegistry::new())));

/// The shared adapter registry.
pub fn global_registry() -> Arc<RwLock<AdapterRegistry>> {
    Arc::clone(&GLOBAL_REGISTRY)
}
