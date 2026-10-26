#![cfg(test)]

    use super::*;
    pub(crate) use crate::module::{no_substrate_entities, DataModule};
    pub(crate) use crate::runtime::ServiceModule;
    use crate::orm::types::{BatchOperation, RecordMetadata};
use crate::orm::types::CollectionSchema;
    mod vector;

    // what this catches: a stale postgres:// DATABASE_URL must fail loudly, never fall back to SQLite.
    #[test]
    pub(super) fn postgres_connection_strings_are_rejected() {
        for url in ["postgres://localhost/continuum", "postgresql://localhost/continuum"] {
            assert!(crate::data::adapters::reject_unsupported_backend(url).unwrap_err().contains("no longer supported"));
        }
        assert!(crate::data::adapters::reject_unsupported_backend("/tmp/x.db").is_ok());
    }

    #[tokio::test]
    pub(super) async fn node_local_catalog_cannot_follow_a_remote_main_database() {
        let node = tempfile::tempdir().unwrap();
        // CONTINUUM_HOME precedes the process test home. Hold the existing shared
        // guard through adapter I/O so another fixture cannot remove its home
        // between path resolution and SQLite open (5067 hosted regression).
        let _home = crate::paths::test_home::HomeGuard::set(node.path()).await;
        let remote = "/shared/fleet/main.db";
        let backend = || DataState::resolve_handle_with("main", |key| {
            assert_eq!(key, "DATABASE_URL");
            Some(remote.to_string())
        }, || panic!("remote backend does not need a home"));
        assert_eq!(DataState::resolve_node_or_backend("main", || panic!("main keeps backend scope"), backend).unwrap(), remote);
        assert_eq!(DataState::resolve_node_or_backend("@node", || Ok(node.path().into()), || panic!("node catalog must never resolve DATABASE_URL")).unwrap(), node.path().join("database/node.db").to_str().unwrap());
        // Broker initialization may precede DataModule::initialize. The actual
        // adapter owner must open independently and reuse its resolved backend.
        // The held home guard supplies isolation through both adapter awaits.
        let state = DataState::new();
        assert_eq!(state.resolve_handle("@node").unwrap(),
            node.path().join(".continuum/database/node.db").to_str().unwrap());
        assert!(state.context.read().unwrap().is_none());
        let first = state.get_adapter("@node").await.unwrap();
        let second = state.get_adapter("@node").await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
    }

    // What this catches: a substrate-entity registration conflict booting a data module anyway.
    // Codex on #5472: the registrar seam must be exercised through `initialize`, not by calling
    // a successful registrar directly. The refusal must reach the runtime's init result, naming
    // the module, and no module context may be published for a refused store.
    #[tokio::test]
    async fn a_failing_substrate_registrar_refuses_initialize_and_publishes_no_context() {
        fn conflicting() -> Result<usize, String> {
            Err("staged_credit registered twice with two shapes".to_string())
        }
        let module = Arc::new(DataModule::new(conflicting));
        let runtime = crate::runtime::runtime::Runtime::new();
        runtime.register(module.clone());

        let refused = runtime.initialize().await.expect_err("a registrar conflict must refuse init"); // expect: the invariant under test
        assert!(refused.contains("data") && refused.contains("staged_credit registered twice"), "{refused}");
        assert!(
            module.state.context.read().unwrap_or_else(|e| e.into_inner()).is_none(),
            "a refused data module must not publish its context"
        );
    }

    // What this catches: generic data/vector commands cannot bypass the owning
    // recall command's authorization or mutate its retained decisions.
    #[tokio::test]
    pub(super) async fn recall_capture_rejects_generic_access_paths() {
        use crate::orm::OrmEntity;
        let module = DataModule::new(no_substrate_entities);
        let private = continuum_genome::genome::recall_impl::RecallDecision::COLLECTION;
        let aliases = [private.to_string(), crate::orm::adapter::naming::to_collection_name(private)];
        let commands = module.commands();
        for collection in aliases {
            for name in ["data/create", "data/read", "data/update", "data/delete",
                "data/list", "data/count", "data/ensure-schema", "data/collection-stats",
                "data/truncate", "vector/search", "vector/index", "vector/stats",
                "vector/invalidate-cache", "vector/backfill"] {
                let command = commands.iter().find(|c| c.name() == name).expect(name);
                let error = command.invoke(json!({
                    "collection": collection, "dbPath": "invalid-handle",
                    "id": "record", "data": {}, "embedding": [1.0],
                    "queryVector": [1.0], "textField": "need"
                }), None).await.expect_err(name);
                assert!(error.contains("protected collection"), "{name}: {error}");
            }
            for name in ["data/query", "data/query-open", "data/queryWithJoin"] {
                let error = module.state.dispatch(name, json!({
                    "collection": collection, "dbPath": "invalid-handle"
                })).await.expect_err(name);
                assert!(error.contains("protected collection"), "{name}: {error}");
            }
            let error = module.state.handle_query_with_join(serde_json::from_value(json!({
                "collection": "public", "dbPath": "invalid-handle",
                "joins": [{"collection": collection, "alias": "decision",
                    "localField": "id", "foreignField": "id", "type": "left"}]
            })).unwrap()).await.expect_err("private join");
            assert!(error.contains("protected collection"));
            for operation_type in [crate::orm::types::BatchOperationType::Create,
                crate::orm::types::BatchOperationType::Read,
                crate::orm::types::BatchOperationType::Update,
                crate::orm::types::BatchOperationType::Delete] {
                let error = module.state.batch_operations("invalid-handle", vec![BatchOperation {
                    operation_type, collection: collection.clone(), id: Some("record".into()), data: Some(json!({}))
                }]).await.expect_err("private batch");
                assert!(error.contains("protected collection"));
            }
        }
    }

    // What this catches: SQL-expression escapes cannot read private decisions;
    // ordinary queries and explicitly owner-gated clearing retain their behavior.
    #[tokio::test]
    pub(super) async fn recall_capture_blocks_query_escapes_without_breaking_owner_clear() {
        use crate::orm::OrmEntity;
        let module = DataModule::new(no_substrate_entities);
        let (_tmp, path) = test_db_path("protected-recall");
        let private = continuum_genome::genome::recall_impl::RecallDecision::COLLECTION;
        let adapter = module.state.get_adapter(&path).await.unwrap();
        let stored = adapter.create(DataRecord {
            id: "private-record".into(), collection: private.into(),
            data: json!({"need": "private need"}), metadata: RecordMetadata::default(),
        }).await;
        assert!(stored.success, "{:?}", stored.error);
        assert!(module.state.create_record(&path, "public".into(), Some("public-record".into()), json!({"value": 1})).await.unwrap().success);
        assert!(module.state.read_record(&path, "public", &"public-record".into()).await.unwrap().success);
        // These previously interpolated SQL expressions could bypass a plain
        // protected-collection equality check while naming a public collection.
        for attack in [
            json!({"collection": "public", "select": [format!("(select need from {private} limit 1) as leaked")]}),
            json!({"collection": format!("{private} where 1=1")}),
            json!({"collection": "public", "filter": {(format!("(select need from {private} limit 1)")): "private need"}}),
            json!({"collection": "public", "sort": [{"field": format!("(select need from {private} limit 1)"), "direction": "asc"}]}),
        ] {
            let mut params = attack;
            params["dbPath"] = json!(path);
            let error = module.state.dispatch("data/query", params).await.expect_err("SQL expression must not execute");
            assert!(error.contains("invalid storage identifier"), "{error}");
        }
        let ordinary = module.state.dispatch("data/query", json!({
            "dbPath": path, "collection": "public", "select": ["value"],
            "filter": {"value": 1}, "sort": [{"field": "value", "direction": "asc"}]
        })).await.unwrap();
        assert!(ordinary.to_json_value().unwrap()["success"].as_bool().unwrap());
        let names = module.state.list_collection_names(&path).await.unwrap().data.unwrap();
        assert!(names.iter().all(|name| require_public_collection(name).is_ok()));
        assert!(adapter.read(private, &"private-record".into()).await.success);
        assert!(module.state.clear_all_collections(&path).await.unwrap().success);
        assert!(!adapter.read(private, &"private-record".into()).await.success);

    }

    /// Helper: per-test isolated SQLite file routed through resolve_handle's
    /// legacy passthrough. Tests still hit the abstraction (handle resolves
    /// the same way TS callers' would); the passthrough is documented as a
    /// migration target pending sentinel-handle adoption everywhere. When
    /// the passthrough is removed, migrate these to per-test HOME +
    /// "main" handle.
    ///
    /// Returns (TempDir guard, db_path String). Hold the guard for the
    /// duration of the test — drop deletes the tempdir.
    pub(super) fn test_db_path(suffix: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .join(format!("{}.db", suffix))
            .to_string_lossy()
            .to_string();
        (dir, path)
    }

    /// Test seam: drive [`DataState::create_record`] through the SAME params
    /// deserialization the typed `data/create` command uses, returning the JSON
    /// envelope the legacy arm produced so the storage-layer tests below keep
    /// asserting one shape. (The command-object wiring itself is covered by
    /// `commands/data/mod.rs`'s tests; this exercises the data path.)
    pub(super) async fn create_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .create_record(handle, p.collection, p.id, p.data)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam: the read counterpart of [`create_via_state`].
    pub(super) async fn read_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::data::read::DataReadParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .read_record(handle, &p.collection, &p.id)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam: drive [`DataState::index_vector`] through the same
    /// [`VectorIndexParams`](crate::commands::vector::index::VectorIndexParams)
    /// deserialization the typed `vector/index` command uses. Returns the typed
    /// `StorageResult<DataRecord>` as JSON.
    pub(super) async fn index_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::vector::index::VectorIndexParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .index_vector(handle, &p.collection, p.id, p.embedding)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam for the typed `vector/stats` command. Returns [`VectorStats`] as
    /// top-level JSON (the new contract — no `data` envelope).
    pub(super) async fn stats_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::vector::stats::VectorStatsParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module.state.vector_stats(handle, &p.collection).await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam for the typed `vector/search` command. Returns
    /// [`VectorSearchResults`] as top-level JSON.
    pub(super) async fn search_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::vector::search::VectorSearchParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .vector_search(
                handle,
                &p.collection,
                p.query_vector,
                p.k,
                p.threshold,
                p.include_data,
            )
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam for the typed `vector/invalidate-cache` command. Returns
    /// [`VectorCacheInvalidation`] as top-level JSON.
    pub(super) async fn invalidate_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::vector::invalidate_cache::VectorInvalidateCacheParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .invalidate_vector_cache(handle, &p.collection)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam for the typed `vector/backfill` command. Mirrors the command's
    /// filter parse and returns [`VectorBackfillStats`] as top-level JSON.
    pub(super) async fn backfill_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::vector::backfill::VectorBackfillParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let filter: Option<std::collections::HashMap<String, FieldFilter>> = match p.filter {
            Some(map) => Some(
                serde_json::from_value(serde_json::Value::Object(map))
                    .map_err(|e| format!("invalid filter: {e}"))?,
            ),
            None => None,
        };
        let result = module
            .state
            .backfill_vectors(
                handle,
                &p.collection,
                &p.text_field,
                p.batch_size,
                p.model,
                filter,
            )
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// what this catches: card d2698cdc — native Windows launches without HOME
    /// lost chat and citizen memory. Every local handle must use the same native
    /// home fallback while preserving explicit overrides and citizen isolation.
    #[test]
    pub(super) fn database_handles_preserve_layout_with_native_home_and_overrides() {
        let handles = [
            ("main", "database/main.db"),
            ("@persona:Asha", "personas/Asha/data/longterm.db"),
            ("@agent:claude-code", "agents/claude-code/data/longterm.db"),
            ("@human:operator", "humans/operator/data/longterm.db"),
            ("@metrics", "metrics/metrics.sqlite"),
            (
                "e2f0e022-04ac-4f66-a26c-7146551745b4",
                "personas/e2f0e022-04ac-4f66-a26c-7146551745b4/longterm.db",
            ),
        ];
        for (home_override, native_home, expected_home) in [
            (
                Some("/tmp/explicit-home"),
                "C:/Users/native",
                "/tmp/explicit-home",
            ),
            (None, "C:/Users/native", "C:/Users/native"),
            (None, "/Users/native", "/Users/native"),
        ] {
            for (handle, suffix) in handles {
                let resolved = DataState::resolve_handle_with(
                    handle,
                    |key| match key {
                        "HOME" => home_override.map(str::to_owned),
                        // Empty DATABASE_URL must still select local SQLite.
                        "DATABASE_URL" => Some(String::new()),
                        _ => panic!("unexpected environment lookup: {key}"),
                    },
                    || {
                        assert!(home_override.is_none(), "explicit HOME takes precedence");
                        Some(PathBuf::from(native_home))
                    },
                )
                .expect("local handle resolves");
                assert_eq!(resolved, format!("{expected_home}/.continuum/{suffix}"));
            }
        }

        let url = "/var/shared/continuum.db";
        assert_eq!(
            DataState::resolve_handle_with(
                "main",
                |key| {
                    assert_eq!(key, "DATABASE_URL", "configured backend needs no HOME");
                    Some(url.to_owned())
                },
                || panic!("configured backend needs no native home"),
            )
            .unwrap(),
            url,
        );
        for (handle, _) in handles {
            let error = DataState::resolve_handle_with(handle, |_| None, || None).unwrap_err();
            assert_eq!(
                error,
                format!("resolve_handle('{handle}'): home directory unavailable")
            );
        }

        // Invalid sentinels fail before resolving a home or touching storage.
        for prefix in ["@persona:", "@agent:", "@human:"] {
            for slug in ["", "../evil", "nested/evil", "nested\\evil"] {
                assert!(DataState::resolve_handle_with(
                    &format!("{prefix}{slug}"),
                    |_| panic!("invalid slug needs no environment lookup"),
                    || panic!("invalid slug needs no native home"),
                )
                .is_err());
            }
        }
        for legacy in [url, "/tmp/legacy.db"] {
            assert_eq!(
                DataState::resolve_handle_with(
                    legacy,
                    |_| panic!("legacy connection needs no environment lookup"),
                    || panic!("legacy connection needs no native home"),
                )
                .unwrap(),
                legacy,
            );
        }
    }

    #[test]
    pub(super) fn data_create_params_drop_the_db_path_leak_but_alias_legacy_callers() {
        // what this catches: the persona-facing `data/create` contract dropped the
        // required `dbPath` leak — `handle` is optional and resolves to "main"
        // downstream — while legacy callers passing `dbPath` keep working through
        // the serde alias. If the alias or the optionality regresses, either the
        // persona surface re-grows a storage-handle leak or chat/sentinel/self_task
        // (which still send `dbPath`) silently start writing to the wrong store.
        let clean: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(json!({
                "collection": "test_users",
                "data": { "name": "Alice" }
            }))
            .expect("clean params deserialize");
        assert!(clean.handle.is_none(), "omitted handle stays None (→ main)");

        let legacy: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(json!({
                "dbPath": "some/store.db",
                "collection": "test_users",
                "data": { "name": "Alice" }
            }))
            .expect("legacy dbPath params deserialize");
        assert_eq!(
            legacy.handle.as_deref(),
            Some("some/store.db"),
            "dbPath is the wire name of the storage handle (the envelope owns `handle`)"
        );
    }

    #[tokio::test]
    pub(super) async fn test_data_module_create_and_read() {
        let module = DataModule::new(no_substrate_entities);
        let (_tmp, db_path) = test_db_path("create_and_read");

        // Create table first
        let schema = CollectionSchema {
            collection: "test_users".to_string(),
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

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create with dbPath
        let create_result = create_via_state(
            &module,
            json!({
                "dbPath": &db_path,
                "collection": "test_users",
                "data": { "name": "Alice" }
            }),
        )
        .await;

        assert!(
            create_result.is_ok(),
            "create_result failed: {:?}",
            create_result
        );

        if let Ok(CommandResult::Json(result)) = create_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            let id = result["data"]["id"].as_str().unwrap();

            // Read with dbPath
            let read_result = read_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_users",
                    "id": id
                }),
            )
            .await;

            assert!(read_result.is_ok());
            if let Ok(CommandResult::Json(read)) = read_result {
                assert!(read["success"].as_bool().unwrap_or(false));
                assert_eq!(read["data"]["data"]["name"], "Alice");
            }
        }
    }

    #[tokio::test]
    pub(super) async fn test_paginated_query() {
        let module = DataModule::new(no_substrate_entities);
        let (_tmp, db_path) = test_db_path("paginated");

        // Create schema
        let schema = CollectionSchema {
            collection: "test_paginated".to_string(),
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

        // Tests bypass the IPC surface (registered-entity-only post-Step 3).
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create 25 records
        for i in 0..25 {
            let _ = create_via_state(
                &module,
                json!({
                    "dbPath": db_path,
                    "collection": "test_paginated",
                    "data": { "name": format!("Item {}", i) }
                }),
            )
            .await;
        }

        // Open paginated query with page size 10. Default count_exact=false
        // means the response carries totalCount=0 (sentinel for "not
        // requested") and has_more starts optimistic; the LIMIT N+1 probe
        // in query-next is the authoritative has_more.
        let open_result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_paginated",
                    "pageSize": 10
                }),
            )
            .await;

        assert!(open_result.is_ok(), "open_result failed: {:?}", open_result);
        let query_id = if let Ok(CommandResult::Json(result)) = &open_result {
            let data = &result["data"];
            assert_eq!(
                data["totalCount"], 0,
                "QW#2: count_exact=false skips COUNT(*); 0 is the sentinel"
            );
            assert_eq!(data["pageSize"], 10);
            assert!(
                data["hasMore"].as_bool().unwrap(),
                "QW#2: open is optimistic, query-next is authoritative"
            );
            data["queryId"].as_str().unwrap().to_string()
        } else {
            panic!("Expected JSON result");
        };

        // Get first page
        let page1 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page1.is_ok());
        if let Ok(CommandResult::Json(result)) = &page1 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 1);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get second page
        let page2 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page2.is_ok());
        if let Ok(CommandResult::Json(result)) = &page2 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 2);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get third page (should have 5 items)
        let page3 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page3.is_ok());
        if let Ok(CommandResult::Json(result)) = &page3 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 5);
            assert_eq!(data["pageNumber"], 3);
            assert!(!data["hasMore"].as_bool().unwrap()); // No more pages
        }

        // Close query
        let close_result = module
            .handle_command("data/query-close", json!({ "queryId": query_id }))
            .await;

        assert!(close_result.is_ok());
        if let Ok(CommandResult::Json(result)) = close_result {
            assert!(result["success"].as_bool().unwrap());
        }
    }

    /// QW#2 back-compat: callers that need an exact "X of N" total can opt
    /// in via count_exact: true. This restores the pre-QW#2 behavior — one
    /// COUNT(*) at open time, totalCount populated in the response.
    #[tokio::test]
    pub(super) async fn test_paginated_query_count_exact() {
        let module = DataModule::new(no_substrate_entities);
        let (_tmp, db_path) = test_db_path("count_exact");

        let schema = CollectionSchema {
            collection: "test_count_exact".to_string(),
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
        // Tests bypass the IPC surface (registered-entity-only post-Step 3).
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        for i in 0..7 {
            let _ = create_via_state(
                &module,
                json!({
                    "dbPath": db_path,
                    "collection": "test_count_exact",
                    "data": { "name": format!("Item {}", i) }
                }),
            )
            .await;
        }

        let open_result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_count_exact",
                    "pageSize": 10,
                    "countExact": true
                }),
            )
            .await;

        assert!(open_result.is_ok(), "open_result failed: {:?}", open_result);
        if let Ok(CommandResult::Json(result)) = open_result {
            let data = &result["data"];
            assert_eq!(
                data["totalCount"], 7,
                "count_exact=true should populate totalCount via COUNT(*)"
            );
            assert!(data["hasMore"].as_bool().unwrap());
        } else {
            panic!("Expected JSON result");
        }
    }

mod part_2;
mod part_3;
