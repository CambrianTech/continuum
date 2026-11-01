//! The ONE write path of a room line into the chat store: the message row and its [`Arrival`],
//! committed together in one batch (PERCEPTION-FROM-THE-STORE.md, Slice P). Two writers use it:
//! a human `chat/send` (a new line) and the inbound airc seam (an import, idempotent by id, so a
//! replay after a crash commits without a second row or arrival).

use airc_core::{PeerId, RoomId};
use serde_json::{json, Value};
use uuid::Uuid;

use super::arrival::{Arrival, ArrivalKind};
use super::chat_data_handle::{now_iso, ChatModule, CHAT_DATA_HANDLE};
use super::import_watermark::{ImportHistory, ImportWatermark, ImportedThrough};
use super::types::CHAT_MESSAGES_COLLECTION;
use crate::orm::types::{BatchOperation, BatchOperationType};
use crate::runtime::late_bound::LateBound;

/// A room line, typed: the routing header and the text, never a JSON payload to fish fields from.
#[derive(Debug, Clone, PartialEq)]
pub struct PostedLine {
    pub message_id: Uuid,
    pub room: RoomId,
    pub author: PeerId,
    pub kind: ArrivalKind,
    pub direct_to: Option<PeerId>,
    pub occurred_at_ms: u64,
    pub text: String,
}

/// Where a line landed in arrival order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imported {
    /// This write created the line, at this arrival.
    Fresh { arrival_seq: u64 },
    /// The line was already stored; its existing arrival.
    Duplicate { arrival_seq: u64 },
}

/// Whether a line is new (its id was just minted) or imported (it may already be stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineWrite {
    New,
    Import,
}

/// The chat module the inbound seam imports through, installed at its initialize.
static IMPORTER: LateBound<ChatModule> = LateBound::new("chat::line_store::importer");

pub(super) fn install_importer(module: std::sync::Arc<ChatModule>) {
    IMPORTER.install(module);
}

/// Where the inbound seam stores the lines it receives. Injected into the seam, so production
/// writes the chat store and a test records what the seam would have written.
#[async_trait::async_trait]
pub trait LineImporter: Send + Sync {
    /// Store `line` (idempotent by its id); awaited, so the attach watermark never passes a line
    /// the store has not committed.
    async fn import(&self, line: PostedLine) -> Result<Imported, String>;

    /// How far the store has imported `room` from airc; `None` when it never has.
    async fn watermark(&self, room: RoomId) -> Result<Option<ImportedThrough>, String>;

    /// Record how far the store has imported `room` (a position already delivered and imported).
    async fn advance_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String>;

    /// Record a position migrated from the pre-store cursor file: unvouched through it.
    async fn migrate_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String>;
}

/// The chat store, as the inbound seam's importer: the installed chat module, resolved at each
/// import (before it is installed an import fails, and the seam re-attaches and replays).
pub struct ChatStore;

#[async_trait::async_trait]
impl LineImporter for ChatStore {
    async fn import(&self, line: PostedLine) -> Result<Imported, String> {
        installed()?
            .commit_line(&line, LineWrite::Import, None, json!({ "text": line.text }))
            .await
    }

    async fn watermark(&self, room: RoomId) -> Result<Option<ImportedThrough>, String> {
        installed()?.import_watermark(room).await
    }

    async fn advance_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String> {
        installed()?.advance_import_watermark(room, mark).await
    }

    async fn migrate_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String> {
        installed()?.migrate_import_watermark(room, mark).await
    }
}

fn installed() -> Result<std::sync::Arc<ChatModule>, String> {
    IMPORTER
        .cloned()
        .ok_or_else(|| "chat store: the chat module is not installed yet (early boot)".to_string())
}

/// Records every imported line and reports each as fresh, in order: the seam's test importer.
#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Default)]
pub struct RecordingImporter(
    pub std::sync::Mutex<Vec<PostedLine>>,
    pub std::sync::Mutex<std::collections::HashMap<RoomId, ImportedThrough>>,
);

#[cfg(any(test, feature = "test-fixtures"))]
#[async_trait::async_trait]
impl LineImporter for RecordingImporter {
    async fn import(&self, line: PostedLine) -> Result<Imported, String> {
        let mut lines = self.0.lock().unwrap_or_else(|p| p.into_inner()); // test fixture: a poisoned lock still holds the lines recorded
        lines.push(line);
        Ok(Imported::Fresh {
            arrival_seq: lines.len() as u64,
        })
    }

