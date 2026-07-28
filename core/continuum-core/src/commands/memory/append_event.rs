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
    /// Events stay CACHE-ONLY by design — the room transcript is the durable
    /// record of activity ([[room-transcript-is-not-durable]] carves the split:
    /// memories persist to longterm.db, session events don't) — but a missing
    /// corpus still hydrates first so the event lands on top of history.
    pub struct MemoryAppendEvent { state: Arc<MemoryState> }
    name: "memory/append-event",
    access: Privileged,
    params: MemoryAppendEventParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_append_event");
        if super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await?.is_some() {
            log_debug!(
                "module", "memory_append_event",
                "Hydrated corpus for {} from durable store", p.persona_id
            );
        }
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
