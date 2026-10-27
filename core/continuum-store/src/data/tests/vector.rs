#![cfg(test)]
//! Vector indexing, ranking and cache behavior on the shared SQLite fixture.
use super::*;

#[tokio::test]
async fn test_vector_index_and_stats() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_index");

    // Create schema with embedding field
    let schema = CollectionSchema {
        collection: "test_vectors".to_string(),
        fields: vec![
            crate::orm::types::SchemaField {
                name: "content".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
            crate::orm::types::SchemaField {
                name: "embedding".to_string(),
                field_type: crate::orm::types::FieldType::Json,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
        ],
        indexes: vec![],
    };

    // Tests bypass the IPC surface (which now requires a REGISTERED
    // entity collection per Phase 2 Step 3) — call the adapter directly
    // with a synthetic test CollectionSchema.
    let adapter = module.state.get_adapter(&db_path).await.unwrap();
    let _ = adapter.ensure_schema(schema).await;

    // Create a record
    let create_result = create_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_vectors",
            "data": { "content": "Hello world" }
        }),
    )
    .await;

    assert!(
        create_result.is_ok(),
        "create_result failed: {:?}",
        create_result
    );
    let record_id = if let Ok(CommandResult::Json(result)) = &create_result {
        result["data"]["id"].as_str().unwrap().to_string()
    } else {
        panic!("Create failed");
    };

    // Index a vector for this record
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

    assert!(index_result.is_ok());
    if let Ok(CommandResult::Json(result)) = &index_result {
        assert!(result["success"].as_bool().unwrap_or(false));
    }

    // Get vector stats — the typed VectorStats serializes at top level (no
    // `data` envelope), the new contract the `vector/stats` command returns.
    let stats_result = stats_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_vectors"
        }),
    )
    .await;

    assert!(stats_result.is_ok());
    if let Ok(CommandResult::Json(stats)) = stats_result {
        assert_eq!(stats["collection"], "test_vectors");
        assert_eq!(stats["totalRecords"], 1);
        assert_eq!(stats["recordsWithVectors"], 1);
        assert_eq!(stats["vectorDimensions"], 384);
    }
}

