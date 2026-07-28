//! `memory/multi-layer-recall` — the persona's primary recall API.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::{MemoryRecallResponse, MultiLayerRecallRequest};
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

fn default_max_results() -> usize {
    10
}

/// Params for `memory/multi-layer-recall`. Wire keys are snake_case.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryMultiLayerRecallParams.ts"
)]
pub struct MemoryMultiLayerRecallParams {
    /// Which persona's corpus to recall from.
    pub persona_id: String,
    /// The semantic query. Absent ⇒ the semantic layer degrades to non-semantic recall.
    #[serde(default)]
    #[ts(optional)]
    pub query_text: Option<String>,
    /// Room scope for the recall.
    pub room_id: String,
    /// Max memories to return.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Which recall layers to run (absent/empty ⇒ all layers).
    #[serde(default)]
    #[ts(optional)]
    pub layers: Option<Vec<String>>,
}

crate::action_command! {
    /// Recall a persona's most relevant memories for a query and room using the
    /// 6-layer parallel recall (recency, semantic, importance, cross-context, …).
    /// The query is embedded once through the adapter-routed embedder; an empty
    /// embedding degrades the semantic layer rather than failing.
    pub struct MemoryMultiLayerRecall { state: Arc<MemoryState> }
    name: "memory/multi-layer-recall",
    access: AiSafe,
    params: MemoryMultiLayerRecallParams,
    output: MemoryRecallResponse,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_multi_layer_recall");
        // First touch after a restart: hydrate the corpus from the persona's
        // durable longterm.db — recall reads THROUGH to the truth, it never
        // reports an empty mind just because the cache is cold (card aded8871).
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await? {
            log_info!(
                "module", "memory_multi_layer_recall",
                "Hydrated corpus for {} from durable store ({loaded} memories)", p.persona_id
            );
        }
        let req = MultiLayerRecallRequest {
            query_text: p.query_text,
            room_id: p.room_id,
            max_results: p.max_results,
            layers: p.layers,
        };
        let resp = this
            .state
            .memory_manager
            .multi_layer_recall(&p.persona_id, &req)
            .await
            .map_err(|e| CommandError::Internal(format!("memory/multi-layer-recall failed: {e}")))?;
        log_info!(
            "module", "memory_multi_layer_recall",
            "Multi-layer recall for {}: {} memories in {:.1}ms ({} candidates from {} layers)",
            p.persona_id, resp.memories.len(), resp.recall_time_ms,
            resp.total_candidates, resp.layer_timings.len()
        );
        Ok(resp)
    }
}
