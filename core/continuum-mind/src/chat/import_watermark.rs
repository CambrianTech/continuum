//! How far the chat store has imported each room from airc: the attach watermark, a durable
//! acceptance identity (PERCEPTION-FROM-THE-STORE.md, Slice P). It lives in the store beside the
//! lines it vouches for, written through the ORM, keyed by the room, with a typed history saying
//! what it vouches for before it.

use airc_core::RoomId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::chat_data_handle::ChatModule;
use crate::orm::types::{BatchOperation, BatchOperationType};

/// A position in a room's airc transcript (the daemon's cursor), typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportWatermark {
    pub epoch: u64,
    pub counter: u64,
    pub event_id: Uuid,
}

/// Whether every line before the watermark is in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "history")]
pub enum ImportHistory {
    /// Every line up to the watermark was imported through the awaited seam.
    Complete,
    /// The watermark was migrated from the pre-store cursor file, which advanced independently of
    /// the old lossy writer: lines at or before this position may be missing from the store (Codex
    /// on #5694). A reader of that history perceives a gap; it is never vouched for.
    UnvouchedThrough {
        epoch: u64,
        counter: u64,
        event_id: Uuid,
    },
}

/// How far the store has imported a room, and what it vouches for before that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportedThrough {
    pub position: ImportWatermark,
    pub history: ImportHistory,
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
    pub history: ImportHistory,
}

const COLLECTION: &str = "chat_import_watermarks";

impl ChatModule {
    /// The room's watermark and history, or `None` when the store has never imported it.
    pub(super) async fn import_watermark(
        &self,
        room: RoomId,
    ) -> Result<Option<ImportedThrough>, String> {
        let Some(row) = self
            .row::<ImportWatermarkRow>(COLLECTION, room.as_uuid())
            .await?
        else {
            return Ok(None); // the store confirms no row: it has never imported this room
        };
        Ok(Some(ImportedThrough {
            position: ImportWatermark {
                epoch: row.epoch,
                counter: row.counter,
                event_id: row.event_id,
            },
            history: row.history,
        }))
    }

    /// Advance the room's watermark (a position already delivered and imported). A new row's
    /// history is complete; an existing row keeps its history, so a migrated room stays unvouched
    /// before its migration point however far it advances.
    pub(super) async fn advance_import_watermark(
        &self,
        room: RoomId,
        mark: ImportWatermark,
    ) -> Result<(), String> {
        let full = row_value(room, mark, ImportHistory::Complete)?;
        let position =
            json!({ "epoch": mark.epoch, "counter": mark.counter, "eventId": mark.event_id });
        self.write_watermark(
            room,
            vec![
                (BatchOperationType::Ensure, full),
                (BatchOperationType::Update, position),
            ],
        )
        .await
    }

    /// Record a watermark migrated from the pre-store cursor file: unvouched through that position.
    pub(super) async fn migrate_import_watermark(
        &self,
        room: RoomId,
        mark: ImportWatermark,
    ) -> Result<(), String> {
        let history = ImportHistory::UnvouchedThrough {
            epoch: mark.epoch,
            counter: mark.counter,
            event_id: mark.event_id,
        };
        self.write_watermark(
            room,
            vec![(BatchOperationType::Ensure, row_value(room, mark, history)?)],
        )
        .await
    }

    async fn write_watermark(
        &self,
        room: RoomId,
        writes: Vec<(BatchOperationType, Value)>,
    ) -> Result<(), String> {
        let operations: Vec<BatchOperation> = writes
            .into_iter()
            .map(|(operation_type, data)| BatchOperation {
                operation_type,
                collection: COLLECTION.to_string(),
                id: Some(room.as_uuid().to_string()),
                data: Some(data),
            })
            .collect();
        self.run_batch(operations).await.map(drop)
    }
}

fn row_value(room: RoomId, mark: ImportWatermark, history: ImportHistory) -> Result<Value, String> {
    let row = ImportWatermarkRow {
        id: room.as_uuid(),
        epoch: mark.epoch,
        counter: mark.counter,
        event_id: mark.event_id,
        history,
    };
    serde_json::to_value(&row).map_err(|e| format!("import watermark: {e}")) // storage boundary: the adapter's row format
}
