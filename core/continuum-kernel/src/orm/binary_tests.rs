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
        id: uuid::Uuid::from_u128(1).to_string(), collection: RequiredNativeRow::COLLECTION.into(),
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
    assert_eq!(store.create_binary(record(), payloads()?).await?, uuid::Uuid::from_u128(1).to_string());
    assert!(store.create_binary(record(), payloads()?).await.is_err());
    let page = read_page(&store, RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref().ok_or("missing required payload")?.bytes(), &[0, 8]);
    let query = QueryBuilder::new(RequiredNativeRow::COLLECTION).select(vec!["label".into()]).limit(1).build();
    let rows = store.query(query).await?;
    assert_eq!(rows[0].data["label"], "typed scalar");
    assert!(rows[0].data.get("payload").is_none());
    drop(store);
    let store = open(path).await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    for (field, expected) in payloads()? {
        let page = read_page(&store, RequiredNativeRow::COLLECTION, &field, None, 1).await?;
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
                    id: uuid::Uuid::from_u128(1).to_string(), collection: RequiredNativeRow::COLLECTION.into(),
                    data: json!({"label":"native cancellation"}), metadata: RecordMetadata::default(),
                }, std::collections::BTreeMap::from([
                    ("payload".into(), payload),
                    ("sketch".into(), BinaryValue::new(descriptor(ElementType::Bit, vec![2]), vec![0])?),
                ])).await.into_result().map(|_| ())
            } else {
                caller.update_binary(RequiredNativeRow::COLLECTION, &uuid::Uuid::from_u128(1).to_string(), "payload", payload, BinaryWriteCondition::Replace)
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
        let mut bytes = Vec::new(); let mut ids = Vec::new();
        let header = real.binary_page_into(RequiredNativeRow::COLLECTION, "payload", None, 1, &mut bytes, &mut ids).await.into_result()?.ok_or("missing descriptor")?;
        assert_eq!(ids, [uuid::Uuid::from_u128(1)]);
        let page = [TestRecord { id: ids.remove(0).to_string(), value: Some(BinaryValue::new(header, bytes)?) }];
        let payload = page[0].value.as_ref().ok_or("cancelled write lost payload")?;
        assert_eq!(payload.bytes(), if creating { &[0, 0x80, 0xff, 0x7f] } else { &[0xff, 0x7f, 0, 0x80] });
        assert_eq!(payload.descriptor(), &descriptor(ElementType::F16, vec![2]));
    }
    drop(gated);
    drop(real);
    let reopened = open(path).await?;
    reopened.ensure(RequiredNativeRow::collection_schema()).await?;
    let page = read_page(&reopened, RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
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
    store.create(DataRecord { id: uuid::Uuid::from_u128(1).to_string(), collection: NativeRow::COLLECTION.into(),
        data: json!({"payload":null}), metadata: RecordMetadata::default() }).await?;
    // Both producers observed the same vacant row. The second completion must
    // preserve the winner's bytes, descriptor and revision in one SQL mutation.
    let winner = BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![0x80, 0xff])?;
    let delayed = BinaryValue::new(descriptor(ElementType::F32, vec![2]), vec![0; 8])?;
    assert!(store.update_binary(NativeRow::COLLECTION.into(), uuid::Uuid::from_u128(1).to_string(), "payload".into(),
        winner.clone(), BinaryWriteCondition::IfAbsent).await?);
    let metadata_query = || QueryBuilder::new(NativeRow::COLLECTION).filter_eq("id", uuid::Uuid::from_u128(1).to_string())
        .select(vec!["id".into()]).limit(1).build();
    let before = store.query(metadata_query()).await?;
    assert!(!store.update_binary(NativeRow::COLLECTION.into(), uuid::Uuid::from_u128(1).to_string(), "payload".into(),
        delayed, BinaryWriteCondition::IfAbsent).await?);
    let after = store.query(metadata_query()).await?;
    assert_eq!(before[0].metadata.version, after[0].metadata.version);
    assert_eq!(before[0].metadata.updated_at, after[0].metadata.updated_at);
    let page = read_page(&store, NativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref(), Some(&winner));
    assert!(!store.update_binary(NativeRow::COLLECTION.into(), uuid::Uuid::from_u128(2).to_string(), "payload".into(),
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
        collection: NativeRow::COLLECTION.into(), id: Some(uuid::Uuid::from_u128(n as u128).to_string()),
        data: Some(json!({"payload":null})),
    }).collect();
    store.batch(operations).await?;
    let variants = [
        (ElementType::F16, vec![0x00, 0x80, 0xff, 0x7f]),
        (ElementType::I8, vec![0x80, 0xff]),
        (ElementType::F32, vec![0, 0, 0, 0x80, 0xff, 0xff, 0x7f, 0x7f]),
    ];
    for (n, (kind, bytes)) in variants.iter().enumerate() {
        store.update_binary(NativeRow::COLLECTION.into(), uuid::Uuid::from_u128(n as u128).to_string(), "payload".into(),
            BinaryValue::new(descriptor(*kind, vec![2]), bytes.clone())?, BinaryWriteCondition::Replace).await?;
    }
    assert!(read_page(&store, NativeRow::COLLECTION, "payload", None, 0).await.is_err());
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
    assert!(store.update_on(&store.db, NativeRow::COLLECTION, &uuid::Uuid::nil().to_string(), json!({"payload":null}), true).await.is_err());
    drop(store);
    let reopened = open(path).await?;
    let mut cursor = None;
    for (n, (kind, bytes)) in variants.iter().enumerate() {
        let page = read_page(&reopened, NativeRow::COLLECTION, "payload", cursor, 1).await?;
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, uuid::Uuid::from_u128(n as u128).to_string());
        let value = page[0].value.as_ref().ok_or("missing native payload")?;
        assert_eq!(value.bytes(), bytes);
        assert_eq!(value.descriptor(), &descriptor(*kind, vec![2]));
        cursor = Some(page[0].id.clone());
    }
    let null_page = read_page(&reopened, NativeRow::COLLECTION, "payload", cursor, 1).await?;
    assert!(null_page.is_empty());
    Ok(())
}

