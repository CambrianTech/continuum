//! `vector/backfill` — generate and store embeddings for existing records that
//! lack them.

use std::collections::HashMap;
use std::sync::Arc;

use crate::modules::data::{DataState, VectorBackfillStats};
use crate::orm::query::FieldFilter;

fn default_batch_size() -> usize {
    100
}

/// Params for `vector/backfill`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorBackfillParams.ts"
)]
pub struct VectorBackfillParams {
    /// The collection to backfill embeddings for.
    pub collection: String,
    /// The record field whose text gets embedded.
    pub text_field: String,
    /// How many records to embed per batch. Defaults to 100.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Optional embedding model override. Defaults to the configured embedder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Optional record filter (same shape as `data/list` filters). Restricts which
    /// records get backfilled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "Record<string, unknown>")]
    pub filter: Option<serde_json::Map<String, serde_json::Value>>,
    /// Storage handle. Defaults to "main" (the shared DB). Accepts the legacy
    /// `dbPath` field name as an alias.
    #[serde(default, alias = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

crate::action_command! {
    /// Generate embeddings for every record in a collection that lacks one (or
    /// matches `filter`), embedding the `textField` text via the configured
    /// embedder, and store each on its record. Returns counts of processed /
    /// skipped / failed records plus elapsed time. Writes many records and spends
    /// inference compute, so gated `Privileged`.
    pub struct VectorBackfill { state: Arc<DataState> }
    name: "vector/backfill",
    access: Privileged,
    params: VectorBackfillParams,
    output: VectorBackfillStats,
    run(this, _ctx, p) => {
        let handle = p.handle.as_deref().unwrap_or("main");
        // Parse the wire filter map into typed FieldFilters (untagged: operator or value).
        let filter: Option<HashMap<String, FieldFilter>> = match p.filter {
            Some(map) => Some(
                serde_json::from_value(serde_json::Value::Object(map))
                    .map_err(|e| format!("invalid filter: {e}"))?,
            ),
            None => None,
        };
        let result = this
            .state
            .backfill_vectors(handle, &p.collection, &p.text_field, p.batch_size, p.model, filter)
            .await?;
        Ok(result)
    }
}
