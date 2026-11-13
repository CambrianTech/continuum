use super::{binary::{BinaryDescriptor, BinaryValue, BinaryWriteCondition, ByteOrder, ElementType}, *};
use super::adapter::AdapterConfig;
use super::relational::RelationalStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "native_payload_test")]
struct NativeRow {
    #[serde(flatten)]
    base: BaseEntity,
    #[entity(binary)]
    payload: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "required_native_payload_test")]
struct RequiredNativeRow {
    #[serde(flatten)]
    base: BaseEntity,
    label: String,
    #[entity(binary)]
    payload: Vec<u8>,
    #[entity(binary)]
    sketch: Vec<u8>,
}

#[tokio::test]
async fn required_native_fields_create_atomically_without_body_readback() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native-pair.sqlite");
    let path = path.to_str().ok_or("test path is not UTF-8")?;
    let store = open(path).await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    let record = || DataRecord {
        id: "native".into(), collection: RequiredNativeRow::COLLECTION.into(),
        data: json!({"label":"typed scalar"}), metadata: RecordMetadata::default(),
    };
    let payloads = || -> Result<_, String> { Ok(std::collections::BTreeMap::from([
        ("payload".into(), BinaryValue::new(BinaryDescriptor {
            unit: true, scale: Some(0.125), ..descriptor(ElementType::I8, vec![2])
        }, vec![0, 8])?),
        ("sketch".into(), BinaryValue::new(descriptor(ElementType::Bit, vec![2]), vec![0])?),
    ])) };
    assert!(store.create(record()).await.is_err());
    let mut incomplete = payloads()?;
    incomplete.remove("sketch");
    assert!(store.create_binary(record(), incomplete).await.is_err());
    assert_eq!(store.count(QueryBuilder::new(RequiredNativeRow::COLLECTION).build()).await?, 0);
    assert_eq!(store.create_binary(record(), payloads()?).await?, "native");
    assert!(store.create_binary(record(), payloads()?).await.is_err());
    let page = store.binary_page(RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref().ok_or("missing required payload")?.bytes(), &[0, 8]);
    let query = QueryBuilder::new(RequiredNativeRow::COLLECTION).select(vec!["label".into()]).limit(1).build();
    let rows = store.query(query).await?;
    assert_eq!(rows[0].data["label"], "typed scalar");
    assert!(rows[0].data.get("payload").is_none());
    drop(store);
    let store = open(path).await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    for (field, expected) in payloads()? {
        let page = store.binary_page(RequiredNativeRow::COLLECTION, &field, None, 1).await?;
        assert_eq!(page[0].value.as_ref(), Some(&expected));
    }
    Ok(())
}

fn descriptor(element_type: ElementType, shape: Vec<u64>) -> BinaryDescriptor {
    BinaryDescriptor { element_type, byte_order: ByteOrder::Little, shape, producer: "fixture/native-v1".into(), unit: false, scale: None }
}

#[tokio::test]
async fn native_write_fault_replay_keeps_bytes_after_request_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    use std::{sync::Arc, time::Duration};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cancelled-native.db");
    let path = path.to_str().ok_or("non-UTF8 fixture path")?;
    let real = Arc::new(SeaOrmAdapter::connect(AdapterConfig {
        connection_string: path.into(), ..Default::default()
    }).await?);
    real.ensure_schema(RequiredNativeRow::collection_schema()).await.into_result()?;
    let gated = Arc::new(super::gated_adapter::GatedAdapter::new(real.clone()));
    for creating in [true, false] {
        let gate = gated.pause_next_write();
        let caller = gated.clone();
        let payload = BinaryValue::new(descriptor(ElementType::F16, vec![2]),
            if creating { vec![0, 0x80, 0xff, 0x7f] } else { vec![0xff, 0x7f, 0, 0x80] })?;
        let request = tokio::spawn(async move {
            if creating {
                caller.create_binary(DataRecord {
                    id: "cancelled".into(), collection: RequiredNativeRow::COLLECTION.into(),
                    data: json!({"label":"native cancellation"}), metadata: RecordMetadata::default(),
                }, std::collections::BTreeMap::from([
                    ("payload".into(), payload),
                    ("sketch".into(), BinaryValue::new(descriptor(ElementType::Bit, vec![2]), vec![0])?),
                ])).await.into_result().map(|_| ())
            } else {
                caller.update_binary(RequiredNativeRow::COLLECTION, "cancelled", "payload", payload, BinaryWriteCondition::Replace)
                    .await.into_result().map(|_| ())
            }
        });
        tokio::time::timeout(Duration::from_secs(5), gate.entered.notified()).await?;
        request.abort();
        assert!(request.await.is_err_and(|error| error.is_cancelled()));
        // A pending write must not block an independent metadata reader.
        let count = tokio::time::timeout(Duration::from_secs(5),
            real.count(QueryBuilder::new(RequiredNativeRow::COLLECTION).build())).await?.into_result()?;
        assert_eq!(count, usize::from(!creating));
        gate.release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), gate.done.notified()).await?;
        let page = real.binary_page(RequiredNativeRow::COLLECTION, "payload", None, 1).await.into_result()?;
        let payload = page[0].value.as_ref().ok_or("cancelled write lost payload")?;
        assert_eq!(payload.bytes(), if creating { &[0, 0x80, 0xff, 0x7f] } else { &[0xff, 0x7f, 0, 0x80] });
        assert_eq!(payload.descriptor(), &descriptor(ElementType::F16, vec![2]));
    }
    drop(gated);
    drop(real);
    let reopened = open(path).await?;
    reopened.ensure(RequiredNativeRow::collection_schema()).await?;
    let page = reopened.binary_page(RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref().ok_or("restart lost payload")?.bytes(), &[0xff, 0x7f, 0, 0x80]);
    Ok(())
}

