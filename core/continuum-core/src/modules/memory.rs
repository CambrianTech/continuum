//! MemoryModule — owns the per-persona `Arc<MemoryState>` for memory/recall.
//!
//! The `memory/*` commands (load-corpus, multi-layer-recall, consciousness-context,
//! append-memory, append-event) are TYPED [`ActionCommand`](crate::sdk_codegen::ActionCommand)s
//! on the ONE registry, living in [`crate::commands::memory`] and exposed here via
//! [`commands()`](MemoryModule::commands) — so a persona is OFFERED recall as a tool.
//! This module no longer dispatches them through `handle_command` (the legacy arm is
//! retired; the executor routes the typed objects first).
//!
//! Recall/context compute is pure in-memory corpus work. DURABILITY is the data
//! layer's: `memory/append-memory` writes through to the persona's `longterm.db`
//! via `data/create` (the late-bound executor below), and the corpus HYDRATES from
//! that store on first touch after boot — the DB is the truth, the corpus is the
//! derived cache. Same late-bound-executor shape as `ChatModule` (task #224).

use crate::memory::PersonaMemoryManager;
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority,
    ServiceModule,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// Shared state for memory module.
pub struct MemoryState {
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    pub memory_manager: Arc<PersonaMemoryManager>,
    /// Substrate-wide command executor, installed by `start_server` via
    /// [`ServiceModule::install_executor`]. The `memory/*` commands use it to
    /// write memories through to the persona's durable `longterm.db` (via
    /// `data/create`) and to hydrate a missing corpus from it (via `data/list`)
    /// — the cross-module dual-write pattern `ChatModule` established.
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
    /// Persona-RAID ledger — the ONE owner of this node's write-behind
    /// journals + replica high-waters (no statics; see memory/replication.rs).
    pub replication: Arc<crate::memory::replication::ReplicationLedger>,
}

impl MemoryState {
    pub fn new(memory_manager: Arc<PersonaMemoryManager>) -> Self {
        Self {
            memory_manager,
            executor_slot: Arc::new(LateBound::new("memory::executor")),
            replication: Arc::new(crate::memory::replication::ReplicationLedger::from_env()),
        }
    }

    /// Resolve the executor, or a loud error naming the boot-ordering contract.
    /// Per [[fallbacks-are-illegal-fail-loud]]: a memory write that cannot reach
    /// the durable store must refuse, never silently degrade to cache-only.
    pub fn executor(&self) -> Result<Arc<CommandExecutor>, String> {
        self.executor_slot.cloned().ok_or_else(|| {
            "MemoryState: CommandExecutor not installed — start_server must call \
             install_executor_on_all before any memory command can persist"
                .to_string()
        })
    }
}

pub struct MemoryModule {
    state: Arc<MemoryState>,
}

impl MemoryModule {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for MemoryModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "memory",
            priority: ModulePriority::Normal,
            command_prefixes: &["memory/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// The `memory/*` commands are migrated to the typed registry — see
    /// [`commands()`](Self::commands) and [`crate::commands::memory`]. The executor
    /// routes those names through the O(1) typed object map (`route_object`) BEFORE
    /// this legacy prefix path, so they never reach here. Any name that DOES fall
    /// through is unregistered — fail loud naming it rather than silently matching.
    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "'{command}' is not a registered memory command — the memory/* family is on \
             the typed registry (crate::commands::memory); legacy handle_command is retired"
        ))
    }

    /// The migrated memory commands as typed self-routing objects on the ONE registry,
    /// each sharing this module's `Arc<MemoryState>`. Their descriptors flow into
    /// `command_registry()` → the persona tool surface + grid ACL; the executor routes
    /// their names straight here. See [`crate::commands::memory`].
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::memory::command_objects(self.state.clone())
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        // `LateBound::install` no-ops when already filled, so a test-injected
        // executor is never clobbered — same contract as ChatModule.
        self.state.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
