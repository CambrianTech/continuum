#![cfg(test)]
//! Migration replay and exact native-transfer tests.
use super::*;
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::SeaOrmAdapter;
    use crate::orm::types::{CollectionSchema, FieldType, SchemaField};

    #[tokio::test]
    async fn equal_counts_cannot_verify_a_rejected_migration() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let source = setup_sqlite(&dir.path().join("source.db").to_string_lossy()).await;
        let target = setup_sqlite(&dir.path().join("target.db").to_string_lossy()).await;
        let schema = CollectionSchema {
            collection: "items".into(),
            fields: crate::orm::base_entity_fields(),
            indexes: vec![],
        };
        for adapter in [&source, &target] {
            adapter.ensure_schema(schema.clone()).await.into_result()?;
            adapter.create(DataRecord {
                id: "collision".into(), collection: "items".into(),
                data: json!({}), metadata: RecordMetadata::default(),
            }).await.into_result()?;
        }
        let mut engine = MigrationEngine::new(source, target, MigrationConfig {
            batch_size: 1, collections: Some(vec!["items".into()]),
        });
        assert!(!engine.verify().await?.verified, "no run has no migration receipts");
        assert!(engine.run().await.is_err(), "a destination refusal stops migration");
        let status = engine.handle().status();
        assert_eq!(status.migrated, 0);
        assert_eq!(status.failed, 1);
        assert!(!engine.handle().is_running());
        let verification = engine.verify().await?;
        assert!(!verification.verified, "equal counts cannot erase the refusal");
        assert_eq!(verification.collections[0].source_count, 1);
        assert_eq!(verification.collections[0].target_count, 1);
        Ok(())
    }

    async fn setup_sqlite(path: &str) -> Arc<dyn StorageAdapter> {
        let adapter = SeaOrmAdapter::connect(AdapterConfig {
                connection_string: path.to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        Arc::new(adapter) as Arc<dyn StorageAdapter>
    }

    #[tokio::test]
    async fn test_migration_sqlite_to_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source.db");
        let target_path = dir.path().join("target.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        // Create schema + data in source
        source
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..25 {
            source
                .create(DataRecord {
                    id: format!("item-{}", i),
                    collection: "items".to_string(),
                    data: json!({"name": format!("Item {}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
        }

        // Run migration
        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig {
                batch_size: 10,
                collections: None,
            },
        );

        let status = engine.run().await.unwrap();
        assert_eq!(status.total, 25);
        assert_eq!(status.migrated, 25);
        assert_eq!(status.failed, 0);

        // Verify
        let verify = engine.verify().await.unwrap();
        assert!(verify.verified);

        // Check target has data
        let target_count = target
            .count(StorageQuery {
                collection: "items".to_string(),
                ..Default::default()
            })
            .await.into_result().expect("test: storage operation succeeds");
        assert_eq!(target_count, 25);
    }

    #[tokio::test]
    async fn test_migration_pause_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source_pause.db");
        let target_path = dir.path().join("target_pause.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "val".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..10 {
            source
                .create(DataRecord {
                    id: format!("p-{}", i),
                    collection: "items".to_string(),
                    data: json!({"val": format!("V{}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
        }

        let gated = Arc::new(crate::orm::gated_adapter::GatedAdapter::new(target.clone()));
        let gate = gated.pause_next_write();
        let mut engine = MigrationEngine::new(
            source.clone(),
            gated,
            MigrationConfig {
                batch_size: 1,
                collections: None,
            },
        );

        // Poll the real continuation to Pending before publishing resume. No
        // sleep or timeout stands in for observing the suspended future.
        engine.pause();
        let handle = engine.handle();
        let mut run = Box::pin(engine.run());
        std::future::poll_fn(|context| {
            use std::future::Future;
            assert!(run.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        }).await;
        assert!(handle.status().paused);
        assert!(handle.is_running());
        assert_eq!(handle.status().migrated, 0);
        handle.resume();
        tokio::select! {
            _ = gate.entered.notified() => {},
            result = &mut run => panic!("migration finished before held write: {result:?}"),
        }
        handle.pause();
        gate.release.notify_one();
        std::future::poll_fn(|context| {
            use std::future::Future;
            assert!(run.as_mut().poll(context).is_pending());
            let status = handle.status();
            if status.migrated == 1 {
                assert_eq!(status.collections[0].status, "paused");
                std::task::Poll::Ready(())
            } else { std::task::Poll::Pending }
        }).await;
        assert_eq!(target.count(StorageQuery {
            collection: "items".into(), ..Default::default()
        }).await.into_result(), Ok(1));
        handle.resume();
        let status = run.await.unwrap();
        assert_eq!(status.migrated, 10);
    }

    #[tokio::test]
    async fn test_migration_empty_source() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("empty_src.db");
        let target_path = dir.path().join("empty_tgt.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "empty".to_string(),
                fields: crate::orm::base_entity_fields(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        let mut engine =
            MigrationEngine::new(source.clone(), target.clone(), MigrationConfig::default());

        let status = engine.run().await.unwrap();
        assert_eq!(status.total, 0);
        assert_eq!(status.migrated, 0);
    }

    #[tokio::test]
    async fn test_migration_verify() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("verify_src.db");
        let target_path = dir.path().join("verify_tgt.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "verifyable".to_string(),
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "x".to_string(),
                    field_type: FieldType::Number,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..5 {
            source
                .create(DataRecord {
                    id: format!("v-{}", i),
                    collection: "verifyable".to_string(),
                    data: json!({"x": i}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
        }

        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig {
                batch_size: 100,
                collections: None,
            },
        );

        engine.run().await.unwrap();

        let verify = engine.verify().await.unwrap();
        assert!(verify.verified);
        assert_eq!(verify.collections[0].source_count, 5);
        assert_eq!(verify.collections[0].target_count, 5);
        assert!(verify.collections[0].matches);
    }

mod native {
use super::*;
use crate::orm::{adapter::AdapterConfig, binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType}, SeaOrmAdapter, types::{CollectionSchema, FieldType, SchemaField}};
use std::{collections::BTreeMap, num::NonZeroUsize};

#[tokio::test]
async fn native_rows_keep_bytes_descriptors_nulls_and_exact_integers_after_restart() -> Result<(), String> {
    let home = tempfile::tempdir().map_err(|e| e.to_string())?;
    let open = |name: &str| AdapterConfig {
        connection_string: home.path().join(name).to_string_lossy().into_owned(),
        ..Default::default()
    };
    let source = Arc::new(SeaOrmAdapter::connect(open("source.db")).await?);
    let target = Arc::new(SeaOrmAdapter::connect(open("target.db")).await?);
    let field = |name: &str, field_type| SchemaField {
        name: name.into(), field_type, indexed: false, unique: false, nullable: true,
        max_length: None, default_value: None, foreign_key: None,
    };
    source.ensure_schema(CollectionSchema {
        collection: "native_rows".into(),
        fields: crate::orm::base_entity_fields().into_iter().chain([
            field("ordinal", FieldType::Number), field("note", FieldType::String),
            field("payload", FieldType::Binary),
        ]).collect(), indexes: vec![],
    }).await.into_result()?;
    for index in 1_u128..=131 {
        let record = DataRecord {
            id: uuid::Uuid::from_u128(index).to_string(), collection: "native_rows".into(),
            data: serde_json::json!({"ordinal": 9_007_199_254_740_993_i64, "note": null}),
            metadata: RecordMetadata::default(),
        };
        if index == 3 {
            source.create(record).await.into_result()?;
        } else {
            let bytes = vec![0, 255, index as u8];
            let value = BinaryValue::new(BinaryDescriptor {
                element_type: ElementType::U8, byte_order: ByteOrder::Little,
                shape: vec![3], producer: format!("fixture-producer-{index}"), unit: false, scale: None,
            }, bytes)?;
            source.create_binary(record, BTreeMap::from([("payload".into(), value)])).await.into_result()?;
        }
    }
    let mut engine = MigrationEngine::new(source.clone(), target.clone(), MigrationConfig {
        batch_size: 2, collections: Some(vec!["native_rows".into()]),
    });
    assert_eq!(engine.run().await?.migrated, 131);
    drop(engine);
    drop(target);
    let target = SeaOrmAdapter::connect(open("target.db")).await?;
    let original = source.binary_snapshot("native_rows").await?;
    let restored = target.binary_snapshot("native_rows").await?;
    let mut after = None;
    let mut compared = 0;
    loop {
        let request = crate::orm::migration_snapshot::MigrationPage {
            after: after.as_deref(), limit: NonZeroUsize::new(2).ok_or("invalid fixture page")?,
        };
        let left = original.migration_page(&request).await?;
        let right = restored.migration_page(&request).await?;
        assert_eq!(left.len(), right.len());
        if left.is_empty() { break; }
        for (left, right) in left.into_iter().zip(right) {
            assert_eq!(left.record.id, right.record.id);
            assert_eq!(left.record.data, right.record.data);
            assert_eq!(left.payloads, right.payloads);
            after = Some(left.record.id);
            compared += 1;
        }
    }
    assert_eq!(compared, 131);
    drop(original);
    drop(restored);

    // Equal row counts cannot hide a changed native producer or payload.
    let checked = Arc::new(SeaOrmAdapter::connect(open("checked.db")).await?);
    let mut verification = MigrationEngine::new(source.clone(), checked.clone(), MigrationConfig {
        batch_size: 2, collections: Some(vec!["native_rows".into()]),
    });
    verification.run().await?;
    assert!(verification.verify().await?.verified);
    checked.update_binary("native_rows", &uuid::Uuid::from_u128(129).to_string(),
        BTreeMap::from([("payload".into(), BinaryValue::new(BinaryDescriptor {
            element_type: ElementType::U8, byte_order: ByteOrder::Little,
            shape: vec![3], producer: "different-producer".into(), unit: false, scale: None,
        }, vec![0, 255, 129])?)]), crate::orm::binary::BinaryWriteCondition::Replace,
    ).await.into_result()?;
    assert!(!verification.verify().await?.verified, "same bytes and counts, wrong producer");
    checked.update_binary("native_rows", &uuid::Uuid::from_u128(129).to_string(),
        BTreeMap::from([("payload".into(), BinaryValue::new(BinaryDescriptor {
            element_type: ElementType::U8, byte_order: ByteOrder::Little,
            shape: vec![3], producer: "fixture-producer-129".into(), unit: false, scale: None,
        }, vec![0, 254, 129])?)]), crate::orm::binary::BinaryWriteCondition::Replace,
    ).await.into_result()?;
    assert!(!verification.verify().await?.verified, "same descriptor and counts, wrong bytes");

    // Replay a corrupt descriptor on the second row of a page. The first
    // valid row must not reach the destination before the whole page validates.
    use sea_orm::{ConnectionTrait, DbBackend, sea_query::{Alias, Expr, Query}};
    let database = sea_orm::Database::connect(format!("sqlite://{}", open("source.db").connection_string))
        .await.map_err(|e| e.to_string())?;
    let mut damage = Query::update();
    damage.table(Alias::new(crate::orm::adapter::naming::to_table_name("native_rows")))
        .value(Alias::new("payload__descriptor"), Expr::value(vec![0_u8]))
        .and_where(Expr::col(Alias::new("id")).eq(uuid::Uuid::from_u128(2).to_string()));
    database.execute(DbBackend::Sqlite.build(&damage)).await.map_err(|e| e.to_string())?;
    database.close().await.map_err(|e| e.to_string())?;
    let rejected = Arc::new(SeaOrmAdapter::connect(open("rejected.db")).await?);
    let mut failed = MigrationEngine::new(source, rejected.clone(), MigrationConfig {
        batch_size: 2, collections: Some(vec!["native_rows".into()]),
    });
    assert!(failed.run().await.is_err());
    assert_eq!(rejected.count(StorageQuery {
        collection: "native_rows".into(), ..Default::default()
    }).await.into_result()?, 0, "a malformed page publishes no prefix");
    assert!(!failed.verify().await?.verified);
    Ok(())
}

}
