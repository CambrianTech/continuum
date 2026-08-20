//! `vector/invalidate-cache` — drop a collection's cached vector set.

use std::sync::Arc;

use crate::modules::data::{DataState, VectorCacheInvalidation};

/// Params for `vector/invalidate-cache`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorInvalidateCacheParams.ts"
)]
pub struct VectorInvalidateCacheParams {
    /// The collection whose cached vectors should be dropped.
    pub collection: String,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Drop a collection's in-memory cached vector set so the next search
    /// reloads it from storage. Called when records are modified outside
    /// `vector/index`. Mutates cache state, so gated `Privileged`.
    pub struct VectorInvalidateCache { state: Arc<DataState> }
    name: "vector/invalidate-cache",
    access: Privileged,
    params: VectorInvalidateCacheParams,
    output: VectorCacheInvalidation,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.invalidate_vector_cache(handle, &p.collection).await?;
        Ok(result)
    }
}
