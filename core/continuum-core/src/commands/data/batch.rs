//! `data/batch` — apply a batch of create/update/delete operations.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{BatchOperation, StorageResult};

/// Params for `data/batch`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataBatchParams.ts"
)]
pub struct DataBatchParams {
    /// The operations to apply (each names its own collection + type).
    pub operations: Vec<BatchOperation>,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Apply a batch of create/update/delete operations and return each result.
    /// Writes shared state, so gated `Privileged`.
    pub struct DataBatch { state: Arc<DataState> }
    name: "data/batch",
    access: Privileged,
    params: DataBatchParams,
    output: StorageResult<Vec<serde_json::Value>>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.batch_operations(handle, p.operations).await?;
        Ok(result)
    }
}
