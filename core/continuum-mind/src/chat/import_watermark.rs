//! How far the chat store has imported each room from airc: the attach watermark, a durable
//! acceptance identity (PERCEPTION-FROM-THE-STORE.md, Slice P). It lives in the store beside the
//! lines it vouches for, written through the ORM, keyed by the room.

use airc_core::RoomId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::chat_data_handle::{ChatModule, CHAT_DATA_HANDLE};
use crate::orm::types::{BatchOperation, BatchOperationType};

/// A position in a room's airc transcript (the daemon's cursor), typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportWatermark {
    pub epoch: u64,
    pub counter: u64,
    pub event_id: Uuid,
}

/// One room's import watermark row; its id IS the room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_import_watermarks")]
pub struct ImportWatermarkRow {
    #[entity(primary_key)]
    pub id: Uuid,
    pub epoch: u64,
    pub counter: u64,
    pub event_id: Uuid,
}

const COLLECTION: &str = "chat_import_watermarks";

impl ChatModule {
    /// The room's watermark, or `None` when the store has never imported it.
    pub(super) async fn import_watermark(
        &self,
        room: RoomId,
    ) -> Result<Option<ImportWatermark>, String> {
        let executor = self.executor()?;
        let params = json!({ "dbPath": CHAT_DATA_HANDLE, "collection": COLLECTION, "id": room.as_uuid().to_string() });
        let result = executor
            .execute_json("data/read", params)
            .await
            .map_err(|e| format!("import watermark: data/read failed: {e}"))?;
        if result.get("success").and_then(Value::as_bool) != Some(true) {
            return Ok(None); // no row: the store has never imported this room
        }
        let Some(data) = result.get("data").and_then(|d| d.get("data")).cloned() else {
            return Ok(None);
        };
        let row: ImportWatermarkRow = serde_json::from_value(data)
            .map_err(|e| format!("import watermark: malformed row: {e}"))?; // storage boundary: the adapter's row format
        Ok(Some(ImportWatermark {
            epoch: row.epoch,
            counter: row.counter,
            event_id: row.event_id,
        }))
    }

    /// Record the room's watermark (one batch: create the row if absent, then overwrite it).
    pub(super) async fn advance_import_watermark(
        &self,
        room: RoomId,
        mark: ImportWatermark,
    ) -> Result<(), String> {
        let executor = self.executor()?;
        let row = ImportWatermarkRow {
            id: room.as_uuid(),
            epoch: mark.epoch,
            counter: mark.counter,
            event_id: mark.event_id,
        };
        let data = serde_json::to_value(&row).map_err(|e| format!("import watermark: {e}"))?; // storage boundary: the adapter's row format
        let op = |operation_type| BatchOperation {
            operation_type,
            collection: COLLECTION.to_string(),
            id: Some(row.id.to_string()),
            data: Some(data.clone()),
        };
        let params = json!({ "dbPath": CHAT_DATA_HANDLE, "operations": [op(BatchOperationType::Ensure), op(BatchOperationType::Update)] });
        let result = executor
            .execute_json("data/batch", params)
            .await
            .map_err(|e| format!("import watermark: data/batch failed: {e}"))?;
        match result.get("success").and_then(Value::as_bool) {
            Some(true) => Ok(()),
            _ => Err(format!("import watermark: the store refused it: {result}")),
        }
    }
}
