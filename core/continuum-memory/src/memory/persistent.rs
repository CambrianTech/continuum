//! The durable memory declaration. Recall-only scores and layers are not stored.
use crate::orm::{BaseEntity, Entity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "memories")]
#[entity(index(name = "memory_persona_kind_time", fields = ["personaId", "memoryType", "timestamp", "id"]))]
pub struct PersistentMemory {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub persona_id: String,
    pub memory_type: String,
    pub content: String,
    #[entity(json)]
    pub context: serde_json::Value,
    pub timestamp: String,
    pub importance: f64,
    pub access_count: u32,
    pub tags: Vec<String>,
    pub related_to: Vec<String>,
    pub source: Option<String>,
    pub last_accessed_at: Option<String>,
    pub origin_node: Option<String>,
    pub origin_seq: Option<u64>,
    /// Selected only through the owner's native payload interface, never the scalar row.
    #[entity(binary)]
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
}

impl PersistentMemory {
    /// Admission owns the record; field allocations move into the durable declaration.
    pub fn from_record(record: super::types::MemoryRecord) -> Self {
        let admitted_at = chrono::Utc::now().to_rfc3339();
        Self {
            base: BaseEntity {
                id: record.id,
                created_at: admitted_at.clone(),
                updated_at: admitted_at,
                version: 1,
            },
            persona_id: record.persona_id,
            memory_type: record.memory_type,
            content: record.content,
            context: record.context,
            timestamp: record.timestamp,
            importance: record.importance,
            access_count: record.access_count,
            tags: record.tags,
            related_to: record.related_to,
            source: record.source,
            last_accessed_at: record.last_accessed_at,
            origin_node: record.origin_node,
            origin_seq: record.origin_seq,
            embedding: None,
        }
    }

    /// Recall annotations start absent; they are computed by the consumer, not persisted.
    pub fn into_record(self) -> super::types::MemoryRecord {
        super::types::MemoryRecord {
            id: self.base.id,
            persona_id: self.persona_id,
            memory_type: self.memory_type,
            content: self.content,
            context: self.context,
            timestamp: self.timestamp,
            importance: self.importance,
            access_count: self.access_count,
            tags: self.tags,
            related_to: self.related_to,
            source: self.source,
            last_accessed_at: self.last_accessed_at,
            origin_node: self.origin_node,
            origin_seq: self.origin_seq,
            layer: None,
            relevance_score: None,
        }
    }
}
