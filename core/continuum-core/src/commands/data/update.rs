//! `data/update` — update a record by id.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{DataRecord, StorageResult, UUID};

/// Params for `data/update`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataUpdateParams.ts"
)]
pub struct DataUpdateParams {
    /// The collection holding the record.
    pub collection: String,
    /// The record id to update.
    pub id: UUID,
    /// The new record body (collection-specific shape).
    #[ts(type = "unknown")]
    pub data: serde_json::Value,
    /// Bump the record's version on write (optimistic-concurrency aware stores).
    #[serde(default)]
    pub increment_version: bool,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// a specific store. Accepts the legacy `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Update a record by id with a new body. Publishes `<collection>:updated` on
    /// success. Overwriting shared state — gated `Privileged`.
    pub struct DataUpdate { state: Arc<DataState> }
    name: "data/update",
    access: Privileged,
    params: DataUpdateParams,
    output: StorageResult<DataRecord>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this
            .state
            .update_record(handle, p.collection, p.id, p.data, p.increment_version)
            .await?;
        Ok(result)
    }
}
