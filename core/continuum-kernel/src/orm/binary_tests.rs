use super::{binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType}, *};
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
}

#[tokio::test]
async fn required_native_fields_create_atomically_without_body_readback() -> Result<(), Box<dyn std::error::Error>> {
    let store = open(":memory:").await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    let record = || DataRecord {
        id: "native".into(), collection: RequiredNativeRow::COLLECTION.into(),
        data: json!({"label":"typed scalar"}), metadata: RecordMetadata::default(),
    };
    let payloads = || -> Result<_, String> { Ok(std::collections::BTreeMap::from([
        ("payload".into(), BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![0xff, 0x80])?),
    ])) };
    assert!(store.create(record()).await.is_err());
    assert_eq!(store.create_binary(record(), payloads()?).await?, "native");
    assert!(store.create_binary(record(), payloads()?).await.is_err());
    let page = store.binary_page(RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(page[0].value.as_ref().ok_or("missing required payload")?.bytes(), &[0xff, 0x80]);
    let query = QueryBuilder::new(RequiredNativeRow::COLLECTION).select(vec!["label".into()]).limit(1).build();
    let rows = store.query(query).await?;
    assert_eq!(rows[0].data["label"], "typed scalar");
    assert!(rows[0].data.get("payload").is_none());
    Ok(())
}

fn descriptor(element_type: ElementType, shape: Vec<u64>) -> BinaryDescriptor {
    BinaryDescriptor { element_type, byte_order: ByteOrder::Little, shape, producer: "fixture/native-v1".into() }
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

async fn open(path: &str) -> Result<RelationalStore, String> {
    let store = RelationalStore::open(AdapterConfig { connection_string: path.into(), ..Default::default() }).await?;
    store.ensure(NativeRow::collection_schema()).await?;
    Ok(store)
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
            BinaryValue::new(descriptor(*kind, vec![2]), bytes.clone())?).await?;
    }
    assert!(store.binary_page(NativeRow::COLLECTION, "payload", None, 0).await.is_err());
    let stats = store.numeric_array_stats(NativeRow::COLLECTION, "payload").await?;
    assert_eq!(stats.total_records, 10003);
    assert_eq!(stats.non_empty_records, 3);
    assert_eq!((stats.min_length, stats.max_length), (2, 2));
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
