#![cfg(test)]
//! Vector indexing, ranking and cache behavior on the shared SQLite fixture. Embeddings are
//! native unit-vector payloads written through `vector/index`, never JSON numbers.
use super::*;
use crate::data::vector_index::{RecordId, ResidentMatrix, UnitVectorError, VectorStoreError};

/// A record id the native store accepts: canonical UUIDs, ordered by `n`.
fn record_id(n: u128) -> RecordId {
    RecordId::from_u128(n + 1)
}

/// A vector collection: optional text and an optional native embedding.
async fn declare_vectors(adapter: &dyn StorageAdapter, collection: &str) {
    let mut fields = crate::orm::base_entity_fields();
    for (name, field_type) in [("content", crate::orm::FieldType::String), ("embedding", crate::orm::FieldType::Binary)] {
        fields.push(crate::orm::SchemaField {
            name: name.into(), field_type, default_value: None,
            nullable: true, indexed: false, unique: false, max_length: None, foreign_key: None,
        });
    }
    adapter.ensure_schema(CollectionSchema { collection: collection.into(), fields, indexes: vec![] })
        .await.into_result().expect("test: declared vector schema");
}

async fn create_row(adapter: &dyn StorageAdapter, collection: &str, id: &str) {
    adapter.create(DataRecord {
        id: id.into(), collection: collection.into(),
        data: json!({"content": format!("document {id}")}),
        metadata: crate::orm::types::RecordMetadata::default(),
    }).await.into_result().expect("test: row");
}

// what this catches: a page of rows without vectors must not end corpus traversal.
#[tokio::test]
async fn vector_corpus_pages_past_rows_without_embeddings() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_pages");
    let adapter = module.state.get_adapter(&db_path).await.expect("test: adapter");
    declare_vectors(adapter.as_ref(), "test_vector_pages").await;
    let page_size = crate::data::max_concurrent_queries::default_page_size();
    for index in 0..page_size + 3 {
        let id = record_id(index as u128).to_string();
        create_row(adapter.as_ref(), "test_vector_pages", &id).await;
        if index >= page_size {
            module.state.index_vector(&db_path, "test_vector_pages", id, vec![index as f64, 1.0])
                .await.expect("test: index");
        }
    }
    let vectors = module.state.vector_corpus(&db_path, "test_vector_pages").await.expect("test: corpus");
    assert_eq!(vectors.len(), 3, "an unindexed page must not end corpus traversal");
    for offset in 0..3 {
        assert_eq!(vectors.id(offset), Some(record_id((page_size + offset) as u128)));
    }
}

#[tokio::test]
async fn backfill_rejects_zero_batch_before_opening_an_embedder() {
    let module = DataModule::new(no_substrate_entities);
    let result = module.state.backfill_vectors(
        "main", "test_vectors", "content", 0, None, None,
    ).await;
    assert!(matches!(result, Err(ref error) if error == "vector/backfill: batch_size must be positive"));
}

// what this catches: a zero vector from the wire is refused, never stored as NaN.
#[tokio::test]
async fn zero_vector_is_refused_at_index() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_zero");
    let adapter = module.state.get_adapter(&db_path).await.expect("test: adapter");
    declare_vectors(adapter.as_ref(), "test_zero").await;
    let zero = record_id(0).to_string();
    create_row(adapter.as_ref(), "test_zero", &zero).await;
    let refused = module.state.index_vector(&db_path, "test_zero", zero, vec![0.0; 384]).await;
    assert!(matches!(refused, Err(VectorStoreError::Unit(UnitVectorError::Zero))), "{refused:?}");
    assert!(module.state.vector_corpus(&db_path, "test_zero").await.expect("test: corpus").len() == 0);
}

