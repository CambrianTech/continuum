//! Hydration through the shared typed store and its native snapshot owner.
use crate::orm::binary::{BinaryDescriptorPageRequest, BinaryPageOutput, BinaryPageRequest};
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
use continuum_store::data::vector_index::{EmbeddingDescriptor, Producer};
use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

const PAGE: NonZeroUsize = NonZeroUsize::MIN.saturating_add(255);

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
    let mut embeddings = HashMap::new();
    let mut descriptor_cursor = None;
    loop {
        let descriptors = snapshot
            .descriptor_page(&BinaryDescriptorPageRequest {
                field: super::EMBEDDING_KEY,
                after: descriptor_cursor.as_ref(),
                limit: PAGE,
            })
            .await?;
        if descriptors.is_empty() {
            break;
        }
        for descriptor in &descriptors {
            let embedding = EmbeddingDescriptor::from_binary(descriptor).map_err(internal)?;
            let Producer::Embedder(producer) = embedding.producer() else {
                return Err(CommandError::Invalid(
                    "memory payload requires its embedder identity".into(),
                ));
            };
            let width = descriptor.byte_len().map_err(internal)?;
            let mut after = None;
            loop {
                let mut bytes = Vec::new();
                let mut ids = Vec::new();
                snapshot
                    .page_matching_into(
                        &BinaryPageRequest {
                            field: super::EMBEDDING_KEY,
                            after,
                            limit: PAGE,
                        },
                        descriptor,
                        BinaryPageOutput {
                            bytes: &mut bytes,
                            ids: &mut ids,
                        },
                    )
                    .await?;
                if ids.is_empty() {
                    break;
                }
                after = ids.last().copied();
                for (id, payload) in ids.into_iter().zip(bytes.chunks_exact(width)) {
                    // The consumer materializes its f32 working set once, after descriptor validation.
                    let values = bytemuck::pod_collect_to_vec::<u8, f32>(payload);
                    embeddings.insert(
                        id,
                        MemoryEmbedding {
                            values,
                            producer: producer.clone(),
                        },
                    );
                }
            }
        }
        descriptor_cursor = descriptors.last().cloned();
    }
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
