#![cfg(test)]
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
        batch_size: 2, throttle_ms: 0, collections: Some(vec!["native_rows".into()]),
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
        batch_size: 2, throttle_ms: 0, collections: Some(vec!["native_rows".into()]),
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
        batch_size: 2, throttle_ms: 0, collections: Some(vec!["native_rows".into()]),
    });
    assert!(failed.run().await.is_err());
    assert_eq!(rejected.count(StorageQuery {
        collection: "native_rows".into(), ..Default::default()
    }).await.into_result()?, 0, "a malformed page publishes no prefix");
    assert!(!failed.verify().await?.verified);
    Ok(())
}
