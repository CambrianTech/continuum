#![cfg(test)]
    use super::*;

    /// These tests require a live PostgreSQL instance.
    /// Run with: cargo test --lib -- --ignored postgres
    /// Or: DATABASE_URL=postgres://user:pass@localhost/testdb cargo test --lib -- --ignored

    fn get_test_url() -> String {
        std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://continuum:continuum@localhost:5432/continuum_test".to_string()
        })
    }

    async fn setup_pg_adapter() -> PostgresAdapter {
        let mut adapter = PostgresAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: get_test_url(),
                namespace: Some("test_orm".to_string()),
                timeout_ms: 30_000,
                max_connections: 5,
            })
            .await
            .expect("PostgreSQL connection failed - is Postgres running?");

        // Clean test schema
        let pool = adapter.pool().unwrap();
        let client = pool.get().await.unwrap();
        client
            .execute("SET search_path TO test_orm", &[])
            .await
            .unwrap();

        adapter
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_create_and_read() {
        let adapter = setup_pg_adapter().await;

        // Clean up from previous runs
        let _ = adapter.truncate("users").await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "users".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "name".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        let record = DataRecord {
            id: "pg-test-123".to_string(),
            collection: "users".to_string(),
            data: json!({"name": "test-user"}),
            metadata: RecordMetadata::default(),
        };

        let create_result = adapter.create(record).await;
        assert!(
            create_result.success,
            "Create failed: {:?}",
            create_result.error
        );

        let read_result = adapter.read("users", &"pg-test-123".to_string()).await;
        assert!(read_result.success, "Read failed: {:?}", read_result.error);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "test-user");
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_query_with_filters() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "query_test".to_string(),
                fields: vec![
                    super::super::types::SchemaField {
                        name: "name".to_string(),
                        field_type: super::super::types::FieldType::String,
                        indexed: true,
                        unique: false,
                        nullable: false,
                        max_length: None,
                        foreign_key: None,
                    },
                    super::super::types::SchemaField {
                        name: "age".to_string(),
                        field_type: super::super::types::FieldType::Number,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                        foreign_key: None,
                    },
                ],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("query_test").await;

        // Create test records
        for (i, name) in ["Alice", "Bob", "Charlie"].iter().enumerate() {
            let record = DataRecord {
                id: format!("qt-{}", i),
                collection: "query_test".to_string(),
                data: json!({"name": name, "age": 20 + i as i64 * 5}),
                metadata: RecordMetadata::default(),
            };
            let r = adapter.create(record).await;
            assert!(r.success, "Create failed: {:?}", r.error);
        }

        // Query all
        let all = adapter
            .query(StorageQuery {
                collection: "query_test".to_string(),
                ..Default::default()
            })
            .await;
        assert!(all.success);
        assert_eq!(all.data.as_ref().unwrap().len(), 3);

        // Query with filter
        let mut filter = HashMap::new();
        filter.insert("name".to_string(), FieldFilter::Value(json!("Alice")));
        let filtered = adapter
            .query(StorageQuery {
                collection: "query_test".to_string(),
                filter: Some(filter),
                ..Default::default()
            })
            .await;
        assert!(filtered.success);
        assert_eq!(filtered.data.as_ref().unwrap().len(), 1);

        // Count
        let count = adapter
            .count(StorageQuery {
                collection: "query_test".to_string(),
                ..Default::default()
            })
            .await;
        assert!(count.success);
        assert_eq!(count.data.unwrap(), 3);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_update_and_delete() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "update_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "name".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("update_test").await;

        // Create
        let record = DataRecord {
            id: "upd-1".to_string(),
            collection: "update_test".to_string(),
            data: json!({"name": "Before"}),
            metadata: RecordMetadata::default(),
        };
        adapter.create(record).await;

        // Update
        let updated = adapter
            .update(
                "update_test",
                &"upd-1".to_string(),
                json!({"name": "After"}),
                true,
            )
            .await;
        assert!(updated.success);
        let data = updated.data.unwrap();
        assert_eq!(data.data["name"], "After");

        // Delete
        let deleted = adapter.delete("update_test", &"upd-1".to_string()).await;
        assert!(deleted.success);
        assert!(deleted.data.unwrap());

        // Verify gone
        let read = adapter.read("update_test", &"upd-1".to_string()).await;
        assert!(!read.success);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_list_collections_and_stats() {
        let adapter = setup_pg_adapter().await;

        // Ensure at least one table exists
        adapter
            .ensure_schema(CollectionSchema {
                collection: "stats_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "data".to_string(),
                    field_type: super::super::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        let collections = adapter.list_collections().await;
        assert!(collections.success);
        assert!(collections
            .data
            .unwrap()
            .contains(&"stats_test".to_string()));

        let stats = adapter.collection_stats("stats_test").await;
        assert!(stats.success);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_batch_operations() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "batch_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "value".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("batch_test").await;

        let ops = vec![
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "batch_test".to_string(),
                id: Some("batch-1".to_string()),
                data: Some(json!({"value": "one"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "batch_test".to_string(),
                id: Some("batch-2".to_string()),
                data: Some(json!({"value": "two"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Read,
                collection: "batch_test".to_string(),
                id: Some("batch-1".to_string()),
                data: None,
            },
        ];

        let result = adapter.batch(ops).await;
        assert!(result.success);
        let results = result.data.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0]["success"].as_bool().unwrap());
        assert!(results[1]["success"].as_bool().unwrap());
        assert!(results[2]["success"].as_bool().unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_jsonb_columns() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "jsonb_test".to_string(),
                fields: vec![
                    super::super::types::SchemaField {
                        name: "config".to_string(),
                        field_type: super::super::types::FieldType::Json,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                        foreign_key: None,
                    },
                    super::super::types::SchemaField {
                        name: "tags".to_string(),
                        field_type: super::super::types::FieldType::Json,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                        foreign_key: None,
                    },
                ],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("jsonb_test").await;

        let record = DataRecord {
            id: "jsonb-1".to_string(),
            collection: "jsonb_test".to_string(),
            data: json!({
                "config": {"theme": "dark", "fontSize": 14},
                "tags": ["rust", "postgres", "orm"]
            }),
            metadata: RecordMetadata::default(),
        };

        let create = adapter.create(record).await;
        assert!(create.success, "JSONB create failed: {:?}", create.error);

        let read = adapter.read("jsonb_test", &"jsonb-1".to_string()).await;
        assert!(read.success, "JSONB read failed: {:?}", read.error);

        let data = read.data.unwrap();
        // JSONB should preserve the structure
        assert_eq!(data.data["config"]["theme"], "dark");
        assert_eq!(data.data["tags"][0], "rust");
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_clear_all() {
        let adapter = setup_pg_adapter().await;

        // Create a table with data
        adapter
            .ensure_schema(CollectionSchema {
                collection: "clear_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "x".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        let record = DataRecord {
            id: "clear-1".to_string(),
            collection: "clear_test".to_string(),
            data: json!({"x": "y"}),
            metadata: RecordMetadata::default(),
        };
        adapter.create(record).await;

        // Clear all
        let result = adapter.clear_all().await;
        assert!(result.success);

        // Verify empty
        let count = adapter
            .count(StorageQuery {
                collection: "clear_test".to_string(),
                ..Default::default()
            })
            .await;
        assert_eq!(count.data.unwrap(), 0);
    }

    /// Batch ATOMICITY against a real server — the four cases Astra specified
    /// after review of 4b1595c31 found four transactional defects that had all
    /// compiled cleanly and passed 102 green ORM tests, none of which touch
    /// Postgres. Each case below names the specific defect it would have caught.
    mod batch_transaction {
        use super::*;

        async fn adapter_with_pool(max_connections: usize) -> PostgresAdapter {
            let mut adapter = PostgresAdapter::new();
            adapter
                .initialize(AdapterConfig {
                    connection_string: get_test_url(),
                    namespace: Some("test_orm".to_string()),
                    timeout_ms: 30_000,
                    max_connections,
                })
                .await
                .expect("PostgreSQL connection failed - is Postgres running?");
            adapter
        }

        fn create(collection: &str, id: &str, data: Value) -> BatchOperation {
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: collection.to_string(),
                id: Some(id.to_string()),
                data: Some(data),
            }
        }

        fn update(collection: &str, id: &str, data: Value) -> BatchOperation {
            BatchOperation {
                operation_type: BatchOperationType::Update,
                collection: collection.to_string(),
                id: Some(id.to_string()),
                data: Some(data),
            }
        }

        // what this catches: a read-back inside the batch checking out a SECOND
        // pooled connection. That session cannot see the batch's uncommitted row,
        // so Create+Update of the same new row returned "Record not found"; and on
        // a ONE-connection pool it first blocks for the full 10s pool timeout,
        // because the only connection is the one its own caller holds. Pool size 1
        // is the whole point of the case — it turns a wrong answer into a hang.
        #[tokio::test]
        #[ignore] // Requires live Postgres
        async fn create_then_update_then_read_in_one_batch_on_a_single_connection_pool() {
            let adapter = adapter_with_pool(1).await;
            let _ = adapter.truncate("batch_same_row").await;
            let id = uuid::Uuid::new_v4().to_string();

            let result = adapter
                .batch(vec![
                    create("batch_same_row", &id, json!({"label": "first"})),
                    update("batch_same_row", &id, json!({"label": "second"})),
                    BatchOperation {
                        operation_type: BatchOperationType::Read,
                        collection: "batch_same_row".to_string(),
                        id: Some(id.clone()),
                        data: None,
                    },
                ])
                .await;

            assert!(
                result.success,
                "a row created earlier in the SAME batch must be visible to a later \
                 operation in it: {:?}",
                result.error
            );
            let stored = adapter.read("batch_same_row", &id).await;
            assert!(stored.success, "the committed row must be readable afterwards");
            assert_eq!(stored.data.unwrap().data["label"], "second");
        }

        // what this catches: a batch abandoned mid-transaction leaving its earlier
        // writes behind, and — worse — returning a connection with an ACTIVE
        // transaction to the pool. The old code opened a raw BEGIN and rolled back
        // only on paths it wrote; a DROPPED future runs none of them. Here the
        // batch is cancelled by timing it out while op2 waits on a row lock held
        // by another session, which is exactly the shape no error path can cover.
        #[tokio::test]
        #[ignore] // Requires live Postgres
        async fn a_cancelled_batch_leaves_no_writes_and_does_not_poison_the_pool() {
            let adapter = adapter_with_pool(4).await;
            let _ = adapter.truncate("batch_cancelled").await;

            let locked_id = uuid::Uuid::new_v4().to_string();
            let created = adapter
                .create(DataRecord {
                    id: locked_id.clone(),
                    collection: "batch_cancelled".to_string(),
                    data: json!({"label": "locked"}),
                    metadata: RecordMetadata::default(),
                })
                .await;
            assert!(created.success, "seed row: {:?}", created.error);

            // Hold a row lock from an INDEPENDENT session so op2 blocks.
            let pool = adapter.pool().unwrap();
            let mut locker = pool.get().await.unwrap();
            let lock_tx = locker.transaction().await.unwrap();
            lock_tx.execute("SET search_path TO test_orm", &[]).await.unwrap();
            lock_tx
                .execute(
                    "SELECT id FROM batch_cancelled WHERE id = $1 FOR UPDATE",
                    &[&locked_id],
                )
                .await
                .unwrap();

            let orphan_id = uuid::Uuid::new_v4().to_string();
            let cancelled = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                adapter.batch(vec![
                    create("batch_cancelled", &orphan_id, json!({"label": "op1"})),
                    update("batch_cancelled", &locked_id, json!({"label": "op2 blocks"})),
                ]),
            )
            .await;
            assert!(cancelled.is_err(), "the batch was expected to block on the row lock");

            // Releasing the lock lets any leaked transaction settle either way.
            drop(lock_tx);
            drop(locker);

            // THE INVARIANT: op1 must not survive a batch that never committed.
            let orphan = adapter.read("batch_cancelled", &orphan_id).await;
            assert!(
                !orphan.success,
                "a write from a CANCELLED batch survived — the transaction was not rolled back"
            );

            // AND the pool must still be usable: a connection handed back mid
            // transaction poisons whichever borrower gets it next.
            let after = adapter
                .batch(vec![create(
                    "batch_cancelled",
                    &uuid::Uuid::new_v4().to_string(),
                    json!({"label": "after"}),
                )])
                .await;
            assert!(
                after.success,
                "a later batch failed, so the cancelled one returned a poisoned \
                 connection to the pool: {:?}",
                after.error
            );
        }

        // what this catches: the column caches published when DDL RUNS rather than
        // when it commits. A batch that created a table/columns and then failed
        // left ensured_columns_cache and col_type_cache asserting a shape the
        // database did not have, so the NEXT write on the same adapter skipped the
        // DDL it needed. Reusing the same adapter is essential — a fresh one would
        // have an empty cache and hide the defect.
        #[tokio::test]
        #[ignore] // Requires live Postgres
        async fn a_failed_batch_leaves_no_cache_claiming_rolled_back_schema() {
            let adapter = adapter_with_pool(4).await;
            let fresh = format!("batch_cache_{}", uuid::Uuid::new_v4().simple());

            // op2 is malformed, so the batch fails after op1 has run.
            let failed = adapter
                .batch(vec![
                    create(&fresh, &uuid::Uuid::new_v4().to_string(), json!({"label": "a"})),
                    BatchOperation {
                        operation_type: BatchOperationType::Create,
                        collection: fresh.clone(),
                        id: Some(uuid::Uuid::new_v4().to_string()),
                        data: None,
                    },
                ])
                .await;
            assert!(!failed.success, "a malformed operation must fail the batch");

            // SAME adapter, same shape: this must still land. If the caches kept a
            // rolled-back claim, the write would skip its DDL and fail.
            let id = uuid::Uuid::new_v4().to_string();
            let after = adapter
                .batch(vec![create(&fresh, &id, json!({"label": "b"}))])
                .await;
            assert!(
                after.success,
                "a valid same-shape write after a failed batch failed — the schema \
                 cache is asserting something the database does not have: {:?}",
                after.error
            );
            assert!(adapter.read(&fresh, &id).await.success);
        }

        // what this catches: schema evolution inside a transaction. Postgres aborts
        // a transaction on the first error and rejects every later statement, so
        // update's missing-column ALTER retry could never succeed in there — the
        // path was not merely buggy, it was unreachable. Ensuring schema BEFORE the
        // transaction is what makes this case pass without per-op savepoints.
        #[tokio::test]
        #[ignore] // Requires live Postgres
        async fn a_batch_update_introducing_a_new_column_succeeds() {
            let adapter = adapter_with_pool(4).await;
            let fresh = format!("batch_evolve_{}", uuid::Uuid::new_v4().simple());
            let id = uuid::Uuid::new_v4().to_string();

            let seeded = adapter
                .batch(vec![create(&fresh, &id, json!({"label": "before"}))])
                .await;
            assert!(seeded.success, "seed batch: {:?}", seeded.error);

            // `note` does not exist yet — the write must add it.
            let evolved = adapter
                .batch(vec![update(
                    &fresh,
                    &id,
                    json!({"label": "after", "note": "a new text column"}),
                )])
                .await;
            assert!(
                evolved.success,
                "an update introducing a new column inside a batch failed — schema \
                 evolution is running inside the transaction: {:?}",
                evolved.error
            );

            let stored = adapter.read(&fresh, &id).await;
            assert!(stored.success);
            let data = stored.data.unwrap().data;
            assert_eq!(data["note"], "a new text column");
            assert_eq!(data["label"], "after");
        }
    }