// Single-row contract assertions may materialize a value; the production API and
// load replay use contiguous caller buffers directly.
struct TestRecord { id: String, value: Option<BinaryValue> }
async fn read_page(store: &RelationalStore, collection: &str, field: &str,
    after: Option<String>, limit: usize) -> Result<Vec<TestRecord>, String> {
    let mut bytes = Vec::new();
    let mut ids = Vec::new();
    let descriptor = store.binary_page_into(collection, field, after.map(|id| uuid::Uuid::parse_str(&id)).transpose().map_err(|e| e.to_string())?, limit, &mut bytes, &mut ids).await?;
    match descriptor {
        None => Ok(Vec::new()),
        Some(descriptor) => {
            if ids.len() != 1 { return Err("single-row assertion received multiple rows".into()); }
            Ok(vec![TestRecord { id: ids.remove(0).to_string(), value: Some(BinaryValue::new(descriptor, bytes)?) }])
        }
    }
}

#[tokio::test]
async fn bulk_native_page_reuses_capacity_and_refuses_mixed_descriptors_atomically() -> Result<(), String> {
    let store = open(":memory:").await?;
    // More empty rows than the requested page: filtering must happen before LIMIT.
    for id in [0, 1, 2] {
        store.create(DataRecord { id: uuid::Uuid::from_u128(id).to_string(), collection: NativeRow::COLLECTION.into(),
            data: json!({"payload": null}), metadata: RecordMetadata::default() }).await?;
    }
    for (id, kind, payload) in [(10, ElementType::I8, vec![1, 2]), (11, ElementType::I8, vec![3, 4]), (12, ElementType::F16, vec![0, 0, 0, 0])] {
        store.create_binary(DataRecord { id: uuid::Uuid::from_u128(id).to_string(), collection: NativeRow::COLLECTION.into(),
            data: json!({}), metadata: RecordMetadata::default() },
            std::collections::BTreeMap::from([("payload".into(), BinaryValue::new(descriptor(kind, vec![2]), payload)?)])) .await?;
    }
    let mut bytes = Vec::with_capacity(64);
    bytes.push(99);
    let pointer = bytes.as_ptr();
    let mut ids = vec![uuid::Uuid::from_u128(99)];
    assert!(store.binary_page_into(NativeRow::COLLECTION, "payload", None, 3, &mut bytes, &mut ids).await.is_err());
    assert_eq!(bytes, [99]);
    assert_eq!(ids, [uuid::Uuid::from_u128(99)]);
    let result = store.binary_page_into(NativeRow::COLLECTION, "payload", None, 2, &mut bytes, &mut ids).await?;
    assert_eq!(result, Some(descriptor(ElementType::I8, vec![2])));
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes, [99, 1, 2, 3, 4]);
    assert_eq!(ids, [99, 10, 11].map(uuid::Uuid::from_u128));
    Ok(())
}
#[tokio::test]
async fn native_identity_refuses_aliases_and_corrupt_late_rows_without_appending() -> Result<(), String> {
    use sea_orm::ConnectionTrait;
    let store = open(":memory:").await?;
    let value = || BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![1, 2]);
    let record = |id: String| DataRecord { id, collection: NativeRow::COLLECTION.into(),
        data: json!({}), metadata: RecordMetadata::default() };
    for bad in ["invalid", "00000000000000000000000000000001", "00000000-0000-0000-0000-00000000000A"] {
        assert!(store.create_binary(record(bad.into()), std::collections::BTreeMap::from([("payload".into(), value()?)])) .await.is_err());
    }
    let id = uuid::Uuid::from_u128(1);
    store.create_binary(record(id.to_string()), std::collections::BTreeMap::from([("payload".into(), value()?)])) .await?;
    // Inject malformed persisted identity through the ORM's statement builder to
    // exercise a late-row fault, without adding a production repair/decoding path.
    let statement = store.insert_statement_with_binary(&record("z-invalid".into()),
        std::collections::BTreeMap::from([("payload".into(), value()?)]))?;
    store.write(move |_, transaction| Box::pin(async move {
        transaction.execute(sea_orm::DbBackend::Sqlite.build(&statement)).await.map_err(|e| e.to_string())?;
        Ok(())
    })).await?;
    let mut bytes = vec![99];
    let mut ids = vec![uuid::Uuid::from_u128(99)];
    assert!(store.binary_page_into(NativeRow::COLLECTION, "payload", None, 2, &mut bytes, &mut ids).await.is_err());
    assert_eq!(bytes, [99]);
    assert_eq!(ids, [uuid::Uuid::from_u128(99)]);
    Ok(())
}
#[tokio::test]
async fn native_revision_custody_is_transactional_monotonic_and_durable() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::EntityTrait;
    use super::relational_native_revision::Entity as Revisions;
    use super::relational_sequence::{Domain, Entity as Heads};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("revision-custody.sqlite");
    let path = path.to_str().ok_or("non-UTF8 fixture path")?;
    let store = open(path).await?;
    let id = uuid::Uuid::from_u128(1).to_string();
    let payload = || BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![1, 2]);
    // The same collection has separate append and native custody domains.
    store.write(|_, transaction| Box::pin(async move {
        super::relational_sequence::next_on(transaction, Domain::Append, NativeRow::COLLECTION, 100).await
    })).await?;
    store.create_binary(DataRecord { id: id.clone(), collection: NativeRow::COLLECTION.into(),
        data: json!({}), metadata: RecordMetadata::default() },
        std::collections::BTreeMap::from([("payload".into(), payload()?)])) .await?;
    let key = (NativeRow::COLLECTION.to_owned(), id.clone());
    let revision = Revisions::find_by_id(key.clone()).one(&store.db).await?.ok_or("missing revision")?;
    assert_eq!(revision.revision, 1);
    assert!(!revision.deleted);
    assert!(!store.update_binary(NativeRow::COLLECTION.into(), id.clone(), "payload".into(), payload()?, BinaryWriteCondition::IfAbsent).await?);
    assert_eq!(Revisions::find_by_id(key.clone()).one(&store.db).await?.ok_or("missing unchanged revision")?.revision, 1);
    assert!(store.update_binary(NativeRow::COLLECTION.into(), id.clone(), "payload".into(), payload()?, BinaryWriteCondition::Replace).await?);
    assert_eq!(Revisions::find_by_id(key.clone()).one(&store.db).await?.ok_or("missing updated revision")?.revision, 2);
    let failed_id = id.clone();
    let failed: Result<(), String> = store.write(move |store, transaction| Box::pin(async move {
        store.native_changed_on(transaction, NativeRow::COLLECTION, &failed_id).await?;
        Err("injected failure after revision write".into())
    })).await;
    assert!(failed.is_err());
    drop(store);
    let store = open(path).await?;
    assert_eq!(Revisions::find_by_id(key).one(&store.db).await?.ok_or("missing restarted revision")?.revision, 2);
    assert_eq!(Heads::find_by_id(("native".to_owned(), NativeRow::COLLECTION.to_owned())).one(&store.db).await?.ok_or("missing native head")?.high, 2);
    assert_eq!(Heads::find_by_id(("append".to_owned(), NativeRow::COLLECTION.to_owned())).one(&store.db).await?.ok_or("missing append head")?.high, 100);
    assert!(store.update_binary(NativeRow::COLLECTION.into(), id.clone(), "payload".into(), payload()?, BinaryWriteCondition::Replace).await?);
    assert_eq!(Revisions::find_by_id((NativeRow::COLLECTION.to_owned(), id)).one(&store.db).await?.ok_or("missing next revision")?.revision, 3);
    Ok(())
}
#[tokio::test]
async fn revision_counter_exhaustion_refuses_without_float_promotion() -> Result<(), String> {
    use super::relational_sequence::{next_on, Domain};
    let store = open(":memory:").await?;
    assert_eq!(store.write(|_, transaction| Box::pin(async move {
        next_on(transaction, Domain::Native, "exhaustion", i64::MAX).await
    })).await?, i64::MAX);
    assert!(store.write(|_, transaction| Box::pin(async move {
        next_on(transaction, Domain::Native, "exhaustion", 1).await
    })).await.is_err());
    use sea_orm::EntityTrait;
    let persisted = super::relational_sequence::Entity::find_by_id(("native".to_owned(), "exhaustion".to_owned()))
        .one(&store.db).await.map_err(|e| e.to_string())?.ok_or("missing maximum head")?;
    assert_eq!(persisted.high, i64::MAX);
    Ok(())
}
#[tokio::test]
async fn native_delete_batch_clear_and_recreate_keep_transactional_tombstones() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::EntityTrait;
    use super::relational_native_revision::Entity as Revisions;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native-delete.sqlite");
    let path = path.to_str().ok_or("non-UTF8 fixture path")?;
    let store = open(path).await?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    for n in 1..=3 {
        store.create_binary(DataRecord { id: id(n), collection: NativeRow::COLLECTION.into(),
            data: json!({}), metadata: RecordMetadata::default() },
            std::collections::BTreeMap::from([("payload".into(), BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![1, 2])?)])) .await?;
    }
    let delete = |n| types::BatchOperation { operation_type: types::BatchOperationType::Delete,
        collection: NativeRow::COLLECTION.into(), id: Some(id(n)), data: None };
    let failed = store.batch(vec![delete(1), types::BatchOperation {
        operation_type: types::BatchOperationType::Create, collection: NativeRow::COLLECTION.into(),
        id: Some(id(2)), data: Some(json!({"payload": null})),
    }]).await;
    assert!(failed.is_err());
    let key = |n| (NativeRow::COLLECTION.to_owned(), id(n));
    let receipt = Revisions::find_by_id(key(1)).one(&store.db).await?.ok_or("missing rollback receipt")?;
    assert!(!receipt.deleted);
    assert_eq!(receipt.revision, 1);
    assert_eq!(store.count(QueryBuilder::new(NativeRow::COLLECTION).build()).await?, 3);
    assert_eq!(store.delete_matching(QueryBuilder::new(NativeRow::COLLECTION).filter_eq("id", id(1)).build()).await?, 1);
    assert_eq!(store.delete_matching(QueryBuilder::new(NativeRow::COLLECTION).filter_eq("id", id(1)).build()).await?, 0);
    assert_eq!(Revisions::find_by_id(key(1)).one(&store.db).await?.ok_or("missing deleted receipt")?.revision, 4);
    store.batch(vec![delete(2)]).await?;
    store.clear().await?;
    assert_eq!(Revisions::find_by_id(key(2)).one(&store.db).await?.ok_or("missing batch receipt")?.revision, 5);
    assert_eq!(Revisions::find_by_id(key(3)).one(&store.db).await?.ok_or("missing clear receipt")?.revision, 6);
    store.create_binary(DataRecord { id: id(1), collection: NativeRow::COLLECTION.into(),
        data: json!({}), metadata: RecordMetadata::default() },
        std::collections::BTreeMap::from([("payload".into(), BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![3, 4])?)])) .await?;
    drop(store);
    let store = open(path).await?;
    let recreated = Revisions::find_by_id(key(1)).one(&store.db).await?.ok_or("missing recreated receipt")?;
    assert_eq!(recreated.revision, 7);
    assert!(!recreated.deleted);
    for n in [2, 3] { assert!(Revisions::find_by_id(key(n)).one(&store.db).await?.ok_or("missing durable tombstone")?.deleted); }
    Ok(())
}