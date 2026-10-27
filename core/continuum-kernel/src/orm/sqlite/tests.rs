#![cfg(test)]
    use super::*;
    use tempfile::tempdir;

    async fn setup_adapter() -> (SqliteAdapter, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut adapter = SqliteAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            })
            .await
            .unwrap();

        (adapter, dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_create_and_read() {
        let (adapter, _dir) = setup_adapter().await;

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
            id: "test-123".to_string(),
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

        let read_result = adapter.read("users", &"test-123".to_string()).await;
        assert!(read_result.success, "Read failed: {:?}", read_result.error);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "test-user");
    }

    // what this catches: a batch that HALF-APPLIES while reporting success. Until this
    // test, `batch` was a plain loop of individually autocommitting create/update calls
    // that folded each failure into a {"success": false} JSON element and returned
    // StorageResult::ok regardless — so a parent row survived a failing child and the
    // caller could not tell. Both adapters declared supports_transactions: true while
    // neither ever opened a transaction (the flag is read by nothing).
    //
    // The consumer is card 0d51573a: a StagedCredit parent and its StagedCreditGeneration
    // children are written as ONE unit. A surviving parent whose children were rolled
    // back is a credit record whose generation provenance silently vanished — the exact
    // "claims cleaner provenance than actually happened" defect the card exists to stop.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_failing_operation_rolls_back_every_earlier_operation_in_the_batch() {
        let (adapter, _dir) = setup_adapter().await;

        let parent = "staged-credit-parent".to_string();

        // The second operation is malformed (Create with no data), so it fails AFTER
        // the first has already been applied inside the transaction.
        let failing = vec![
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "staged_credit".to_string(),
                id: Some(parent.clone()),
                data: Some(json!({"cardId": "0d51573a"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "staged_credit".to_string(),
                id: Some("staged-credit-child".to_string()),
                data: None,
            },
        ];

        let result = adapter.batch(failing).await;
        assert!(
            !result.success,
            "a batch containing a failing operation must not report success"
        );

        // THE INVARIANT: the operation that already succeeded must not survive.
        let orphan = adapter.read("staged_credit", &parent).await;
        assert!(
            !orphan.success,
            "the parent row survived a rolled-back batch — the batch half-applied"
        );

        // POSITIVE CONTROL: without this, an implementation that rolled back
        // unconditionally (or never wrote at all) would pass every assertion above.
        let good = vec![
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "staged_credit".to_string(),
                id: Some(parent.clone()),
                data: Some(json!({"cardId": "0d51573a"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "staged_credit".to_string(),
                id: Some("staged-credit-child".to_string()),
                data: Some(json!({"stagedCreditId": "staged-credit-parent"})),
            },
        ];
        let committed = adapter.batch(good).await;
        assert!(
            committed.success,
            "a well-formed batch must commit: {:?}",
            committed.error
        );
        assert!(
            adapter.read("staged_credit", &parent).await.success,
            "a committed batch must leave its rows readable"
        );
        assert!(
            adapter
                .read("staged_credit", &"staged-credit-child".to_string())
                .await
                .success,
            "a committed batch must leave EVERY row readable, not just the first"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_reads() {
        let (adapter, _dir) = setup_adapter().await;

        // Pre-create table via schema
        adapter
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "value".to_string(),
                    field_type: super::super::types::FieldType::Number,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            })
            .await;

        // Create test data
        for i in 0..20 {
            let record = DataRecord {
                id: format!("item-{}", i),
                collection: "items".to_string(),
                data: json!({"value": i}),
                metadata: RecordMetadata::default(),
            };
            let result = adapter.create(record).await;
            assert!(
                result.success,
                "Create item-{} failed: {:?}",
                i, result.error
            );
        }

        // Truly concurrent reads via spawn_blocking — each goes to a different reader
        let ids: Vec<String> = (0..20).map(|i| format!("item-{}", i)).collect();
        let futures: Vec<_> = ids.iter().map(|id| adapter.read("items", id)).collect();
        let results = futures::future::join_all(futures).await;

        let success_count = results.iter().filter(|r| r.success).count();
        let errors: Vec<_> = results
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.success)
            .map(|(i, r)| format!("item-{}: {:?}", i, r.error))
            .collect();
        assert_eq!(
            success_count, 20,
            "All reads should succeed (got {}/20). Errors: {:?}",
            success_count, errors
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_writes() {
        let (adapter, _dir) = setup_adapter().await;

        // Concurrent writes — all serialized through the writer Mutex
        let mut futures = Vec::new();
        for i in 0..10 {
            let record = DataRecord {
                id: format!("write-{}", i),
                collection: "concurrent_writes".to_string(),
                data: json!({"value": i, "name": format!("item-{}", i)}),
                metadata: RecordMetadata::default(),
            };
            futures.push(adapter.create(record));
        }
        let results = futures::future::join_all(futures).await;

        let success_count = results.iter().filter(|r| r.success).count();
        assert_eq!(success_count, 10, "All concurrent writes should succeed");

        // Verify all data is readable
        let query_result = adapter
            .query(StorageQuery {
                collection: "concurrent_writes".to_string(),
                ..Default::default()
            })
            .await;
        assert!(query_result.success);
        assert_eq!(query_result.data.unwrap().len(), 10);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_query_projection_evolves_missing_columns_before_select() {
        let (adapter, _dir) = setup_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "recipes".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "displayName".to_string(),
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

        let result = adapter
            .query(StorageQuery {
                collection: "recipes".to_string(),
                select: Some(vec![
                    "displayName".to_string(),
                    "team".to_string(),
                    "modes".to_string(),
                ]),
                limit: Some(10),
                ..Default::default()
            })
            .await;

        assert!(
            result.success,
            "projection query should evolve missing selected columns: {:?}",
            result.error
        );
    }

    // what this catches: a collection written row by row (its table created from the
    // first row, never through ensure_schema) must still get the indexes its entity
    // declares, or every room page of chat_messages is a full scan plus a temp B-tree
    // (103,319 rows on the M5, 2026-10-26). Also pins the two not-yet cases: a table
    // nobody wrote is left alone, and an index over a column the table has not grown
    // yet waits for the next pass instead of failing the store open.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn declared_indexes_reach_a_table_created_from_its_rows() {
        use super::super::types::{SchemaField, SchemaIndex};
        let (adapter, _dir) = setup_adapter().await;
        let declared = |collection: &str| CollectionSchema {
            collection: collection.to_string(),
            fields: vec![SchemaField {
                name: "senderType".to_string(),
                field_type: FieldType::String,
                indexed: true,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![SchemaIndex {
                name: "idx_paged_rows_room_timestamp".to_string(),
                fields: vec!["roomId".to_string(), "timestamp".to_string()],
                unique: false,
            }],
        };

        let nothing_written = adapter.ensure_declared_indexes(vec![declared("paged_rows")]).await;
        assert_eq!(nothing_written, IndexPass { declared: 2, ..IndexPass::default() });

        let row = DataRecord {
            id: "row-1".to_string(),
            collection: "paged_rows".to_string(),
            data: json!({"roomId": "room-a", "timestamp": "2026-10-26T00:00:00Z"}),
            metadata: RecordMetadata::default(),
        };
        assert!(adapter.create(row).await.success);

        let pass = adapter.ensure_declared_indexes(vec![declared("paged_rows")]).await;
        assert_eq!(pass, IndexPass { declared: 2, ensured: 1, awaiting_columns: 1, failed: vec![] });

        let writer = adapter.get_writer().expect("initialized adapter has a writer");
        let conn = writer.lock().expect("writer lock");
        let plan: Vec<String> = conn
            .prepare(
                "EXPLAIN QUERY PLAN SELECT * FROM paged_rows WHERE room_id = 'room-a' \
                 AND timestamp < 'z' ORDER BY timestamp DESC LIMIT 50",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| r.get::<_, String>(3))?.collect::<Result<Vec<_>, _>>()
            })
            .expect("plan reads");
        assert!(
            plan.iter().any(|step| step.contains("USING INDEX idx_paged_rows_room_timestamp")),
            "the room page must search the declared index, got {plan:?}"
        );
        assert!(!plan.iter().any(|step| step.contains("TEMP B-TREE")), "no sort pass: {plan:?}");
    }

    // what this catches: a performance index taking the store down. A declared index
    // the database refuses (here UNIQUE over duplicate rows) must come back counted,
    // naming its entity and index with the database's error, while the other declared
    // index still builds and the store keeps serving reads and writes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_index_that_cannot_be_built_is_counted_and_the_store_stays_open() {
        use super::super::types::SchemaIndex;
        let (adapter, _dir) = setup_adapter().await;
        for id in ["dup-1", "dup-2"] {
            let row = DataRecord {
                id: id.to_string(),
                collection: "dup_rows".to_string(),
                data: json!({"slug": "same", "roomId": "room-a"}),
                metadata: RecordMetadata::default(),
            };
            assert!(adapter.create(row).await.success);
        }
        let schema = CollectionSchema {
            collection: "dup_rows".to_string(),
            fields: vec![],
            indexes: vec![
                SchemaIndex { name: "idx_dup_rows_slug".to_string(), fields: vec!["slug".to_string()], unique: true },
                SchemaIndex { name: "idx_dup_rows_room".to_string(), fields: vec!["roomId".to_string()], unique: false },
            ],
        };

        let pass = adapter.ensure_declared_indexes(vec![schema]).await;
        assert_eq!((pass.declared, pass.ensured, pass.failed.len()), (2, 1, 1));
        let failure = &pass.failed[0];
        assert_eq!((failure.collection.as_str(), failure.index.as_str()), ("dup_rows", "idx_dup_rows_slug"));
        assert!(failure.error.contains("UNIQUE"), "the database's error is kept: {}", failure.error);

        let read = adapter.read("dup_rows", &"dup-1".to_string()).await;
        assert!(read.success, "the store still reads: {:?}", read.error);
        let more = DataRecord {
            id: "dup-3".to_string(),
            collection: "dup_rows".to_string(),
            data: json!({"slug": "same", "roomId": "room-b"}),
            metadata: RecordMetadata::default(),
        };
        assert!(adapter.create(more).await.success, "the store still writes");
    }
