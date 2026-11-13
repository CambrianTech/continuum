//! The chat store's Rust-authored ORM entities, registered by the chat module itself (never by a
//! sibling that would import chat and close a module loop).

/// Every collection [`register_chat_orm_entities`] registers.
pub const CHAT_ORM_COLLECTIONS: &[&str] = &[
    <MessageRow as crate::orm::OrmEntity>::COLLECTION,
    <super::arrival::Arrival as crate::orm::OrmEntity>::COLLECTION,
    <super::import_watermark::ImportWatermarkRow as crate::orm::OrmEntity>::COLLECTION,
    <super::publication::PublicationObligation as crate::orm::OrmEntity>::COLLECTION,
];

/// Register the chat store's entities (arrivals with their unique indexes; import watermarks).
/// Idempotent; a conflicting shape refuses boot.
pub fn register_chat_orm_entities(
    registry: &crate::orm::OrmEntityRegistry,
) -> Result<(), crate::orm::RegistrationError> {
    registry.register::<MessageRow>()?;
    registry.register::<super::arrival::Arrival>()?;
    registry.register::<super::import_watermark::ImportWatermarkRow>()?;
    registry.register::<super::publication::PublicationObligation>()?;
    Ok(())
}

/// The persisted room line, declared by the same owner that writes it.
/// Display names and sender classes belong to identity lookup, not duplicated message columns.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_messages")]
#[entity(index(name = "idx_chat_messages_room_timestamp", fields = ["roomId", "timestamp", "id"]))]
#[entity(index(name = "idx_chat_messages_timestamp", fields = ["timestamp", "id"]))]
pub(super) struct MessageRow {
    #[entity(primary_key)]
    pub id: uuid::Uuid,
    pub room_id: uuid::Uuid,
    #[entity(indexed)]
    pub sender_id: uuid::Uuid,
    /// Canonical UTC milliseconds, the same ordered representation used by chat cursors.
    pub timestamp: String,
    #[entity(json)]
    pub content: serde_json::Value,
    pub reply_to_id: Option<uuid::Uuid>,
    #[entity(json)]
    pub metadata: serde_json::Value,
    pub status: String,
}