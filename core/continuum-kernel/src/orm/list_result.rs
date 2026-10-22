//! The `data/list` result: one page of matching records and the true total. An ORM fact,
//! read by every caller that lists through the data layer (the mind's training ledgers, the
//! memory commands, the data module that answers), so it lives with the ORM.

use serde::{Deserialize, Serialize};

/// Result of `data/list` — the matching records + an accurate total.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataListResult.ts"
)]
pub struct DataListResult {
    /// The matching records (each carries id, collection, data, metadata).
    #[ts(type = "Array<unknown>")]
    pub items: Vec<serde_json::Value>,
    /// Total records matching the filter (SQL COUNT — independent of `limit`).
    pub total: u32,
}
