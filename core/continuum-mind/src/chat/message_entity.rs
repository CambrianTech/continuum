//! A chat line's row in `chat_messages`, declared by the core that writes it (one owner, beside
//! `line_store::message_row`). Before this the declaration was the web desktop's TS `ChatMessageEntity`
//! (entity_schemas.json), which required `senderName`, `senderType` and `priority`: fields the core never
//! writes ("WHO spoke is `senderId`"), so every node's store read as Breaking against it (data/schema-plan,
//! the M5 and the IntelMac, 2026-11-28). The row is what the core writes; nothing else.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use airc_core::{PeerId, RoomId};

/// One room line as the chat store holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_messages")]
// a room's page reads newest first (27-29 ms per page without it on the M5's 103,319 rows, under 1 ms with it)
#[entity(index(name = "idx_chat_messages_room_timestamp", fields = ["roomId", "timestamp"]))]
pub struct ChatMessageRow {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(indexed)]
    pub room_id: RoomId,
    /// Who spoke: every citizen alike, human or persona.
    #[entity(indexed)]
    pub sender_id: PeerId,
    /// When the line occurred, ISO 8601 (`chat_data_handle::now_iso`).
    #[entity(indexed)]
    pub timestamp: String,
    /// `{ text, media }`, as the line was posted.
    pub content: Value,
    pub reply_to_id: Option<Uuid>,
    pub metadata: Option<MessageMetadata>,
    /// A String column, as every node holds it (`sent`), never a JSON one.
    #[entity(string)]
    pub status: ChatStatus,
}

/// Where a line came from: `user` for every citizen alike, human or persona.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetadata {
    pub source: MessageSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageSource {
    User,
}

/// A line's delivery state. The core writes only `sent`; reads never decode this column (`StoredLine`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatStatus {
    Sent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::types::{BatchOperation, BatchOperationType};
    use crate::orm::OrmEntity;

    // what this catches (data/schema-plan on the M5 and the IntelMac, 2026-11-28: chat_messages read
    // Breaking): a declaration that requires what the core never writes. A store grown row by row from the
    // core's own rows, as every node's was, opens against this declaration with no required column missing.
    #[tokio::test]
    async fn a_store_grown_from_the_cores_rows_holds_this_declaration() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (id, room, sender) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let row = ChatMessageRow {
            id,
            room_id: RoomId::from_uuid(room),
            sender_id: PeerId::from_uuid(sender),
            timestamp: "2026-11-28T05:00:00.000Z".into(),
            content: serde_json::json!({ "text": "hello", "media": [] }),
            reply_to_id: Some(Uuid::new_v4()),
            metadata: Some(MessageMetadata { source: MessageSource::User }),
            status: ChatStatus::Sent,
        };
        let data = serde_json::to_value(&row).expect("test: a row serializes"); // expect: the test's own row
        // the bytes every node already holds (Fable on #6305): ids as bare UUID text, the status a plain word
        assert_eq!(data["roomId"], serde_json::json!(room.to_string()));
        assert_eq!(data["senderId"], serde_json::json!(sender.to_string()));
        assert_eq!(data["status"], serde_json::json!("sent"));
        assert_eq!(data["metadata"], serde_json::json!({ "source": "user" }));
        let declared = ChatMessageRow::collection_schema();
        let status = declared.fields.iter().find(|field| field.name == "status").expect("test: status is declared"); // expect: the test's own entity
        assert_eq!(status.field_type, crate::orm::FieldType::String, "a String column, as stored, never JSON");
        let written = adapter
            .batch(vec![BatchOperation { operation_type: BatchOperationType::Create, collection: ChatMessageRow::COLLECTION.into(), id: Some(id.to_string()), data: Some(data) }])
            .await;
        assert!(written.success, "test: the core's row writes: {:?}", written.error);
        // the declaration this replaces, the web desktop's TS entity: the same store reads Breaking against it
        let Ok(Some(ts)) = continuum_kernel::entity_schemas::resolve(ChatMessageRow::COLLECTION) else { panic!("test: the TS declaration still ships") };
        assert!(adapter.ensure_schema(continuum_kernel::entity_schemas::to_collection_schema(ts)).await.success, "test: a breaking store is reported, never refused");
        let before = adapter.schema_report();
        assert!(before.iter().any(|plan| plan.collection == ChatMessageRow::COLLECTION && plan.standing() == crate::orm::sqlite::Standing::Breaking), "the TS declaration reads this store Breaking: {before:?}");
        assert!(adapter.ensure_schema(ChatMessageRow::collection_schema()).await.success, "the store opens against the core's declaration");
        let report = adapter.schema_report();
        let Some(chat) = report.iter().find(|plan| plan.collection == ChatMessageRow::COLLECTION) else { panic!("test: chat_messages planned, got {report:?}") };
        assert_ne!(chat.standing(), crate::orm::sqlite::Standing::Breaking, "{:?}", chat.items);
        assert!(
            !chat.items.iter().any(|item| matches!(item, crate::orm::sqlite::PlanItem::AddColumn { required: true, .. })),
            "nothing the core never writes is required: {:?}",
            chat.items
        );
    }
}
