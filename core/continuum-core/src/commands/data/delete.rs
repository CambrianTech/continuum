//! `data/delete` — delete a record by id.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::{StorageResult, UUID};

/// Params for `data/delete`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataDeleteParams.ts"
)]
pub struct DataDeleteParams {
    /// The collection holding the record.
    pub collection: String,
    /// The record id to delete.
    pub id: UUID,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// a specific store. Accepts the legacy `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Delete a record by id. Publishes `<collection>:deleted` on success. This is
    /// destructive and irreversible — marked `Privileged` here (the highest the
    /// descriptor enum carries); the grid trust policy further restricts
    /// `data/delete` to Owner nodes by name (`modules/grid/acl.rs`,
    /// `routing/grid_trust_policy.rs`), so remote peers cannot invoke it.
    pub struct DataDelete { state: Arc<DataState> }
    name: "data/delete",
    access: Privileged,
    params: DataDeleteParams,
    output: StorageResult<bool>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.delete_record(handle, p.collection, p.id).await?;
        Ok(result)
    }
}
