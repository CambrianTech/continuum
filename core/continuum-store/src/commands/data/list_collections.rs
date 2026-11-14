//! `data/list-collections` — list the collection names present in a store.

use std::sync::Arc;

use crate::data::DataState;

/// Params for `data/list-collections`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataListCollectionsParams.ts"
)]
pub struct DataListCollectionsParams {
    /// Storage handle. Defaults to "main" (the shared DB).
    // `dbPath` on the wire, NOT `handle`: the envelope owns that name and refuses a
    // string for it (`expected struct HandleRef`) — card ea28d2f6. The Rust field keeps
    // its name; only the wire changes.
    #[serde(default, rename = "dbPath", skip_serializing_if = "crate::data::StoreHandle::is_main")]
    #[ts(optional, as = "Option<crate::data::StoreHandle>")]
    pub handle: crate::data::StoreHandle,
}

crate::action_command! {
    /// List the collection names present in a store — the catalog a persona reads
    /// to discover what it can query. Gated `AiSafe`.
    pub struct DataListCollections { state: Arc<DataState> }
    name: "data/list-collections",
    access: AiSafe,
    params: DataListCollectionsParams,
    output: crate::orm::data_outputs::CollectionNames,
    run(this, _ctx, p) => {
        let handle = p.handle.as_str();
        let result = this.state.list_collection_names(handle).await?;
        Ok(crate::orm::data_outputs::CollectionNames { collections: result })
    }
}
