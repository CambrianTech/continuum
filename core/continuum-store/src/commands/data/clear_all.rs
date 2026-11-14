//! `data/clear-all` — wipe every collection in a store.

use std::sync::Arc;

use crate::data::DataState;
use crate::orm::adapter::ClearAllResult;

/// Params for `data/clear-all`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataClearAllParams.ts"
)]
pub struct DataClearAllParams {
    /// Storage handle. Defaults to "main" (the shared DB).
    // `dbPath` on the wire, NOT `handle`: the envelope owns that name and refuses a
    // string for it (`expected struct HandleRef`) — card ea28d2f6. The Rust field keeps
    // its name; only the wire changes.
    #[serde(default, rename = "dbPath", skip_serializing_if = "crate::data::StoreHandle::is_main")]
    #[ts(optional, as = "Option<crate::data::StoreHandle>")]
    pub handle: crate::data::StoreHandle,
}

crate::action_command! {
    /// Wipe every collection in a store, reporting the tables cleared and the
    /// record count deleted. The most destructive data op, so gated `Privileged`
    /// — the named grid trust policy restricts `data/clear-all` to the Owner over
    /// the wire.
    pub struct DataClearAll { state: Arc<DataState> }
    name: "data/clear-all",
    access: Privileged,
    params: DataClearAllParams,
    output: ClearAllResult,
    run(this, _ctx, p) => {
        let handle = p.handle.as_str();
        let result = this.state.clear_all_collections(handle).await?;
        Ok(result)
    }
}
