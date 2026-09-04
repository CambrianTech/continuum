//! `memory/consciousness-context` — build temporal + cross-context + intention context.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::{ConsciousnessContextRequest, ConsciousnessContextResponse};
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

/// Params for `memory/consciousness-context`. Wire keys are snake_case.
// No `Default`: a persona reference has no sensible default, and an empty one
// would read as a real answer. Construct these params explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryConsciousnessContextParams.ts"
)]
pub struct MemoryConsciousnessContextParams {
    /// Which persona to build consciousness context for.
    pub persona_id: crate::identity::PersonaRef,
    /// Room scope for the context.
    pub room_id: String,
    /// The message currently being considered (focuses cross-context retrieval).
    #[serde(default)]
    #[ts(optional)]
    pub current_message: Option<String>,
    /// Skip the (more expensive) semantic search portion when true.
    #[serde(default)]
    pub skip_semantic_search: bool,
}

crate::action_command! {
    /// Build a persona's consciousness context for a room: temporal continuity
    /// ("what was I doing before?"), cross-context events, and active intentions,
    /// formatted for RAG injection. Cached per-persona with a short TTL.
    pub struct MemoryConsciousnessContext { state: Arc<MemoryState> }
    name: "memory/consciousness-context",
    access: AiSafe,
    params: MemoryConsciousnessContextParams,
    output: ConsciousnessContextResponse,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_consciousness_context");
        // Cold-cache read-through: hydrate from the durable store on first
        // touch after a restart (same seam as multi-layer-recall, card aded8871).
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await? {
            log_info!(
                "module", "memory_consciousness_context",
                "Hydrated corpus for {} from durable store ({loaded} memories)", p.persona_id
            );
        }
        let req = ConsciousnessContextRequest {
            room_id: p.room_id,
            current_message: p.current_message,
            skip_semantic_search: p.skip_semantic_search,
        };
        let resp = this
            .state
            .memory_manager
            .consciousness_context(&p.persona_id, &req)
            .map_err(|e| {
                CommandError::Internal(format!("memory/consciousness-context failed: {e}"))
            })?;
        log_info!(
            "module", "memory_consciousness_context",
            "Consciousness context for {}: {:.1}ms, {} cross-context events, {} intentions",
            p.persona_id, resp.build_time_ms, resp.cross_context_event_count,
            resp.active_intention_count
        );
        Ok(resp)
    }
}
