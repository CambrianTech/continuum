//! `data/truncate` — delete every record in one collection (keep the schema).

use std::sync::Arc;

use crate::data::DataState;

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
    #[serde(default, rename = "dbPath", skip_serializing_if = "crate::data::StoreHandle::is_main")]
    #[ts(optional, as = "Option<crate::data::StoreHandle>")]
    pub handle: crate::data::StoreHandle,
}

crate::action_command! {
    /// Delete every record in one collection (the schema stays). Destructive, so
    /// gated `Privileged` — the named grid trust policy restricts `data/truncate`
    /// to the Owner over the wire.
    pub struct DataTruncate { state: Arc<DataState> }
    name: "data/truncate",
    access: Privileged,
    params: DataTruncateParams,
    output: crate::orm::data_outputs::Truncated,
    run(this, _ctx, p) => {
        let handle = p.handle.as_str();
        let result = this.state.truncate_collection(handle, &p.collection).await?;
        Ok(crate::orm::data_outputs::Truncated { removed: result })
    }
}
