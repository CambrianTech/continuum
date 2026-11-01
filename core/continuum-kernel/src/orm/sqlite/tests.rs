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

    /// The sequenced append and the store-owned revision it publishes (Slice P's arrival order).
    mod sequenced {
        use super::*;
        use crate::orm::types::{FieldType, SchemaField, SequenceSpec};

        fn field(name: &str, field_type: FieldType) -> SchemaField {
            SchemaField {
                name: name.to_string(),
                field_type,
                indexed: true,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            }
        }

        async fn arrivals() -> (SqliteAdapter, tempfile::TempDir) {
            let (adapter, dir) = setup_adapter().await;
            let schema = CollectionSchema {
                collection: "arrivals".to_string(),
                fields: vec![
                    field("messageId", FieldType::String),
                    field("arrivalSeq", FieldType::Number),
                ],
                indexes: vec![],
            };
            assert!(
                adapter.ensure_schema(schema).await.success,
                "test: the schema registers"
            ); // test: a fixture step that must succeed
            let lines = CollectionSchema {
                collection: "lines".to_string(),
                fields: vec![field("text", FieldType::String)],
                indexes: vec![],
            };
            assert!(
                adapter.ensure_schema(lines).await.success,
                "test: the line schema registers"
            ); // test: a fixture step that must succeed
            (adapter, dir)
        }

        fn append(id: &str, message: &str) -> BatchOperation {
            BatchOperation {
                operation_type: BatchOperationType::Append(SequenceSpec {
                    sequence: "arrivalSeq".into(),
                    unique: "messageId".into(),
                }),
                collection: "arrivals".to_string(),
                id: Some(id.to_string()),
                data: Some(json!({"messageId": message})),
            }
        }

        fn seq_of(result: &StorageResult<Vec<Value>>) -> (u64, bool) {
            let last = result
                .data
                .as_ref()
                .and_then(|v| v.last())
                .expect("test: the batch reported its append"); // test: the assertion failing loudly
            (
                last["seq"].as_u64().expect("test: a sequence"),
                last["fresh"].as_bool().expect("test: a freshness flag"),
            ) // test: the assertion failing loudly
        }

        // what this catches: an arrival order that is not commit order, or a duplicate import
        // minting a second arrival for one message (the replay cursor would deliver it twice).
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn appends_number_in_commit_order_and_a_duplicate_reuses_its_arrival() {
            let (adapter, _dir) = arrivals().await;
            assert_eq!(
                seq_of(&adapter.batch(vec![append("a1", "m1")]).await),
                (1, true)
            );
            assert_eq!(
                seq_of(&adapter.batch(vec![append("a2", "m2")]).await),
                (2, true)
            );
            assert_eq!(
                seq_of(&adapter.batch(vec![append("a3", "m1")]).await),
                (1, false),
                "the duplicate reuses m1's arrival"
            );
            assert!(
                !adapter.read("arrivals", &"a3".to_string()).await.success,
                "no second row for m1"
            );
            assert_eq!(
                seq_of(&adapter.batch(vec![append("a4", "m3")]).await),
                (3, true),
                "no gap from the duplicate"
            );
        }

        // what this catches: an import of a line already stored failing the whole batch (so a
        // replay after a crash could never commit), or minting a second row or arrival for it.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn an_ensured_row_and_its_append_are_idempotent_together() {
            let (adapter, _dir) = arrivals().await;
            let line = |arrival: &str| {
                vec![
                    BatchOperation {
                        operation_type: BatchOperationType::Ensure,
                        collection: "lines".into(),
                        id: Some("m1".into()),
                        data: Some(json!({"text": "hi"})),
                    },
                    append(arrival, "m1"),
                ]
            };
            let first = adapter.batch(line("a1")).await;
            assert_eq!(
                first.data.as_ref().map(|v| v[0]["created"].clone()),
                Some(json!(true))
            );
            assert_eq!(seq_of(&first), (1, true));
            let again = adapter.batch(line("a2")).await;
            assert!(again.success, "the replay commits: {:?}", again.error);
            assert_eq!(
                again.data.as_ref().map(|v| v[0]["created"].clone()),
                Some(json!(false)),
                "the stored line is untouched"
            );
            assert_eq!(seq_of(&again), (1, false), "and its arrival is reused");
        }

        // what this catches: a rolled-back batch consuming a sequence or announcing a revision
        // for a row that never committed (a consumer would wait for, or skip over, a ghost).
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_rolled_back_append_neither_numbers_nor_announces() {
            let (adapter, _dir) = arrivals().await;
            let revision = adapter.sequence_revision("arrivals");
            let broken = BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "arrivals".into(),
                id: Some("x".into()),
                data: None,
            };
            assert!(
                !adapter
                    .batch(vec![append("a1", "m1"), broken])
                    .await
                    .success
            );
            assert_eq!(
                *revision.borrow(),
                0,
                "nothing committed, nothing announced"
            );
            assert_eq!(
                seq_of(&adapter.batch(vec![append("a1", "m1")]).await),
                (1, true),
                "the rolled-back sequence is reused"
            );
            assert_eq!(*revision.borrow(), 1);
        }

        // what this catches: a committed row stranded unannounced because its writer was
        // dropped at the commit await (Codex at 2abfa3c3). The future is polled once (the
        // blocking write is spawned) and dropped; the revision still rises.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_writer_dropped_at_the_commit_await_still_announces_its_row() {
            let (adapter, _dir) = arrivals().await;
            let mut revision = adapter.sequence_revision("arrivals");
            let mut write = Box::pin(adapter.batch(vec![append("a1", "m1")]));
            let _ = futures::poll!(&mut write); // the first poll spawns the blocking write; whether it has finished does not matter
            drop(write);
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                revision.wait_for(|high| *high == 1),
            )
            .await
            .expect("test: the dropped writer's commit is announced") // test: the assertion failing loudly
            .expect("test: the revision stays owned by the store"); // test: the assertion failing loudly
            assert!(
                adapter.read("arrivals", &"a1".to_string()).await.success,
                "and the row is there"
            );
        }
    }
