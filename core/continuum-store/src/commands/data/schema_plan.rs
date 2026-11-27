//! `data/schema-plan`: each declared collection of a store against the table it holds, as the store
//! stood when it was opened for that collection (ONE-FRAME-ONE-MEMORY.md, "The durable store", step 0a).

use std::sync::Arc;

use crate::data::DataState;
use crate::orm::sqlite::CollectionPlan;

/// Params for `data/schema-plan`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/DataSchemaPlanParams.ts")]
pub struct DataSchemaPlanParams {
    /// Storage handle. Defaults to "main" (the shared DB).
    // `dbPath` on the wire, NOT `handle`: the envelope owns that name (card ea28d2f6).
    #[serde(default, rename = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Each collection a store was opened for, planned against its declaration: what it would take to
    /// match (a table, columns, indexes), what it holds that no declaration names, and any column whose
    /// type affinity differs. Read-only: it reports, nothing is applied. Gated `AiSafe`.
    pub struct DataSchemaPlan { state: Arc<DataState> }
    name: "data/schema-plan",
    access: AiSafe,
    params: DataSchemaPlanParams,
    output: Vec<CollectionPlan>,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main"); // unwrap_or: no dbPath means the shared store, as every data/ command reads it
        let adapter = this.state.get_adapter(handle).await?;
        Ok(adapter.schema_report())
    }
}
