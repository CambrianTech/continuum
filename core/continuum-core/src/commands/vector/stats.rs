//! `vector/stats` — embedding coverage statistics for a collection.

use std::sync::Arc;

use crate::modules::data::{DataState, VectorStats};

/// Params for `vector/stats`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorStatsParams.ts"
)]
pub struct VectorStatsParams {
    /// The collection to report on.
    pub collection: String,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Report how many records in a collection carry an embedding, the vector
    /// dimensionality, and the in-memory cache occupancy. A read — gated `AiSafe`.
    pub struct VectorStatsCommand { state: Arc<DataState> }
    name: "vector/stats",
    access: AiSafe,
    params: VectorStatsParams,
    output: VectorStats,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.vector_stats(handle, &p.collection).await?;
        Ok(result)
    }
}
