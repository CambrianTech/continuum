//! `vector/index` — store an embedding on a record.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{DataRecord, StorageResult};

/// Params for `vector/index`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorIndexParams.ts"
)]
pub struct VectorIndexParams {
    /// The collection holding the record.
    pub collection: String,
    /// The record id to attach the embedding to.
    pub id: String,
    /// The embedding to store on the record's `embedding` field.
    pub embedding: Vec<f64>,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Store an embedding on a record (its `embedding` field) and drop the
    /// collection's cached vector set so the next search reloads it. Writes a
    /// record, so gated `Privileged`.
    pub struct VectorIndex { state: Arc<DataState> }
    name: "vector/index",
    access: Privileged,
    params: VectorIndexParams,
    output: StorageResult<DataRecord>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this
            .state
            .index_vector(handle, &p.collection, p.id, p.embedding)
            .await?;
        Ok(result)
    }
}
