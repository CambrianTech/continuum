//! `data/ensure-schema` — ensure a collection's schema exists.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::orm::types::StorageResult;

/// Params for `data/ensure-schema`.
///
/// Callers pass a collection NAME, not an inline schema — the wire never carries
/// SQL, fields, or indexes. Rust resolves the schema from the ORM registry (Rust
/// substrate entities) or `entity_schemas.json` (TS-decorator authored).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataEnsureSchemaParams.ts"
)]
pub struct DataEnsureSchemaParams {
    /// The collection whose schema should exist.
    pub collection: String,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Ensure a collection's schema exists, resolving the schema by collection
    /// name (the wire never carries SQL/fields/indexes). A structural mutation, so
    /// gated `Privileged`.
    pub struct DataEnsureSchema { state: Arc<DataState> }
    name: "data/ensure-schema",
    access: Privileged,
    params: DataEnsureSchemaParams,
    output: StorageResult<bool>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = this.state.ensure_collection_schema(handle, &p.collection).await?;
        Ok(result)
    }
}
