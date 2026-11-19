//! The named results of the `data/*` commands whose answer is one value. A command's result is a
//! named type on the wire (the registry refuses an inline primitive), so a client reads a field it
//! can name instead of a bare number or list.

use super::types::BatchOutcome;

/// `data/count`: how many rows match.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/RecordCount.ts")]
pub struct RecordCount {
    #[ts(type = "number")]
    pub count: usize,
}

/// `data/list-collections`: the store's collection names.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/CollectionNames.ts")]
pub struct CollectionNames {
    pub collections: Vec<String>,
}

/// `data/batch`: what each operation did, in order; the batch committed all of them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/BatchOutcomes.ts")]
pub struct BatchOutcomes {
    pub outcomes: Vec<BatchOutcome>,
}

/// `data/ensure-schema`: the collection whose declared schema the store now holds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/SchemaEnsured.ts")]
pub struct SchemaEnsured {
    pub collection: String,
}

/// `data/truncate`: how many rows the truncate removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/Truncated.ts")]
pub struct Truncated {
    #[ts(type = "number")]
    pub removed: usize,
}
