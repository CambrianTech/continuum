#![cfg(test)]
use super::*;

    #[tokio::test]
    #[ignore = "Requires libonnxruntime.dylib — run with ORT_DYLIB_PATH set"]
    pub(super) async fn test_backfill_vectors() {
        let module = DataModule::new(no_substrate_entities);
        let (_tmp, db_path) = test_db_path("backfill");

        // Create schema with content and embedding fields
        let schema = CollectionSchema {
            collection: "test_backfill".to_string(),
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

        // Create records without embeddings
        for i in 0..5 {
            let _ = create_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_backfill",
                    "data": { "content": format!("Test content number {}", i) }
                }),
            )
            .await;
        }

        // Run backfill — VectorBackfillStats serializes at top level (the new
        // contract); counts live directly on the object, not under a `data` envelope.
        let backfill_result = backfill_via_state(
            &module,
            json!({
                "dbPath": &db_path,
                "collection": "test_backfill",
                "textField": "content",
                "batchSize": 10
            }),
        )
        .await;

        assert!(backfill_result.is_ok(), "Backfill should succeed");

        if let Ok(CommandResult::Json(stats)) = backfill_result {
            assert_eq!(stats["total"].as_u64().unwrap(), 5);
            assert_eq!(stats["processed"].as_u64().unwrap(), 5);
            assert_eq!(stats["failed"].as_u64().unwrap(), 0);
        }

        // Verify embeddings were added
        let stats_result = stats_via_state(
            &module,
            json!({
                "dbPath": &db_path,
                "collection": "test_backfill"
            }),
        )
        .await;

        assert!(stats_result.is_ok());
        if let Ok(CommandResult::Json(stats)) = stats_result {
            assert_eq!(stats["recordsWithVectors"].as_u64().unwrap(), 5);
            assert!(stats["vectorDimensions"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    pub(super) fn test_cosine_similarity() {
        // Test identical vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Identical vectors should have similarity 1.0"
        );

        // Test orthogonal vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 0.001,
            "Orthogonal vectors should have similarity 0.0"
        );

        // Test opposite vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 0.001,
            "Opposite vectors should have similarity -1.0"
        );

        // Test with 384-dimension vectors (typical embedding size)
        let a: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let b: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Identical 384-dim vectors should have similarity 1.0"
        );
    }

    // ====================================================================
    // HandleRef migration tests for data/query-open/next/close
    // ====================================================================
    //
    // The cursor surface migrated from a hand-rolled string queryId to
    // typed HandleRef minted via CommandResponse::with_handle. These
    // tests cover the migration's hard edges:
    //   - both wire shapes (envelope handle + legacy queryId) resolve
    //   - cross-module/cross-resource handles fail loud with named
    //     owner/type values, not silent misroutes
    //   - stale handles surface a typed "handle not found" error that
    //     names the cursor + suggests likely causes
    //   - the legacy field stays additive — old TS consumers see the
    //     same JSON shape they parse today, plus a new top-level
    //     `handle` field they can ignore
    /// Helper: stand up a fresh DataModule + a temp SQLite + the schema
    /// + N rows. Used by every cursor test below — keeps the cursor
    /// tests focused on the handle behavior, not on row setup.
    pub(super) async fn setup_paginated_for_handle_tests(
        suffix: &str,
        rows: usize,
    ) -> (DataModule, tempfile::TempDir, String) {
        let module = DataModule::new(no_substrate_entities);
        let (tmp, db_path) = test_db_path(suffix);

        let schema = CollectionSchema {
            collection: "test_handle_cursor".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        for i in 0..rows {
            let _ = create_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_handle_cursor",
                    "data": { "name": format!("Item {i}") }
                }),
            )
            .await;
        }
        (module, tmp, db_path)
    }

    /// Helper: open a cursor + return the response JSON so each test
    /// can read the new `handle` field and the legacy `data.queryId`
    /// without re-implementing the open call.
    pub(super) async fn open_cursor(module: &DataModule, db_path: &str, page_size: usize) -> Value {
        let result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_handle_cursor",
                    "pageSize": page_size,
                }),
            )
            .await
            .expect("query-open must succeed");
        let CommandResult::Json(v) = result else {
            panic!("query-open must return CommandResult::Json")
        };
        v
    }

    #[tokio::test]
    pub(super) async fn query_open_returns_handle_alongside_legacy_query_id() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_open", 3).await;
        let response = open_cursor(&module, &db_path, 10).await;

        // Legacy shape: nested data.queryId still present so existing
        // TS consumers keep parsing the same fields.
        let legacy_id = response["data"]["queryId"]
            .as_str()
            .expect("legacy queryId must remain in the response shape during migration window");

        // New shape: kernel-level handle minted at top level with the
        // canonical owner + type tag from the data module's
        // QUERY_CURSOR_TYPE_TAG / DATA_MODULE_OWNER constants.
        let handle = &response["handle"];
        assert!(handle.is_object(), "handle must be present: {response}");
        assert_eq!(handle["owner"], "data");
        assert_eq!(handle["type_tag"], "data::QueryCursor");
        assert!(
            handle["created_at_ms"].as_u64().is_some(),
            "handle must carry a creation timestamp"
        );

        // Identity invariant: the two surfaces MUST address the same
        // cursor. Otherwise a caller threading the handle and a
        // caller threading the queryId would see different state.
        let handle_id = handle["id"]
            .as_str()
            .expect("handle.id must be the canonical UUID string");
        assert_eq!(
            legacy_id, handle_id,
            "legacy queryId and handle.id must be the SAME UUID — otherwise dual-shape callers diverge"
        );
        // Both fields are real UUIDs.
        uuid::Uuid::parse_str(handle_id).expect("handle.id must parse as a UUID");
    }

    #[tokio::test]
    pub(super) async fn query_next_accepts_handle_in_envelope() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_next", 5).await;
        let open = open_cursor(&module, &db_path, 3).await;
        let handle = open["handle"].clone();

        // New canonical shape: thread the handle via the envelope.
        let next = module
            .handle_command("data/query-next", json!({ "handle": handle }))
            .await
            .expect("query-next via handle must succeed");
        let CommandResult::Json(v) = next else {
            panic!("expected Json result")
        };
        assert_eq!(
            v["data"]["items"].as_array().unwrap().len(),
            3,
            "first page must contain pageSize items"
        );
        assert_eq!(v["data"]["pageNumber"], 1);
        assert_eq!(v["data"]["hasMore"], true);
    }

    #[tokio::test]
    pub(super) async fn query_next_still_accepts_legacy_query_id_field() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_legacy", 5).await;
        let open = open_cursor(&module, &db_path, 3).await;
        let legacy_id = open["data"]["queryId"].as_str().unwrap().to_string();

        // Existing TS callsites send {"queryId": "..."} flat — that path
        // must keep working through the migration window.
        let next = module
            .handle_command("data/query-next", json!({ "queryId": legacy_id }))
            .await
            .expect("query-next via legacy queryId must succeed");
        let CommandResult::Json(v) = next else {
            panic!("expected Json result")
        };
        assert_eq!(v["data"]["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    pub(super) async fn query_next_rejects_handle_with_wrong_owner() {
        // KINK: a handle minted by another module reaching this
        // module's handler is a routing bug — fail loud with the
        // mis-owned value named, NOT a silent lookup miss that would
        // look like "stale handle".
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_wrong_owner", 1).await;
        let bogus_handle = json!({
            "owner": "chat",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::QueryCursor",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": bogus_handle }))
            .await
            .expect_err("handle with non-data owner must surface a typed error");
        assert!(
            err.contains("handle owner mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("\"chat\"") && err.contains("\"data\""),
            "error must name both the offender and the expected owner: {err}"
        );
    }

    #[tokio::test]
    pub(super) async fn query_next_rejects_handle_with_wrong_type_tag() {
        // KINK: even within the data module, multiple handle shapes
        // are possible in principle (a future data::Migration handle
        // alongside data::QueryCursor). Threading the wrong type tag
        // here must fail loud, not silently treat it as a cursor.
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_wrong_type", 1).await;
        let wrong_type = json!({
            "owner": "data",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::Migration",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": wrong_type }))
            .await
            .expect_err("wrong type_tag must surface a typed error");
        assert!(
            err.contains("handle type mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("data::Migration") && err.contains("data::QueryCursor"),
            "error must name both the offender and the expected type: {err}"
        );
    }

    #[tokio::test]
    pub(super) async fn query_next_rejects_when_neither_handle_nor_query_id_provided() {
        // No handle, no queryId. The TS resolver previously deserialized
        // an empty `{}` into a `QueryNextParams` with an empty string;
        // here, BOTH fields are optional so the empty case is reachable.
        // It must surface a typed error rather than silently 404 with
        // an empty-string lookup.
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_neither", 1).await;
        let err = module
            .handle_command("data/query-next", json!({}))
            .await
            .expect_err("empty params must surface a typed error");
        assert!(
            err.contains("neither `handle`") && err.contains("nor `queryId`"),
            "error must name both supported shapes: {err}"
        );
    }

    #[tokio::test]
    pub(super) async fn query_next_with_unknown_handle_returns_handle_not_found() {
        // Stale-handle path: a well-formed handle whose id was never
        // (or no longer) in the DashMap. Must surface a typed error
        // that names the cursor + suggests likely causes (TTL eviction,
        // already-closed, prior process instance).
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_unknown", 1).await;
        let stale_handle = json!({
            "owner": "data",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::QueryCursor",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": stale_handle }))
            .await
            .expect_err("stale handle must surface a typed error");
        assert!(
            err.contains("handle not found"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("query-close") || err.contains("evicted"),
            "error must hint at likely causes so the caller can self-diagnose: {err}"
        );
    }

    #[tokio::test]
    pub(super) async fn query_close_accepts_handle_in_envelope() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_close", 1).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let handle = open["handle"].clone();

        let close = module
            .handle_command("data/query-close", json!({ "handle": handle }))
            .await
            .expect("close via handle must succeed");
        let CommandResult::Json(v) = close else {
            panic!("expected Json result")
        };
        assert_eq!(v["success"], true);

        // Subsequent next on the SAME handle must now fail loud — the
        // close actually freed the state, not just acked.
        let stale_handle = open["handle"].clone();
        let err = module
            .handle_command("data/query-next", json!({ "handle": stale_handle }))
            .await
            .expect_err("after-close lookup must fail loud");
        assert!(
            err.contains("handle not found"),
            "close + reuse must surface stale-handle error: {err}"
        );
    }

    #[tokio::test]
    pub(super) async fn query_close_still_accepts_legacy_query_id_field() {
        let (module, _tmp, db_path) =
            setup_paginated_for_handle_tests("handle_close_legacy", 1).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let legacy_id = open["data"]["queryId"].as_str().unwrap().to_string();

        let close = module
            .handle_command("data/query-close", json!({ "queryId": legacy_id }))
            .await
            .expect("legacy close must succeed");
        let CommandResult::Json(v) = close else {
            panic!("expected Json result")
        };
        assert_eq!(v["success"], true);
    }

    #[tokio::test]
    pub(super) async fn full_round_trip_open_next_close_via_handles_only() {
        // End-to-end through the new canonical shape ONLY (no legacy
        // queryId reads). 12 rows, page size 5: page 1 → 5 items,
        // page 2 → 5 items, page 3 → 2 items + hasMore=false. The
        // handle stays valid across the entire cursor lifetime.
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("round_trip", 12).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let handle = open["handle"].clone();

        // ── page 1 ───────────────────────────────────────────────────
        let p1 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 1 must succeed");
        let CommandResult::Json(p1) = p1 else {
            panic!("expected Json")
        };
        assert_eq!(p1["data"]["items"].as_array().unwrap().len(), 5);
        assert_eq!(p1["data"]["pageNumber"], 1);
        assert_eq!(p1["data"]["hasMore"], true);

        // ── page 2 ───────────────────────────────────────────────────
        let p2 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 2 must succeed");
        let CommandResult::Json(p2) = p2 else {
            panic!("expected Json")
        };
        assert_eq!(p2["data"]["items"].as_array().unwrap().len(), 5);
        assert_eq!(p2["data"]["pageNumber"], 2);
        assert_eq!(p2["data"]["hasMore"], true);

        // ── page 3: partial + terminal ───────────────────────────────
        let p3 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 3 must succeed");
        let CommandResult::Json(p3) = p3 else {
            panic!("expected Json")
        };
        assert_eq!(p3["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(p3["data"]["pageNumber"], 3);
        assert_eq!(p3["data"]["hasMore"], false);

        // ── close ────────────────────────────────────────────────────
        let close = module
            .handle_command("data/query-close", json!({ "handle": handle }))
            .await
            .expect("close must succeed");
        let CommandResult::Json(close) = close else {
            panic!("expected Json")
        };
        assert_eq!(close["success"], true);
    }