#[test]
fn native_payload_moves_allocation_and_checks_only_descriptor() -> Result<(), String> {
    // Arbitrary bit patterns must survive; storage is not a numeric validator.
    let bytes = vec![0xff, 0x7f, 0x00, 0x80];
    let pointer = bytes.as_ptr();
    let value = BinaryValue::new(descriptor(ElementType::F16, vec![2]), bytes)?;
    assert_eq!(value.bytes().as_ptr(), pointer);
    assert_eq!(value.bytes(), &[0xff, 0x7f, 0, 0x80]);
    assert!(BinaryValue::new(descriptor(ElementType::F32, vec![2]), vec![0; 7]).is_err());
    assert!(descriptor(ElementType::F64, vec![u64::MAX, 2]).byte_len().is_err());
    let header = value.descriptor().encode()?;
    assert_eq!(BinaryDescriptor::decode(&header)?, *value.descriptor());
    assert!(BinaryDescriptor::decode(&header[..header.len() - 1]).is_err());
    Ok(())
}

#[test]
fn packed_bits_and_quantization_headers_are_exact_and_fail_closed() -> Result<(), String> {
    for (bits, bytes) in [(0, 0), (1, 1), (8, 1), (9, 2), (768, 96)] {
        assert_eq!(descriptor(ElementType::Bit, vec![bits]).byte_len()?, bytes);
    }
    let scaled = BinaryDescriptor { unit: true, scale: Some(0.125),
        ..descriptor(ElementType::I8, vec![2]) };
    let bytes = vec![0, 8];
    let pointer = bytes.as_ptr();
    let value = BinaryValue::new(scaled.clone(), bytes)?;
    assert_eq!(value.bytes().as_ptr(), pointer);
    let header = scaled.encode()?;
    assert_eq!(BinaryDescriptor::decode(&header)?, scaled);
    for length in 0..header.len() {
        assert!(BinaryDescriptor::decode(&header[..length]).is_err());
    }
    let mut bad = header.clone();
    bad[8] |= 4;
    assert!(BinaryDescriptor::decode(&bad).is_err());
    bad = header;
    bad[0] = 1;
    assert!(BinaryDescriptor::decode(&bad).is_err());
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(BinaryDescriptor { scale: Some(scale), ..scaled.clone() }.encode().is_err());
    }
    assert!(BinaryDescriptor { element_type: ElementType::F32, ..scaled }.encode().is_err());
    assert!(BinaryDescriptor { unit: true, ..descriptor(ElementType::Bit, vec![8]) }.encode().is_err());
    Ok(())
}

async fn open(path: &str) -> Result<RelationalStore, String> {
    let store = RelationalStore::open(AdapterConfig { connection_string: path.into(), ..Default::default() }).await?;
    store.ensure(NativeRow::collection_schema()).await?;
    Ok(store)
}