#[tokio::test]
async fn test_vector_search_basic() {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, db_path) = test_db_path("vector_search");

    // Create schema
    let schema = CollectionSchema {
        collection: "test_search".to_string(),
        fields: vec![
            crate::orm::types::SchemaField {
                name: "content".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
            crate::orm::types::SchemaField {
                name: "embedding".to_string(),
                field_type: crate::orm::types::FieldType::Json,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
        ],
        indexes: vec![],
    };

    // Tests bypass the IPC surface (which now requires a REGISTERED
    // entity collection per Phase 2 Step 3) — call the adapter directly
    // with a synthetic test CollectionSchema.
    let adapter = module.state.get_adapter(&db_path).await.unwrap();
    let _ = adapter.ensure_schema(schema).await;

    // Create records with embeddings
    let embeddings: Vec<Vec<f64>> = vec![
        (0..384).map(|i| (i as f64) * 0.001).collect(),
        // Identical vectors exercise ties at the top-k boundary.
        (0..384).map(|i| (i as f64) * 0.001).collect(),
        (0..384).map(|i| (i as f64) * 0.003).collect(),
    ];

    for (idx, emb) in embeddings.iter().enumerate() {
        let _ = create_via_state(
            &module,
            json!({
                "dbPath": &db_path,
                "collection": "test_search",
                "data": {
                    "content": format!("Document {}", idx),
                    "embedding": emb
                }
            }),
        )
        .await;
    }

    // Search for similar vectors
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

    assert!(search_result.is_ok());
    if let Ok(CommandResult::Json(result)) = search_result {
        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        // First result should be most similar (score close to 1.0)
        let first_score = results[0]["score"].as_f64().unwrap();
        assert!(
            first_score > 0.9,
            "Expected high similarity, got {}",
            first_score
        );
        assert_eq!(result["count"], results.len());
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
        let Some(id) = results[0]["id"].as_str() else {
            panic!("vector hit must carry its record id");
        };
        assert!(adapter.delete("test_search", &id.to_owned()).await.success);
        let hydrated = module
            .state
            .vector_search(&db_path, "test_search", query_vector, 3, 0.0, true)
            .await;
        assert!(
            matches!(hydrated, Err(ref error) if error.contains(id)),
            "missing cached record must reach the caller: {hydrated:?}"
        );
    }
}

#[tokio::test]
async fn test_vector_cache_invalidation() {
    let module = DataModule::new(no_substrate_entities);
    let home = tempfile::tempdir().unwrap();
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    let db_path = "@node".to_string();
    let alias = module.state.resolve_handle(&db_path).expect("resolve isolated node store");
    assert_ne!(db_path, alias);

    // Create schema
    let schema = CollectionSchema {
        collection: "test_cache".to_string(),
        fields: vec![crate::orm::types::SchemaField {
            name: "embedding".to_string(),
            field_type: crate::orm::types::FieldType::Json,
            indexed: false,
            unique: false,
            nullable: true,
            max_length: None,
            foreign_key: None,
        }],
        indexes: vec![],
    };

    // Tests bypass the IPC surface (which now requires a REGISTERED
    // entity collection per Phase 2 Step 3) — call the adapter directly
    // with a synthetic test CollectionSchema.
    let adapter = module.state.get_adapter(&db_path).await.unwrap();
    let _ = adapter.ensure_schema(schema).await;

    // Create a record with embedding
    let _ = create_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache",
            "data": {
                "embedding": vec![1.0; 384]
            }
        }),
    )
    .await;

    // First search populates cache
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

    // Verify cache has vectors via stats (top-level VectorStats, new contract)
    let stats_result = stats_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache"
        }),
    )
    .await;

    if let Ok(CommandResult::Json(stats)) = &stats_result {
        assert!(stats["cachedVectors"].as_u64().unwrap() > 0);
    }

    // Invalidate cache — VectorCacheInvalidation carries `cacheInvalidated`
    // (and the collection), the typed contract `vector/invalidate-cache` returns.
    let invalidate_result = invalidate_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache"
        }),
    )
    .await;

    assert!(invalidate_result.is_ok());
    if let Ok(CommandResult::Json(result)) = invalidate_result {
        assert!(result["cacheInvalidated"].as_bool().unwrap_or(false));
    }

    // Verify cache is empty
    let stats_after = stats_via_state(
        &module,
        json!({
            "dbPath": &db_path,
            "collection": "test_cache"
        }),
    )
    .await;

    if let Ok(CommandResult::Json(stats)) = stats_after {
        assert_eq!(stats["cachedVectors"].as_u64().unwrap(), 0);
    }
    // Each acknowledged mutation must retire the warmed corpus before the next search.
    for operation in ["delete", "batch", "truncate", "clear"] {
        module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.0, true).await.expect("warm before create");
        let created = module.state.create_record(
            &alias, "test_cache".into(), Some("mutation-target".into()),
            json!({"embedding": vec![1.0; 384]}),
        ).await.expect("create through mutation owner");
        assert!(created.success, "{created:?}");
        let warm = module.state.vector_search(
            &db_path, "test_cache", vec![1.0; 384], 10, 0.0, true,
        ).await.expect("warm corpus");
        assert!(warm.results.iter().any(|hit| hit.id == "mutation-target"));
        let alias_stats = module.state.vector_stats(&alias, "test_cache").await.expect("alias stats");
        assert_eq!(alias_stats.cached_vectors, warm.corpus_size);
        assert!(module.state.invalidate_vector_cache(&alias, "test_cache").await.expect("alias invalidation").cache_invalidated);
        assert_eq!(module.state.vector_stats(&db_path, "test_cache").await.expect("sentinel stats").cached_vectors, 0);
        module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.0, true).await.expect("warm before index");
        assert!(module.state.index_vector(&alias, "test_cache", "mutation-target".into(), vec![1.0; 384]).await.expect("alias index").success);
        assert_eq!(module.state.vector_stats(&db_path, "test_cache").await.expect("indexed stats").cached_vectors, 0);
        module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.0, true).await.expect("warm before update");
        let updated = module.state.update_record(&alias, "test_cache".into(), "mutation-target".into(),
            json!({"embedding": vec![-1.0; 384]}), false).await.expect("update embedding");
        assert!(updated.success);
        let changed = module.state.vector_search(&db_path, "test_cache", vec![1.0; 384], 10, 0.5, true)
            .await.expect("search updated corpus");
        assert!(changed.results.iter().all(|hit| hit.id != "mutation-target"));
        match operation {
            "batch" => {
                let result = module.state.batch_operations(&alias, vec![crate::orm::types::BatchOperation {
                    operation_type: crate::orm::types::BatchOperationType::Delete,
                    collection: "test_cache".into(), id: Some("mutation-target".into()), data: None,
                }]).await.expect("batch delete");
                assert!(result.success, "{result:?}");
            }
            "delete" => assert!(module.state.delete_record(&alias, "test_cache".into(), "mutation-target".into()).await.expect("delete").success),
            "truncate" => assert!(module.state.truncate_collection(&alias, "test_cache").await.expect("truncate").success),
            "clear" => assert!(module.state.clear_all_collections(&alias).await.expect("clear").success),
            _ => unreachable!("fixture operation"),
        }
        let after = module.state.vector_search(
            &db_path, "test_cache", vec![1.0; 384], 10, 0.0, true,
        ).await.expect("search after acknowledged mutation");
        assert!(after.results.iter().all(|hit| hit.id != "mutation-target"));
        let expected = if matches!(operation, "truncate" | "clear") { 0 } else { warm.corpus_size - 1 };
        assert_eq!(after.corpus_size, expected, "{operation} must refresh the corpus");
    }

}
