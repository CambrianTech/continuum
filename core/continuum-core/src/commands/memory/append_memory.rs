//! `memory/append-memory` — append one memory to a persona's cached corpus.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AppendResult;
use crate::log_debug;
use crate::logging::TimingGuard;
use crate::memory::CorpusMemory;
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

/// Params for `memory/append-memory`. Wire keys are snake_case.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryAppendMemoryParams.ts"
)]
pub struct MemoryAppendMemoryParams {
    /// Which persona's corpus to append to.
    pub persona_id: crate::identity::PersonaRef,
    /// The memory (with optional precomputed embedding) to append.
    pub memory: CorpusMemory,
}

crate::action_command! {
    /// Append a single memory to a persona's memory: written through to the
    /// persona's durable `longterm.db` FIRST (the data layer is the truth),
    /// then into the cached corpus (in-place O(1) write, trims to capacity).
    /// A missing corpus hydrates from the store before the append, so the
    /// first write after a core restart lands on top of history, not a void.
    pub struct MemoryAppendMemory { state: Arc<MemoryState> }
    name: "memory/append-memory",
    access: Privileged,
    params: MemoryAppendMemoryParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_append_memory");
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await? {
            log_debug!(
                "module", "memory_append_memory",
                "Hydrated corpus for {} from durable store ({loaded} memories)", p.persona_id
            );
        }
        // Durable first, cache second — if the durable write fails the call
        // fails loud and the cache is untouched (truth never trails the cache).
        super::persist_memory(&this.state, &p.persona_id, &p.memory).await?;
        this.state
            .memory_manager
            .append_memory(&p.persona_id, p.memory)
            .map_err(|e| CommandError::Internal(format!("memory/append-memory failed: {e}")))?;
        log_debug!(
            "module", "memory_append_memory",
            "Appended memory (durable + corpus) for {}", p.persona_id
        );
        Ok(AppendResult { appended: true })
    }
}
