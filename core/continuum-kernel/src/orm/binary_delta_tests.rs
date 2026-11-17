#![cfg(test)]
use super::*;
use crate::orm::binary_delta::*;
use std::{collections::BTreeMap, num::NonZeroUsize};

#[tokio::test]
async fn delta_refuses_late_corruption_without_partial_output(
) -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::sea_query::{Alias, Expr, Query};
    use sea_orm::{ConnectionTrait, DbBackend};
    let store = open(":memory:").await?;
    for id in 1..=2 {
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
    }
    // Fault injection through the storage owner's statement builder, not an application path.
    let malformed = Query::update()
        .table(Alias::new(NativeRow::COLLECTION))
        .value(Alias::new("payload"), vec![9_u8])
        .and_where(Expr::col(Alias::new("id")).eq(uuid::Uuid::from_u128(1).to_string()))
        .to_owned();
    store
        .write(move |_, transaction| {
            Box::pin(async move {
                transaction
                    .execute(DbBackend::Sqlite.build(&malformed))
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        })
        .await?;
    let snapshot = store.binary_snapshot(NativeRow::COLLECTION).await?;
    let mut bytes = vec![99];
    let mut changes = vec![NativeDelta {
        cursor: NativeDeltaCursor {
            revision: 1,
            id: uuid::Uuid::from_u128(99),
        },
        value: NativeDeltaValue::Remove,
    }];
    let request = NativeDeltaRequest {
        field: "payload",
        since: 0,
        after: None,
        limit: NonZeroUsize::new(10).ok_or("positive fixture limit")?,
    };
    assert!(matches!(
        snapshot
            .changes_into(
                &request,
                NativeDeltaOutput {
                    bytes: &mut bytes,
                    changes: &mut changes
                }
            )
            .await,
        Err(NativeDeltaError::Custody)
    ));
    assert_eq!(bytes, vec![99]);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].cursor.id, uuid::Uuid::from_u128(99));
    let empty = NativeDeltaRequest {
        since: snapshot.revision(),
        ..request
    };
    assert!(snapshot
        .changes_into(
            &empty,
            NativeDeltaOutput {
                bytes: &mut bytes,
                changes: &mut changes
            }
        )
        .await?
        .is_none());
    let invalid = NativeDeltaRequest {
        after: Some(NativeDeltaCursor {
            revision: 0,
            id: uuid::Uuid::from_u128(2),
        }),
        since: 0,
        ..empty
    };
    assert!(matches!(
        snapshot
            .changes_into(
                &invalid,
                NativeDeltaOutput {
                    bytes: &mut bytes,
                    changes: &mut changes
                }
            )
            .await,
        Err(NativeDeltaError::Interval)
    ));
    assert_eq!(bytes, vec![99]);
    assert_eq!(changes.len(), 1);
    Ok(())
}

fn payload(bytes: Vec<u8>) -> Result<BTreeMap<String, BinaryValue>, String> {
    Ok(BTreeMap::from([(
        "payload".into(),
        BinaryValue::new(descriptor(ElementType::I8, vec![2]), bytes)?,
    )]))
}

#[tokio::test]
async fn native_delta_pages_pin_payloads_and_include_tied_tombstones(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("deltas.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let store = open(path).await?;
    for id in 1..=4 {
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
    }
    let before = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(
        store
            .delete_matching(
                QueryBuilder::new(NativeRow::COLLECTION)
                    .filter(
                        "id",
                        QueryOperator::In(vec![
                            uuid::Uuid::from_u128(2).to_string().into(),
                            uuid::Uuid::from_u128(3).to_string().into()
                        ])
                    )
                    .build()
            )
            .await?,
        2
    );
    store
        .update_binary(
            NativeRow::COLLECTION.into(),
            uuid::Uuid::from_u128(1).to_string(),
            payload(vec![3, 4])?,
            BinaryWriteCondition::Replace,
        )
        .await?;
    // Recreating only scalar state does not resurrect an absent native field.
    store
        .create(DataRecord {
            id: uuid::Uuid::from_u128(2).to_string(),
            collection: NativeRow::COLLECTION.into(),
            data: json!({"payload":null}),
            metadata: RecordMetadata::default(),
        })
        .await?;
    let snapshot = store.binary_snapshot(NativeRow::COLLECTION).await?;
    assert_eq!(snapshot.revision(), 6);
    store
        .update_binary(
            NativeRow::COLLECTION.into(),
            uuid::Uuid::from_u128(1).to_string(),
            payload(vec![5, 6])?,
            BinaryWriteCondition::Replace,
        )
        .await?;
    let mut bytes = vec![99];
    let mut changes = Vec::new();
    let mut after = None;
    loop {
        after = snapshot
            .changes_into(
                &NativeDeltaRequest {
                    field: "payload",
                    since: 0,
                    after,
                    limit: NonZeroUsize::MIN,
                },
                NativeDeltaOutput {
                    bytes: &mut bytes,
                    changes: &mut changes,
                },
            )
            .await?;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(
        changes
            .iter()
            .map(|change| (change.cursor.revision, change.cursor.id.as_u128()))
            .collect::<Vec<_>>(),
        vec![(6, 1), (5, 3), (5, 2), (4, 4)]
    );
    assert_eq!(bytes, vec![99, 3, 4, 1, 2]);
    assert!(
        matches!(changes[0].value, NativeDeltaValue::Upsert { ref bytes, .. } if bytes == &(1..3))
    );
    assert_eq!(changes[1].value, NativeDeltaValue::Remove);
    assert_eq!(changes[2].value, NativeDeltaValue::Remove);
    let mut old_bytes = Vec::new();
    let mut old_changes = Vec::new();
    before
        .changes_into(
            &NativeDeltaRequest {
                field: "payload",
                since: 0,
                after: None,
                limit: NonZeroUsize::new(10).ok_or("positive fixture limit")?,
            },
            NativeDeltaOutput {
                bytes: &mut old_bytes,
                changes: &mut old_changes,
            },
        )
        .await?;
    assert_eq!(old_changes.len(), 4);
    assert_eq!(old_bytes, vec![1, 2, 1, 2, 1, 2, 1, 2]);
    drop(before);
    drop(snapshot);
    drop(store);
    let reopened = open(path).await?;
    let next = reopened.binary_snapshot(NativeRow::COLLECTION).await?;
    let mut next_bytes = Vec::new();
    let mut next_changes = Vec::new();
    assert!(next
        .changes_into(
            &NativeDeltaRequest {
                field: "payload",
                since: 6,
                after: None,
                limit: NonZeroUsize::new(10).ok_or("positive fixture limit")?
            },
            NativeDeltaOutput {
                bytes: &mut next_bytes,
                changes: &mut next_changes
            }
        )
        .await?
        .is_none());
    assert_eq!(next_changes.len(), 1);
    assert_eq!(next_changes[0].cursor.revision, 7);
    assert_eq!(next_bytes, vec![5, 6]);
    let saved = next_bytes.clone();
    assert!(matches!(
        next.changes_into(
            &NativeDeltaRequest {
                field: "payload",
                since: 8,
                after: None,
                limit: NonZeroUsize::MIN
            },
            NativeDeltaOutput {
                bytes: &mut next_bytes,
                changes: &mut next_changes
            }
        )
        .await,
        Err(NativeDeltaError::Interval)
    ));
    assert_eq!(next_bytes, saved);
    assert_eq!(next_changes.len(), 1);
    Ok(())
}
