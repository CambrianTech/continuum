//! `data/collection-stats` — statistics for one collection.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{CollectionStats, StorageResult};

/// Params for `data/collection-stats`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataCollectionStatsParams.ts"
)]
pub struct DataCollectionStatsParams {
    /// The collection to describe.
    pub collection: String,
    /// Storage handle. Defaults to "main" (the shared DB).
    // `dbPath` on the wire, NOT `handle`: the envelope owns that name and refuses a
    // string for it (`expected struct HandleRef`) — card ea28d2f6. The Rust field keeps
    // its name; only the wire changes.
    #[serde(default, rename = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Statistics for one collection: record count, total size, last-modified,
    /// schema, and indices. Gated `AiSafe`.
    pub struct DataCollectionStats { state: Arc<DataState> }
    name: "data/collection-stats",
    access: AiSafe,
    params: DataCollectionStatsParams,
    output: StorageResult<CollectionStats>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.collection_statistics(handle, &p.collection).await?;
        Ok(result)
    }
}