#[tokio::test]
async fn delayed_backfill_cannot_replace_a_newer_producer_payload() -> Result<(), String> {
    let store = open(":memory:").await?;
    store.create(DataRecord { id: "race".into(), collection: NativeRow::COLLECTION.into(),
        data: json!({"payload":null}), metadata: RecordMetadata::default() }).await?;
    // Both producers observed the same vacant row. The second completion must
    // preserve the winner's bytes, descriptor and revision in one SQL mutation.
    let winner = BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![0x80, 0xff])?;
    let delayed = BinaryValue::new(descriptor(ElementType::F32, vec![2]), vec![0; 8])?;
    assert!(store.update_binary(NativeRow::COLLECTION.into(), "race".into(), "payload".into(),
        winner.clone(), BinaryWriteCondition::IfAbsent).await?);
    let metadata_query = || QueryBuilder::new(NativeRow::COLLECTION).filter_eq("id", "race")
        .select(vec!["id".into()]).limit(1).build();
    let before = store.query(metadata_query()).await?;
    assert!(!store.update_binary(NativeRow::COLLECTION.into(), "race".into(), "payload".into(),
        delayed, BinaryWriteCondition::IfAbsent).await?);
    let after = store.query(metadata_query()).await?;
    assert_eq!(before[0].metadata.version, after[0].metadata.version);
    assert_eq!(before[0].metadata.updated_at, after[0].metadata.updated_at);
    let page = store.binary_page(NativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref(), Some(&winner));
    assert!(!store.update_binary(NativeRow::COLLECTION.into(), "removed".into(), "payload".into(),
        winner, BinaryWriteCondition::IfAbsent).await?);
    Ok(())
}

#[tokio::test]
async fn native_bytes_and_descriptors_survive_pages_atomic_update_and_restart() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native.db");
    let path = path.to_str().ok_or("non-UTF8 test path")?;
    let store = open(path).await?;
    let operations = (0..10003).map(|n| types::BatchOperation {
        operation_type: types::BatchOperationType::Create,
        collection: NativeRow::COLLECTION.into(), id: Some(format!("{n:06}")),
        data: Some(json!({"payload":null})),
    }).collect();
    store.batch(operations).await?;
    let variants = [
        (ElementType::F16, vec![0x00, 0x80, 0xff, 0x7f]),
        (ElementType::I8, vec![0x80, 0xff]),
        (ElementType::F32, vec![0, 0, 0, 0x80, 0xff, 0xff, 0x7f, 0x7f]),
    ];
    for (n, (kind, bytes)) in variants.iter().enumerate() {
        store.update_binary(NativeRow::COLLECTION.into(), format!("{n:06}"), "payload".into(),
            BinaryValue::new(descriptor(*kind, vec![2]), bytes.clone())?, BinaryWriteCondition::Replace).await?;
    }
    assert!(store.binary_page(NativeRow::COLLECTION, "payload", None, 0).await.is_err());
    let stats = store.numeric_array_stats(NativeRow::COLLECTION, "payload").await?;
    assert_eq!(stats.total_records, 10003);
    assert_eq!(stats.non_empty_records, 3);
    assert_eq!((stats.min_length, stats.max_length), (2, 2));
    // Backfill narrows an existing field predicate without hydrating payloads
    // or silently replacing the caller's constraint.
    for (existing, expected) in [
        (query::QueryOperator::Eq(json!(null)), 10000),
        (query::QueryOperator::IsNotNull, 0),
    ] {
        let missing = QueryBuilder::new(NativeRow::COLLECTION)
            .filter("payload", query::QueryOperator::And(vec![existing, query::QueryOperator::IsNull]))
            .select(vec!["id".into()]).limit(2).build();
        assert_eq!(store.count(missing.clone()).await?, expected);
        let rows = store.query(missing).await?;
        assert_eq!(rows.len(), expected.min(2));
        assert!(rows.iter().all(|row| row.data.get("payload").is_none()));
    }
    // Generic scalar writes cannot mutate just the payload of a committed pair.
    assert!(store.update_on(&store.db, NativeRow::COLLECTION, "000000", json!({"payload":null}), true).await.is_err());
    drop(store);
    let reopened = open(path).await?;
    let mut cursor = None;
    for (n, (kind, bytes)) in variants.iter().enumerate() {
        let page = reopened.binary_page(NativeRow::COLLECTION, "payload", cursor, 1).await?;
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, format!("{n:06}"));
        let value = page[0].value.as_ref().ok_or("missing native payload")?;
        assert_eq!(value.bytes(), bytes);
        assert_eq!(value.descriptor(), &descriptor(*kind, vec![2]));
        cursor = Some(page[0].id.clone());
    }
    let null_page = reopened.binary_page(NativeRow::COLLECTION, "payload", cursor, 1).await?;
    assert!(null_page[0].value.is_none());
    Ok(())
}
