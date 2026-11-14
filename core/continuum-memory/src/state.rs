//! `MemoryState`: the per-persona memory manager and the late-bound command executor the
//! `memory/*` commands share (the module that owns it composes the command list).

use crate::memory::PersonaMemoryManager;
use crate::runtime::{CommandExecutor, LateBound, StorageProvider};
use std::sync::Arc;

/// Shared state for memory module.
pub struct MemoryState {
    storage: LateBound<dyn StorageProvider>,
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    pub memory_manager: Arc<PersonaMemoryManager>,
    /// Substrate-wide command executor, installed by `start_server` via
    /// [`ServiceModule::install_executor`]. The `memory/*` commands use it to
    /// write memories through to the persona's durable `longterm.db` (via
    /// `data/create`) and to hydrate a missing corpus from it (via `data/list`)
    /// — the cross-module dual-write pattern `ChatModule` established.
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

impl MemoryState {
    pub fn new(memory_manager: Arc<PersonaMemoryManager>) -> Self {
        Self {
            storage: LateBound::new("memory::storage"),
            memory_manager,
            executor_slot: Arc::new(LateBound::new("memory::executor")),
        }
    }

    /// Module initialization supplies the existing shared storage owner.
    pub fn install_storage(&self, storage: Arc<dyn StorageProvider>) {
        self.storage.install(storage);
    }

    pub fn storage(&self) -> Result<&dyn StorageProvider, String> {
        Ok(self.storage.require()?.as_ref())
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
