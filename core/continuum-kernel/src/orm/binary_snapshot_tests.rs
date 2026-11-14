#![cfg(test)]

use super::*;
use crate::orm::binary::{BinaryPageOutput, BinaryPageRequest};
use std::{collections::BTreeMap, num::NonZeroUsize};

fn payload(bytes: Vec<u8>) -> Result<BTreeMap<String, BinaryValue>, String> {
    Ok(BTreeMap::from([(
        "payload".into(),
        BinaryValue::new(descriptor(ElementType::I8, vec![2]), bytes)?,
    )]))
}

async fn insert(store: &RelationalStore, id: u128) -> Result<(), String> {
    store
        .create_binary(
            DataRecord {
                id: uuid::Uuid::from_u128(id).to_string(),
                collection: NativeRow::COLLECTION.into(),
                data: json!({}),
                metadata: RecordMetadata::default(),
            },
            payload(vec![1, 2])?,
        )
        .await?;
    Ok(())
}

async fn page(
    snapshot: &BinarySnapshot,
    after: Option<uuid::Uuid>,
) -> Result<(Vec<u8>, Vec<uuid::Uuid>), String> {
    let mut bytes = Vec::new();
    let mut ids = Vec::new();
    snapshot
        .page_into(
            &BinaryPageRequest {
                field: "payload",
                after,
                limit: NonZeroUsize::new(1).ok_or("positive fixture limit")?,
            },
            BinaryPageOutput {
                bytes: &mut bytes,
                ids: &mut ids,
            },
        )
        .await?;
    Ok((bytes, ids))
}

#[tokio::test]
async fn snapshot_pins_empty_head_and_pages_while_writer_commits(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("snapshot.db");
    let store = open(path.to_str().ok_or("non-UTF8 fixture path")?).await?;
    let empty = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(empty.revision(), 0);
    // Await actual commits, never a timer: the read snapshot must not hold the writer gate.
    insert(&store, 1).await?;
    insert(&store, 2).await?;
    assert_eq!(page(&empty, None).await?, (vec![], vec![]));
    drop(empty);

    let snapshot = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(snapshot.revision(), 2);
    let first = uuid::Uuid::from_u128(1);
    let second = uuid::Uuid::from_u128(2);
    assert_eq!(page(&snapshot, None).await?, (vec![1, 2], vec![first]));
    assert!(
        store
            .update_binary(
                NativeRow::COLLECTION.into(),
                second.to_string(),
                payload(vec![3, 4])?,
                BinaryWriteCondition::Replace
            )
            .await?
    );
    assert_eq!(
        page(&snapshot, Some(first)).await?,
        (vec![1, 2], vec![second])
    );
    assert_eq!(snapshot.revision(), 2);
    let fresh = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(fresh.revision(), 3);
    assert_eq!(page(&fresh, Some(first)).await?, (vec![3, 4], vec![second]));
    drop(fresh);
    assert_eq!(
        store
            .delete_matching(
                QueryBuilder::new(NativeRow::COLLECTION)
                    .filter("id", QueryOperator::Eq(second.to_string().into()))
                    .build()
            )
            .await?,
        1
    );
    assert_eq!(
        page(&snapshot, Some(first)).await?,
        (vec![1, 2], vec![second])
    );
    drop(snapshot);
    let fresh = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(fresh.revision(), 4);
    assert_eq!(page(&fresh, Some(first)).await?, (vec![], vec![]));
    drop(fresh);
    drop(store);
    let reopened = open(path.to_str().ok_or("non-UTF8 fixture path")?).await?;
    let restored = reopened.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(restored.revision(), 4);
    assert_eq!(page(&restored, None).await?, (vec![1, 2], vec![first]));
    Ok(())
}
