//! `vector/search` — cosine-similarity nearest-neighbour search over a
//! collection's embeddings.

use std::sync::Arc;

use crate::modules::data::{DataState, VectorSearchResults};

fn default_k() -> usize {
    10
}
fn default_include_data() -> bool {
    true
}

/// Params for `vector/search`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorSearchParams.ts"
)]
pub struct VectorSearchParams {
    /// The collection to search.
    pub collection: String,
    /// The query embedding to rank records against.
    pub query_vector: Vec<f64>,
    /// Max hits to return. Defaults to 10.
    #[serde(default = "default_k")]
    pub k: usize,
    /// Minimum cosine score a hit must clear. Defaults to 0 (no floor).
    #[serde(default)]
    pub threshold: f64,
    /// Include each hit's full record `data`. Defaults to true.
    #[serde(default = "default_include_data")]
    pub include_data: bool,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Find the records whose embeddings are nearest the query vector by cosine
    /// similarity. Returns the top-`k` hits above `threshold`, each with its score
    /// and distance (and full record `data` when `includeData`), plus the corpus
    /// size the search ran against. A read — gated `AiSafe`.
    pub struct VectorSearch { state: Arc<DataState> }
    name: "vector/search",
    access: AiSafe,
    params: VectorSearchParams,
    output: VectorSearchResults,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this
            .state
            .vector_search(handle, &p.collection, p.query_vector, p.k, p.threshold, p.include_data)
            .await?;
        Ok(result)
    }
}
