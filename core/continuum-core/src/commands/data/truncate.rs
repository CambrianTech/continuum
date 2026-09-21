//! `data/truncate` — delete every record in one collection (keep the schema).

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::StorageResult;

/// Params for `data/truncate`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataTruncateParams.ts"
)]
pub struct DataTruncateParams {
    /// The collection to empty.
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
    /// Delete every record in one collection (the schema stays). Destructive, so
    /// gated `Privileged` — the named grid trust policy restricts `data/truncate`
    /// to the Owner over the wire.
    pub struct DataTruncate { state: Arc<DataState> }
    name: "data/truncate",
    access: Privileged,
    params: DataTruncateParams,
    output: StorageResult<bool>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.truncate_collection(handle, &p.collection).await?;
        Ok(result)
    }
}
