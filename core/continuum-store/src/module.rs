//! The data module: the `ServiceModule` shell that wires the store's state ([`crate::data`]) to its
//! commands ([`crate::commands`]). It sits ABOVE both, so the state never imports its commands and
//! the store's modules form no cycle (card 2709ec6d; in core the two lived under different top
//! modules, and the cut made the loop visible to `modules_in_cycles`).

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::data::DataState;
use crate::log_info;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// `ServiceModule` shell over the shared [`DataState`]. The kernel registers
/// this; the typed `data/*` commands capture `self.state.clone()` via
/// [`DataModule::commands`]. All storage logic lives on `DataState`.
pub struct DataModule {
    pub(crate) state: Arc<DataState>,
    substrate_entities: SubstrateEntities,
}

/// Registers the Rust-authored ORM entities that the layers ABOVE the store own (the persona
/// substrate's `staged_credit` and its siblings) into the global registry, and returns how many
/// collections resolve afterwards. The store cannot name those layers, so the boot that knows them
/// supplies this (core's `modules::data_wiring`; card 2709ec6d). A conflict is a boot failure.
pub type SubstrateEntities = fn() -> Result<usize, String>;

/// The store's own tests exercise the store, not the persona substrate: they register no entities.
#[cfg(test)]
pub(crate) fn no_substrate_entities() -> Result<usize, String> {
    Ok(0)
}

impl DataModule {
    /// The data module, registering `substrate_entities` when it initializes. A required argument,
    /// so no caller can silently boot without the substrate's collections.
    pub fn new(substrate_entities: SubstrateEntities) -> Self {
        Self {
            state: Arc::new(DataState::new()),
            substrate_entities,
        }
    }
}

#[async_trait]
impl ServiceModule for DataModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "data",
            priority: ModulePriority::Normal,
            // `vector/*` fully migrated to typed self-routing commands
            // (commands/vector/*.rs) — no legacy arm remains, so the prefix is gone.
            command_prefixes: &["data/", "adapter/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // THE SUBSTRATE'S OWN ENTITIES (2026-09-14): `register_substrate_orm_entities`
        // existed with a doc comment saying "boot wires it" and NO caller — so every
        // `data/ensure-schema` for a Rust-authored collection (`staged_credit`, the
        // learning flywheel's per-persona credit) was refused as an unknown
        // collection, on every node, and the flywheel stayed dead for work turns
        // (training.credit.stage_failed ×2 per act turn, 0 staged). A conflict is a
        // boot failure, never a warning: two shapes for one collection is a bug.
        let registered = (self.substrate_entities)()?;
        continuum_base::probe!(
            class = "orm.substrate_entities.registered",
            registered = registered as u64,
            "Rust-authored ORM entities registered before the first ensure-schema"
        );
        // Store context for event publishing
        let ctx_arc = Arc::new(ModuleContext::new(
            ctx.registry.clone(),
            ctx.bus.clone(),
            ctx.compute.clone(),
            ctx.runtime.clone(),
        ));
        *self
            .state
            .context
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(ctx_arc);
        log_info!("data", "init", "DataModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // Legacy Registry-A entry point — delegates to the shared state's
        // in-process dispatch. As `data/*` arms migrate to typed commands in
        // `commands/data/*`, the executor's `route_object` path wins first and
        // this string match shrinks toward deletion (Wave Z).
        self.state.dispatch(command, params).await
    }

    async fn shutdown(&self) -> Result<(), String> {
        // Close all adapters - clear the DashMap
        // Adapters will clean up when their Arc refcount drops to zero
        self.state.adapters.clear();
        Ok(())
    }

    /// The migrated `data/*` commands as typed self-routing objects on the ONE
    /// registry. Each shares this module's `Arc<DataState>`; the executor routes
    /// their names straight here (winning over the legacy `data/` prefix arm),
    /// and their `CommandSpec` descriptors flow into `command_registry()` → the
    /// persona tool surface + grid ACL. See [`crate::commands::data`].
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        // DataModule owns the adapter pool, so it contributes the `data/*`
        // content commands, the `vector/*` embedding-search commands, the
        // `adapter/*` introspection commands, and the `migration/*` operator
        // commands — each family sharing this module's `Arc<DataState>`.
        let mut objects = crate::commands::data::command_objects(self.state.clone());
        objects.extend(crate::commands::vector::command_objects(self.state.clone()));
        objects.extend(crate::commands::adapter::command_objects(
            self.state.clone(),
        ));
        objects.extend(crate::commands::migration::command_objects(
            self.state.clone(),
        ));
        objects
    }

    fn as_storage(&self) -> Option<Arc<dyn crate::runtime::StorageProvider>> {
        Some(self.state.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
