//! The chat store's Rust-authored ORM entities, registered by the chat module itself (never by a
//! sibling that would import chat and close a module loop).

/// Every collection [`register_chat_orm_entities`] registers.
pub const CHAT_ORM_COLLECTIONS: &[&str] = &[
    <super::arrival::Arrival as crate::orm::OrmEntity>::COLLECTION,
    <super::import_watermark::ImportWatermarkRow as crate::orm::OrmEntity>::COLLECTION,
    <super::publication::PublicationObligation as crate::orm::OrmEntity>::COLLECTION,
];

/// Register the chat store's entities (arrivals with their unique indexes; import watermarks).
/// Idempotent; a conflicting shape refuses boot.
pub fn register_chat_orm_entities(
    registry: &crate::orm::OrmEntityRegistry,
) -> Result<(), crate::orm::RegistrationError> {
    registry.register::<super::arrival::Arrival>()?;
    registry.register::<super::import_watermark::ImportWatermarkRow>()?;
    registry.register::<super::publication::PublicationObligation>()?;
    Ok(())
}
