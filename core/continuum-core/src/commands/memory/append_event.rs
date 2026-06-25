//! `memory/append-event` — append one timeline event to a persona's cached corpus.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AppendResult;
use crate::log_debug;
use crate::logging::TimingGuard;
use crate::memory::CorpusTimelineEvent;
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

/// Params for `memory/append-event`. Wire keys are snake_case.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryAppendEventParams.ts"
)]
pub struct MemoryAppendEventParams {
    /// Which persona's corpus to append to.
    pub persona_id: String,
    /// The timeline event (with optional precomputed embedding) to append.
    pub event: CorpusTimelineEvent,
}

crate::action_command! {
    /// Append a single timeline event to a persona's cached corpus (in-place O(1)
    /// write, trims to capacity). Records cross-context activity for consciousness.
    pub struct MemoryAppendEvent { state: Arc<MemoryState> }
    name: "memory/append-event",
    access: Privileged,
    params: MemoryAppendEventParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_append_event");
        this.state
            .memory_manager
            .append_event(&p.persona_id, p.event)
            .map_err(|e| CommandError::Internal(format!("memory/append-event failed: {e}")))?;
        log_debug!(
            "module", "memory_append_event",
            "Appended event to corpus for {}", p.persona_id
        );
        Ok(AppendResult { appended: true })
    }
}
