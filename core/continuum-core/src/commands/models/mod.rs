//! `models/<verb>` — the rich, real-time model API as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! "What models exist, what can each do, are they downloaded, have we verified
//! them, how fast are they?" — answered live, mutated without a reboot. The
//! data comes from the live [`ModelCatalog`](crate::model_registry::live::ModelCatalog):
//! seeded from the immutable registry at boot, mutated at runtime by this very
//! surface (`pull`/`try`/`register` in later slices). Reads project the current
//! `Arc<CatalogSnapshot>` lock-free; the widget/persona subscribe to the watch
//! channel and react when the universe changes.
//!
//! ## Why typed commands (not a `match` arm)
//!
//! The legacy `ModelsModule::handle_command` arms were dispatchable but had no
//! descriptor in `command_registry()`, so a persona was never OFFERED model
//! management as a tool. As typed commands each gets a descriptor (persona tool
//! surface, grid ACL, codegen, `cu`) AND routes through the O(1) lock-free typed
//! path. The wire name mirrors the file path — `commands/models/list.rs` ⟺
//! `models/list`.
//!
//! ## State ownership
//!
//! [`ModelsList`] and [`ModelsCapabilities`] capture the shared
//! `Arc<ModelCatalog>` (dep-holding form) so every caller reads the SAME live
//! universe. [`ModelsDiscover`] is stateless — it queries provider HTTP APIs and
//! is auto-registered; it joins the catalog only once discovery-registration
//! lands (#74).

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::DynCommand;

pub mod capabilities;
pub mod discover;
pub mod list;
pub mod try_;

use capabilities::ModelsCapabilities;
use list::ModelsList;
use try_::ModelsTry;

/// The dep-holding `models/*` command objects the [`ModelsModule`](crate::modules::models::ModelsModule)
/// contributes to the kernel's typed object map. The read commands share the one
/// `Arc<ModelCatalog>` so they read a single live universe; `models/try` also
/// captures the shared `AdapterRegistry` (the SAME global pool `ai/generate`
/// uses — never a parallel allocator) because verification runs the model.
/// `models/discover` is stateless and self-registers, so it is intentionally
/// absent here.
pub fn command_objects(
    catalog: Arc<ModelCatalog>,
    registry: Arc<RwLock<AdapterRegistry>>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(ModelsList {
            catalog: catalog.clone(),
        }),
        Arc::new(ModelsCapabilities {
            catalog: catalog.clone(),
        }),
        Arc::new(ModelsTry { catalog, registry }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;

    // what this catches: the dep-holding family wires exactly the catalog-backed
    // commands with a shared Arc<ModelCatalog>. A regression that drops one (or
    // accidentally lists the stateless discover, double-registering it) is caught.
    #[test]
    fn family_exposes_the_catalog_backed_commands() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let cat = Arc::new(ModelCatalog::from_registry(&reg));
        let adapters = Arc::new(RwLock::new(AdapterRegistry::new()));
        let objs = command_objects(cat, adapters);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"models/list"));
        assert!(names.contains(&"models/capabilities"));
        assert!(names.contains(&"models/try"));
        assert!(
            !names.contains(&"models/discover"),
            "discover is stateless + self-registered, never in the dep-holding family"
        );
    }
}