    async fn watermark(&self, room: RoomId) -> Result<Option<ImportedThrough>, String> {
        Ok(self
            .1
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&room)
            .copied()) // test fixture: a poisoned lock still holds the marks
    }

    async fn advance_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String> {
        let mut marks = self.1.lock().unwrap_or_else(|p| p.into_inner()); // test fixture: a poisoned lock still holds the marks
        let history = marks
            .get(&room)
            .map_or(ImportHistory::Complete, |m| m.history);
        marks.insert(
            room,
            ImportedThrough {
                position: mark,
                history,
            },
        );
        Ok(())
    }

    async fn migrate_watermark(&self, room: RoomId, mark: ImportWatermark) -> Result<(), String> {
        let history = ImportHistory::UnvouchedThrough {
            epoch: mark.epoch,
            counter: mark.counter,
            event_id: mark.event_id,
        };
        self.1.lock().unwrap_or_else(|p| p.into_inner()).insert(
            room,
            ImportedThrough {
                position: mark,
                history,
            },
        ); // test fixture: a poisoned lock still holds the marks
        Ok(())
    }
}

impl ChatModule {
    /// Commit `line`'s message row and its arrival in ONE batch.
    pub(super) async fn commit_line(
        &self,
        line: &PostedLine,
        write: LineWrite,
        reply_to: Option<Uuid>,
        content: Value,
    ) -> Result<Imported, String> {
        let executor = self.executor()?;
        // The same ChatMessageEntity shape for every writer; WHO spoke is `senderId`, and
        // `metadata.source: "user"` for all citizens alike.
        let message = json!({
            "id": line.message_id.to_string(),
            "roomId": line.room.as_uuid().to_string(),
            "senderId": line.author.as_uuid().to_string(),
            "timestamp": now_iso(line.occurred_at_ms),
            "content": content,
            "replyToId": reply_to.map(|id| id.to_string()),
            "metadata": { "source": "user" },
            "status": "sent",
        });
        let message_op = BatchOperation {
            operation_type: match write {
                LineWrite::New => BatchOperationType::Create,
                LineWrite::Import => BatchOperationType::Ensure,
            },
            collection: CHAT_MESSAGES_COLLECTION.to_string(),
            id: Some(line.message_id.to_string()),
            data: Some(message),
        };
        let arrival = Arrival::new(
            line.message_id,
            line.room,
            line.author,
            line.kind,
            line.direct_to,
        );
        let append_op = arrival.append_op().map_err(|e| format!("chat line: {e}"))?;
        let params = json!({ "dbPath": CHAT_DATA_HANDLE, "operations": [message_op, append_op] });
        let result = executor
            .execute_json("data/batch", params)
            .await
            .map_err(|e| format!("chat line: data/batch failed: {e}"))?;
        if result.get("success").and_then(Value::as_bool) != Some(true) {
            let detail = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("success=false without an error");
            return Err(format!("chat line: the store refused the line: {detail}"));
        }
        let appended = result
            .get("data")
            .and_then(|d| d.get(1))
            .ok_or("chat line: the batch reported no arrival")?;
        let arrival_seq = appended
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or("chat line: the arrival carried no sequence")?;
        match appended.get("fresh").and_then(Value::as_bool) {
            Some(true) => Ok(Imported::Fresh { arrival_seq }),
            Some(false) => Ok(Imported::Duplicate { arrival_seq }),
            // An append that does not say whether it created the arrival is a malformed reply, never a
            // duplicate: reading it as one would vouch for a line nobody confirmed (Codex on #5694).
            None => Err("chat line: the arrival did not say whether it was fresh".to_string()),
        }
    }
}

/// Refuses every import: the seam's test importer for a store that cannot write.
#[cfg(any(test, feature = "test-fixtures"))]
pub struct RefusingImporter;

#[cfg(any(test, feature = "test-fixtures"))]
#[async_trait::async_trait]
impl LineImporter for RefusingImporter {
    async fn import(&self, _line: PostedLine) -> Result<Imported, String> {
        Err("test: the store refused the line".to_string())
    }

    async fn watermark(&self, _room: RoomId) -> Result<Option<ImportedThrough>, String> {
        Err("test: the store cannot read".to_string())
    }

    async fn advance_watermark(&self, _room: RoomId, _mark: ImportWatermark) -> Result<(), String> {
        Err("test: the store refused the watermark".to_string())
    }

    async fn migrate_watermark(&self, _room: RoomId, _mark: ImportWatermark) -> Result<(), String> {
        Err("test: the store refused the watermark".to_string())
    }
}
