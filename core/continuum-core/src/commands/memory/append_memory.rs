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
    pub persona_id: String,
    /// The memory (with optional precomputed embedding) to append.
    pub memory: CorpusMemory,
}

crate::action_command! {
    /// Append a single memory to a persona's cached corpus (in-place O(1) write,
    /// trims to capacity). Used by consolidation as new memories form.
    pub struct MemoryAppendMemory { state: Arc<MemoryState> }
    name: "memory/append-memory",
    access: Privileged,
    params: MemoryAppendMemoryParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_append_memory");
        this.state
            .memory_manager
            .append_memory(&p.persona_id, p.memory)
            .map_err(|e| CommandError::Internal(format!("memory/append-memory failed: {e}")))?;
        log_debug!(
            "module", "memory_append_memory",
            "Appended memory to corpus for {}", p.persona_id
        );
        Ok(AppendResult { appended: true })
    }
}
