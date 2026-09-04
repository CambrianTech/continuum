//! `data/list-collections` — list the collection names present in a store.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::StorageResult;

/// Params for `data/list-collections`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataListCollectionsParams.ts"
)]
pub struct DataListCollectionsParams {
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// List the collection names present in a store — the catalog a persona reads
    /// to discover what it can query. Gated `AiSafe`.
    pub struct DataListCollections { state: Arc<DataState> }
    name: "data/list-collections",
    access: AiSafe,
    params: DataListCollectionsParams,
    output: StorageResult<Vec<String>>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.list_collection_names(handle).await?;
        Ok(result)
    }
}
