//! `data/create` — create a record in a collection.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{DataRecord, StorageResult, UUID};

/// Params for `data/create`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataCreateParams.ts"
)]
pub struct DataCreateParams {
    /// The collection to write to.
    pub collection: String,
    /// Optional record id. A v4 UUID is minted when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<UUID>,
    /// The record body (collection-specific shape).
    #[ts(type = "unknown")]
    pub data: serde_json::Value,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// a specific store. Accepts the legacy `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Create a record in a collection (a v4 id is minted when none is given).
    /// Publishes `<collection>:created` on success. Writing shared state is a
    /// trusted-citizen action — gated `Privileged`.
    pub struct DataCreate { state: Arc<DataState> }
    name: "data/create",
    access: Privileged,
    params: DataCreateParams,
    output: StorageResult<DataRecord>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this
            .state
            .create_record(handle, p.collection, p.id, p.data)
            .await?;
        Ok(result)
    }
}
