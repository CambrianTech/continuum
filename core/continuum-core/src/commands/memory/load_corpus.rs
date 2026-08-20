//! `memory/load-corpus` — load (replace) a persona's in-memory corpus from the ORM.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::{CorpusMemory, CorpusTimelineEvent, LoadCorpusResponse};
use crate::modules::memory::MemoryState;

/// Params for `memory/load-corpus`. Wire keys are snake_case (the ORM IPC contract).
// No `Default`: a persona reference has no sensible default, and an empty one
// would read as a real answer. Construct these params explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryLoadCorpusParams.ts"
)]
pub struct MemoryLoadCorpusParams {
    /// Which persona's corpus to (re)load — replaces any previously cached corpus.
    pub persona_id: crate::identity::PersonaRef,
    /// Memories with optional precomputed embedding vectors (sent from the ORM).
    #[serde(default)]
    pub memories: Vec<CorpusMemory>,
    /// Timeline events with optional precomputed embedding vectors (sent from the ORM).
    #[serde(default)]
    pub events: Vec<CorpusTimelineEvent>,
}

crate::action_command! {
    /// Load a persona's memory corpus (memories + timeline events) into the in-memory
    /// store, replacing any previously cached corpus. Called by the ORM bring-up path
    /// before recall is possible. Returns the loaded/embedded counts and load time.
    pub struct MemoryLoadCorpus { state: Arc<MemoryState> }
    name: "memory/load-corpus",
    access: Privileged,
    params: MemoryLoadCorpusParams,
    output: LoadCorpusResponse,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_load_corpus");
        // Empty-clobber guard (card aded8871 follow-up, live incident 2026-07-24):
        // the legacy skill preloaded an EMPTY corpus before appending, which
        // REPLACED a hydrated corpus with nothing — reintroducing amnesia through
        // the front door. An empty load over an existing corpus is never a real
        // reload (the durable store is the truth and appends hydrate on first
        // touch), so refuse it loudly instead of clobbering.
        if p.memories.is_empty()
            && p.events.is_empty()
            && this.state.memory_manager.has_corpus(&p.persona_id)
        {
            return Err(crate::sdk_codegen::CommandError::Invalid(format!(
                "memory/load-corpus: refusing to replace {}'s live corpus with an EMPTY one — \
                 appends and recall hydrate from the durable store on first touch; an empty \
                 preload is obsolete and would clobber loaded memories",
                p.persona_id
            )));
        }
        let resp = this
            .state
            .memory_manager
            .load_corpus(&p.persona_id, p.memories, p.events);
        log_info!(
            "module", "memory_load_corpus",
            "Loaded corpus for {}: {} memories ({} embedded), {} events ({} embedded), {:.1}ms",
            p.persona_id, resp.memory_count, resp.embedded_memory_count,
            resp.timeline_event_count, resp.embedded_event_count, resp.load_time_ms
        );
        Ok(resp)
    }
}
