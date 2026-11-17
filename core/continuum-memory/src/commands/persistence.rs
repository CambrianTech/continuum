//! Hydration through the shared typed store and its native snapshot owner.
use crate::orm::binary::{BinarySelection, BinaryDescriptor, BinaryValue, ByteOrder, ElementType};
use crate::orm::{
    query::{QueryBuilder, QueryOperator},
    store::OrmStore,
};
use crate::orm::{OrmEntity, StorageAdapter};
use crate::{
    identity::PersonaRef,
    memory::{persistent::PersistentMemory, types::MemoryEmbedding, CorpusMemory},
    sdk_codegen::CommandError,
};
use continuum_store::data::vector_index::{EmbedderId, Producer};
use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

const PAGE: NonZeroUsize = NonZeroUsize::MIN.saturating_add(255);

/// A durable lesson selection. Absence of a watermark means the first traversal.
pub struct SharedLessonSelection<'a> {
    pub persona: &'a PersonaRef,
    pub since_timestamp: Option<&'a str>,
}

/// Read scalar lessons from one pinned view without fetching their native embeddings.
/// Complete the read before returning any lessons to a training submitter.
pub async fn shared_lessons(
    state: &super::MemoryState,
    selection: &SharedLessonSelection<'_>,
) -> Result<Vec<crate::memory::MemoryRecord>, CommandError> {
    let adapter = state
        .storage()
        .map_err(internal)?
        .adapter(&super::persona_db_handle(selection.persona))
        .await
        .map_err(internal)?;
    read_shared_lessons(&adapter, selection).await
}

pub(super) async fn read_shared_lessons(
    adapter: &Arc<dyn StorageAdapter>,
    selection: &SharedLessonSelection<'_>,
) -> Result<Vec<crate::memory::MemoryRecord>, CommandError> {
    let store = OrmStore::<PersistentMemory>::new(adapter.clone())
        .await
        .map_err(internal)?;
    let snapshot = adapter
        .binary_snapshot(PersistentMemory::COLLECTION)
        .await?;
    let mut after = None;
    let mut records = Vec::new();
    loop {
        let mut query = QueryBuilder::new(PersistentMemory::COLLECTION)
            .filter_eq("personaId", selection.persona.as_str())
            .filter_eq("memoryType", "shared")
            .sort_asc("id")
            .limit(PAGE.get());
        if let Some(timestamp) = selection.since_timestamp {
            query = query.filter("timestamp", QueryOperator::Gt(timestamp.into()));
        }
        if let Some(id) = after {
            query = query.filter("id", QueryOperator::Gt(serde_json::Value::String(id)));
        }
        let page = store
            .find_matching_in(&snapshot, &query.build())
            .await
            .map_err(internal)?;
        if page.is_empty() {
            break;
        }
        after = page.last().map(|(id, _)| id.to_string());
        records.extend(page.into_iter().map(|(_, entity)| entity.into_record()));
    }
    records.retain(|record| !record.content.trim().is_empty());
    records.sort_unstable_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)));
    Ok(records)
}

pub(super) async fn load(
    adapter: Arc<dyn StorageAdapter>,
    persona: &PersonaRef,
) -> Result<Vec<CorpusMemory>, CommandError> {
    let store = OrmStore::<PersistentMemory>::new(adapter.clone())
        .await
        .map_err(internal)?;
    let snapshot = adapter
        .binary_snapshot(PersistentMemory::COLLECTION)
        .await?;
    let mut after = None;
    let mut memories = Vec::new();
    loop {
        let mut query = QueryBuilder::new(PersistentMemory::COLLECTION)
            .filter_eq("personaId", persona.as_str())
            .sort_asc("id")
            .limit(PAGE.get());
        if let Some(id) = after {
            query = query.filter("id", QueryOperator::Gt(serde_json::Value::String(id)));
        }
        let page = store
            .find_matching_in(&snapshot, &query.build())
            .await
            .map_err(internal)?;
        if page.is_empty() {
            break;
        }
        after = page.last().map(|(id, _)| id.to_string());
        let ids: Vec<_> = page.iter().map(|(id, _)| *id).collect();
        let mut embeddings = HashMap::new();
        for (id, payload) in snapshot.read_selected(&BinarySelection {
            field: super::EMBEDDING_KEY,
            ids: &ids,
        }).await? {
            let producer = memory_producer(payload.descriptor())?;
            embeddings.insert(id, MemoryEmbedding {
                values: bytemuck::pod_collect_to_vec::<u8, f32>(payload.bytes()),
                producer,
            });
        }
        for (id, entity) in page {
            memories.push(CorpusMemory {
                record: entity.into_record(),
                embedding: embeddings.remove(&id),
            });
        }
    }
    Ok(memories)
}

fn internal(error: impl std::fmt::Display) -> CommandError {
    CommandError::Internal(error.to_string())
}

const NATIVE_ORDER: ByteOrder = if cfg!(target_endian = "little") {
    ByteOrder::Little
} else {
    ByteOrder::Big
};

/// Memory preserves the embedder's buffer; normalization belongs to its producer.
pub(super) fn native_embedding(embedding: &MemoryEmbedding) -> Result<BinaryValue, CommandError> {
    let descriptor = BinaryDescriptor {
        element_type: ElementType::F32,
        byte_order: NATIVE_ORDER,
        shape: vec![embedding.values.len() as u64],
        producer: embedding.producer.as_str().to_owned(),
        unit: false,
        scale: None,
    };
    memory_producer(&descriptor)?;
    BinaryValue::new(descriptor, bytemuck::cast_slice::<f32, u8>(&embedding.values).to_vec())
        .map_err(internal)
}

fn memory_producer(descriptor: &BinaryDescriptor) -> Result<EmbedderId, CommandError> {
    if descriptor.element_type != ElementType::F32
        || descriptor.byte_order != NATIVE_ORDER
        || !matches!(descriptor.shape.as_slice(), [dimension] if *dimension > 0)
        || descriptor.scale.is_some()
    {
        return Err(CommandError::Invalid("memory embedding requires a nonempty native f32 vector".into()));
    }
    match Producer::from_identity(&descriptor.producer) {
        Producer::Embedder(producer) => Ok(producer),
        _ => Err(CommandError::Invalid("memory payload requires its embedder identity".into())),
    }
}
