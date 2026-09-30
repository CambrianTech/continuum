//! `adapter/info` — the storage adapter's identity + full capability surface.

use std::sync::Arc;

use crate::modules::data::{AdapterInfo, DataState};

/// Params for `adapter/info`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/adapter/AdapterInfoParams.ts"
)]
pub struct AdapterInfoParams {
    /// Storage handle. Defaults to "main" (the shared DB).
    // `dbPath` on the wire, NOT `handle`: the envelope owns that name and refuses a
    // string for it (`expected struct HandleRef`) — card ea28d2f6. The Rust field keeps
    // its name; only the wire changes.
    #[serde(default, rename = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// The storage adapter's identity (name, handle) and full capability surface
    /// (transactions, joins, indexing, full-text + vector search, batch, max
    /// record size). Subsumes the old `adapter/capabilities` — capabilities are a
    /// field on this result. Gated `AiSafe`.
    pub struct AdapterInfoCommand { state: Arc<DataState> }
    name: "adapter/info",
    access: AiSafe,
    params: AdapterInfoParams,
    output: AdapterInfo,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.adapter_info(handle).await?;
        Ok(result)
    }
}
