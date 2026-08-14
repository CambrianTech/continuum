//! `data/count` — count records in a collection (optionally filtered).

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::StorageResult;

/// Params for `data/count`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataCountParams.ts"
)]
pub struct DataCountParams {
    /// The collection to count.
    pub collection: String,
    /// Optional equality filter (`{ "field": value }`); omit to count all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub filter: Option<serde_json::Map<String, serde_json::Value>>,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Count records in a collection. Returns an accurate SQL COUNT (the true
    /// total, independent of any paging). An optional equality filter narrows the
    /// count. Reading shared state is a read — gated `AiSafe`.
    pub struct DataCount { state: Arc<DataState> }
    name: "data/count",
    access: AiSafe,
    params: DataCountParams,
    output: StorageResult<usize>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.count_records(handle, p.collection, p.filter).await?;
        Ok(result)
    }
}
