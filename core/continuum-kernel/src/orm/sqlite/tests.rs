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
