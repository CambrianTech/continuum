use crate::orm::{
    store::{NativeEntityWrite, OrmStore},
    SeaOrmAdapter, StorageAdapter,
};
use crate::{
    identity::PersonaRef,
    memory::{persistent::PersistentMemory, MemoryRecord},
};
use continuum_store::data::vector_index::{unit_f32, EmbedderId, Producer};
use std::{collections::BTreeMap, sync::Arc};

fn record(id: uuid::Uuid, persona: &PersonaRef) -> PersistentMemory {
    PersistentMemory::from_record(MemoryRecord {
        id: id.to_string(),
        persona_id: persona.as_str().to_owned(),
        memory_type: "observation".into(),
        content: "retain the reviewed correction".into(),
        context: serde_json::json!({"task": "native-restart"}),
        timestamp: "2026-11-14T00:00:00Z".into(),
        importance: 0.75,
        access_count: 0,
        tags: vec!["rust".into()],
        related_to: vec![],
        source: None,
        last_accessed_at: None,
        origin_node: None,
        origin_seq: Some(9_007_199_254_740_993),
        layer: None,
        relevance_score: None,
    })
}

#[tokio::test]
async fn native_memory_survives_reopen_with_exact_scalars_and_producer(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("memory.db");
    let persona = PersonaRef("native-restart-fixture".into());
    let id = uuid::Uuid::from_u128(1);
    let producer = EmbedderId::new("restart-fixture-3d");
    {
        let adapter: Arc<dyn StorageAdapter> = Arc::new(SeaOrmAdapter::open(&path).await?);
        let store = OrmStore::<PersistentMemory>::new(adapter).await?;
        let entity = record(id, &persona);
        let payload = unit_f32(
            [1.0, 0.0, 0.0].into_iter(),
            &Producer::Embedder(producer.clone()),
        )?;
        store
            .save_native(NativeEntityWrite {
                id,
                entity: &entity,
                payloads: BTreeMap::from([("embedding".into(), payload)]),
            })
            .await?;
        let without_vector = record(uuid::Uuid::from_u128(2), &persona);
        store
            .save(uuid::Uuid::from_u128(2), &without_vector)
            .await?;
        // A malformed native write must not leave a scalar row behind.
        let refused_id = uuid::Uuid::from_u128(3);
        let refused = record(refused_id, &persona);
        let payload = unit_f32(
            [1.0, 0.0, 0.0].into_iter(),
            &Producer::Embedder(producer.clone()),
        )?;
        assert!(store
            .save_native(NativeEntityWrite {
                id: refused_id,
                entity: &refused,
                payloads: BTreeMap::from([("undeclaredPayload".into(), payload)]),
            })
            .await
            .is_err());
        assert_eq!(store.count().await?, 2);
    }
    let reopened: Arc<dyn StorageAdapter> = Arc::new(SeaOrmAdapter::open(&path).await?);
    let loaded = super::persistence::load(reopened, &persona).await?;
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].record.id, id.to_string());
    assert_eq!(loaded[0].record.origin_seq, Some(9_007_199_254_740_993));
    assert_eq!(loaded[0].record.source, None);
    assert_eq!(
        loaded[0].record.context,
        serde_json::json!({"task": "native-restart"})
    );
    let embedding = loaded[0]
        .embedding
        .as_ref()
        .ok_or("missing native embedding")?;
    assert_eq!(embedding.values, [1.0, 0.0, 0.0]);
    assert_eq!(embedding.producer, producer);
    assert!(loaded[1].embedding.is_none());
    Ok(())
}

#[tokio::test]
async fn hydration_refuses_vectors_without_embedder_provenance(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("wrong-producer.db");
    let persona = PersonaRef("wrong-producer-fixture".into());
    let adapter: Arc<dyn StorageAdapter> = Arc::new(SeaOrmAdapter::open(&path).await?);
    let store = OrmStore::<PersistentMemory>::new(Arc::clone(&adapter)).await?;
    let id = uuid::Uuid::from_u128(1);
    let entity = record(id, &persona);
    let payload = unit_f32([1.0, 0.0, 0.0].into_iter(), &Producer::IndexCommand)?;
    store
        .save_native(NativeEntityWrite {
            id,
            entity: &entity,
            payloads: BTreeMap::from([("embedding".into(), payload)]),
        })
        .await?;
    assert!(matches!(
        super::persistence::load(adapter, &persona).await,
        Err(crate::sdk_codegen::CommandError::Invalid(_))
    ));
    Ok(())
}

#[tokio::test]
async fn shared_lessons_page_past_256_and_filter_before_submission(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("lessons.db");
    let persona = PersonaRef("lesson-recipient".into());
    let adapter: Arc<dyn StorageAdapter> = Arc::new(SeaOrmAdapter::open(&path).await?);
    let store = OrmStore::<PersistentMemory>::new(Arc::clone(&adapter)).await?;
    for number in 1..=520 {
        let id = uuid::Uuid::from_u128(number);
        let mut entity = record(id, &persona);
        entity.memory_type = "shared".into();
        if number == 1 {
            entity.persona_id = "other".into();
        }
        if number == 2 {
            entity.memory_type = "observation".into();
        }
        if number == 3 {
            entity.content = "  ".into();
        }
        if number == 4 {
            entity.timestamp = "2026-11-13T00:00:00Z".into();
        }
        store.save(id, &entity).await?;
    }
    drop(store);
    drop(adapter);
    let reopened: Arc<dyn StorageAdapter> = Arc::new(SeaOrmAdapter::open(&path).await?);
    let records = super::persistence::read_shared_lessons(
        &reopened,
        &super::SharedLessonSelection {
            persona: &persona,
            since_timestamp: Some("2026-11-13T00:00:00Z"),
        },
    )
    .await?;
    assert_eq!(records.len(), 516);
    assert_eq!(
        records.first().ok_or("no first lesson")?.id,
        uuid::Uuid::from_u128(5).to_string()
    );
    assert_eq!(
        records.last().ok_or("no last lesson")?.id,
        uuid::Uuid::from_u128(520).to_string()
    );
    assert!(records.iter().all(|record| record.source.is_none()));
    Ok(())
}