#[tokio::test]
async fn test_vector_index_and_stats() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_index");
    let adapter = module.state.get_adapter(&db_path).await.expect("test: adapter");
    declare_vectors(adapter.as_ref(), "test_vectors").await;

    let create_result = create_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_vectors",
            "data": { "content": "Hello world" }
        }),
    )
    .await;
    let record_id = match &create_result {
        Ok(CommandResult::Json(result)) => result["data"]["id"].as_str().expect("test: created id").to_string(),
        other => panic!("create failed: {other:?}"),
    };

    // A nullable embedding means this document has not been indexed yet.
    let before = module.state.vector_stats(&db_path, "test_vectors").await.expect("test: stats");
    assert_eq!(before.records_with_vectors, 0);
    assert!(module.state.vector_corpus(&db_path, "test_vectors").await.expect("test: corpus").len() == 0);

    let test_embedding: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
    let index_result = index_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_vectors",
            "id": record_id,
            "embedding": test_embedding
        }),
    )
    .await;
    match &index_result {
        Ok(CommandResult::Json(result)) => assert!(result["success"].as_bool().unwrap_or(false), "{result}"), // test: absent success is a failure
        other => panic!("index failed: {other:?}"),
    }

    let stats_result = stats_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_vectors"
        }),
    )
    .await;
    match stats_result {
        Ok(CommandResult::Json(stats)) => {
            assert_eq!(stats["collection"], "test_vectors");
            assert_eq!(stats["totalRecords"], 1);
            assert_eq!(stats["recordsWithVectors"], 1);
            assert_eq!(stats["vectorDimensions"], 384);
        }
        other => panic!("stats failed: {other:?}"),
    }
}

#[tokio::test]
async fn test_vector_search_basic() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_search");
    let adapter = module.state.get_adapter(&db_path).await.expect("test: adapter");
    declare_vectors(adapter.as_ref(), "test_search").await;

    let embeddings: Vec<Vec<f64>> = vec![
        (0..384).map(|i| (i as f64) * 0.001).collect(),
        // Identical vectors exercise ties at the top-k boundary.
        (0..384).map(|i| (i as f64) * 0.001).collect(),
        (0..384).map(|i| (i as f64) * 0.003).collect(),
    ];
    for (idx, emb) in embeddings.iter().enumerate() {
        let id = record_id(idx as u128).to_string();
        create_row(adapter.as_ref(), "test_search", &id).await;
        module.state.index_vector(&db_path, "test_search", id, emb.clone()).await.expect("test: index");
    }

    let query_vector: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
    let search_result = search_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_search",
            "queryVector": query_vector,
            "k": 3,
            "threshold": 0.0,
            "includeData": true
        }),
    )
    .await;
    let result = match search_result {
        Ok(CommandResult::Json(result)) => result,
        other => panic!("search failed: {other:?}"),
    };
    let results = result["results"].as_array().expect("test: results array");
    assert_eq!(results.len(), 3);
    let first_score = results[0]["score"].as_f64().expect("test: score");
    assert!(first_score > 0.9, "Expected high similarity, got {first_score}");
    assert_eq!(result["count"], results.len());
    // Hydration returns the record's scalar fields; the native payload stays in storage.
    assert!(results[0]["data"].get("embedding").is_none(), "{}", results[0]);
    // Partial selection must equal the complete ranking at every k boundary.
    for k in [0, 1, 2, 3, 99] {
        let selected = module
            .state
            .vector_search(&db_path, "test_search", query_vector.clone(), k, 0.0, false)
            .await
            .expect("rank cached corpus");
        assert_eq!(selected.count, k.min(results.len()));
        assert_eq!(selected.corpus_size, results.len());
        for (hit, expected) in selected.results.iter().zip(results) {
            assert_eq!(Some(hit.id.as_str()), expected["id"].as_str());
            assert_eq!(Some(hit.score), expected["score"].as_f64());
        }
    }
    let filtered = module
        .state
        .vector_search(&db_path, "test_search", query_vector.clone(), 2, 2.0, false)
        .await
        .expect("filter cached corpus");
    assert_eq!(filtered.count, 0);

    // A cached hit can disappear before hydration. Surface the read failure,
    // rather than return partial results with a contradictory count.
    let id = results[0]["id"].as_str().expect("vector hit must carry its record id");
    assert!(adapter.delete("test_search", &id.to_owned()).await.success);
    let hydrated = module
        .state
        .vector_search(&db_path, "test_search", query_vector, 3, 0.0, true)
        .await;
    assert!(
        matches!(hydrated, Err(VectorStoreError::Hydration { id: missing, .. }) if missing.to_string() == id),
        "missing cached record must reach the caller: {hydrated:?}"
    );
}

