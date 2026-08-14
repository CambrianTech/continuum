//! `data/read` — read a single record by id (persona/UI-facing).

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{DataRecord, StorageResult, UUID};

/// Params for `data/read`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataReadParams.ts"
)]
pub struct DataReadParams {
    /// The collection to read from (e.g. "rooms", "users", "messages").
    pub collection: String,
    /// The record id.
    pub id: UUID,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// a specific store. Accepts the legacy `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Read a single record by its id from a collection. Returns the record (or a
    /// not-found result). Reading shared state is a read — gated `AiSafe`.
    pub struct DataRead { state: Arc<DataState> }
    name: "data/read",
    access: AiSafe,
    params: DataReadParams,
    output: StorageResult<DataRecord>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.read_record(handle, &p.collection, &p.id).await?;
        Ok(result)
    }
}