#[tokio::test]
async fn test_vector_cache_invalidation() {
    let module = DataModule::new(no_substrate_entities);
    let home = tempfile::tempdir().expect("test: home");
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    let db_path = "@node".to_string();
    let alias = module.state.resolve_handle(&db_path).expect("resolve isolated node store");
    assert_ne!(db_path, alias);
    let adapter = module.state.get_adapter(&db_path).await.expect("test: adapter");
    declare_vectors(adapter.as_ref(), "test_cache").await;
    let resident = record_id(0).to_string();
    let target = record_id(1).to_string();
    create_row(adapter.as_ref(), "test_cache", &resident).await;
    module.state.index_vector(&db_path, "test_cache", resident, vec![1.0; 384]).await.expect("test: index");

    // First search populates the resident matrix.
    let query: Vec<f64> = vec![1.0; 384];
    let _ = search_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache",
            "queryVector": query,
            "k": 1
        }),
    )
    .await;
    assert!(module.state.vector_stats(&db_path, "test_cache").await.expect("test: stats").cached_vectors > 0);

    let invalidate_result = invalidate_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache"
        }),
    )
    .await;
    match invalidate_result {
        Ok(CommandResult::Json(result)) => assert!(result["cacheInvalidated"].as_bool().unwrap_or(false), "{result}"), // test: absent flag is a failure
        other => panic!("invalidate failed: {other:?}"),
    }
    assert_eq!(module.state.vector_stats(&db_path, "test_cache").await.expect("test: stats").cached_vectors, 0);

    // Each acknowledged mutation must retire the warmed matrix before the next search.
    for operation in ["delete", "batch", "truncate", "clear"] {
        module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.0, true).await.expect("warm before create");
        let created = module.state.create_record(
            &alias, "test_cache".into(), Some(target.clone()), json!({}),
        ).await.expect("create through mutation owner");
        assert!(created.success, "{created:?}");
        module.state.index_vector(&alias, "test_cache", target.clone(), vec![1.0; 384]).await.expect("alias index");
        let warm = module.state.vector_search(
            &db_path, "test_cache", vec![1.0; 384], 10, 0.0, true,
        ).await.expect("warm corpus");
        assert!(warm.results.iter().any(|hit| hit.id == target));
        let alias_stats = module.state.vector_stats(&alias, "test_cache").await.expect("alias stats");
        assert_eq!(alias_stats.cached_vectors, warm.corpus_size);
        assert!(module.state.invalidate_vector_cache(&alias, "test_cache").await.expect("alias invalidation").cache_invalidated);
        assert_eq!(module.state.vector_stats(&db_path, "test_cache").await.expect("sentinel stats").cached_vectors, 0);
        module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.0, true).await.expect("warm before reindex");
        module.state.index_vector(&alias, "test_cache", target.clone(), vec![-1.0; 384]).await.expect("reindex");
        let changed = module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.5, true)
            .await.expect("search reindexed corpus");
        assert!(changed.results.iter().all(|hit| hit.id != target));
        match operation {
            "batch" => {
                let result = module.state.batch_operations(&alias, vec![crate::orm::types::BatchOperation {
                    operation_type: crate::orm::types::BatchOperationType::Delete,
                    collection: "test_cache".into(), id: Some(target.clone()), data: None,
                }]).await.expect("batch delete");
                assert!(result.success, "{result:?}");
            }
            "delete" => assert!(module.state.delete_record(&alias, "test_cache".into(), target.clone()).await.expect("delete").success),
            "truncate" => assert!(module.state.truncate_collection(&alias, "test_cache").await.expect("truncate").success),
            "clear" => assert!(module.state.clear_all_collections(&alias).await.expect("clear").success),
            _ => unreachable!("fixture operation"),
        }
        let after = module.state.vector_search(
            &db_path, "test_cache", vec![1.0; 384], 10, 0.0, true,
        ).await.expect("search after acknowledged mutation");
        assert!(after.results.iter().all(|hit| hit.id != target));
        let expected = if matches!(operation, "truncate" | "clear") { 0 } else { warm.corpus_size - 1 };
        assert_eq!(after.corpus_size, expected, "{operation} must refresh the corpus");
    }
}
