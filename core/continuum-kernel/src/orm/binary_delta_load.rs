#![cfg(test)]

//! Sparse-update replay against the existing 10,003-row isolated load fixture.
use super::*;
use crate::orm::binary::{BinaryPageOutput, BinaryPageRequest, BinaryWriteCondition};
use crate::orm::binary_delta::{NativeDeltaOutput, NativeDeltaRequest, NativeDeltaValue};
use std::{collections::BTreeMap, num::NonZeroUsize};

pub(super) async fn replay(
    store: &RelationalStore,
    descriptor: &BinaryDescriptor,
    original: &[u8],
) -> Result<(), String> {
    // The existing corpus is already cached by the baseline's full scan. Its initial
    // fixture inserts have no revision receipts; this is incremental sync, not bootstrap.
    // Admit the rows participating in this mutation schedule through the real native
    // owner before capturing the cached revision. Unchanged fixture rows stay cached.
    for id in 0..32 {
        store
            .update_binary(
                Native::COLLECTION.into(),
                uuid::Uuid::from_u128(id).to_string(),
                BTreeMap::from([(
                    "payload".into(),
                    BinaryValue::new(descriptor.clone(), original.to_vec())?,
                )]),
                BinaryWriteCondition::Replace,
            )
            .await?;
    }
    let since = store.binary_snapshot(Native::COLLECTION).await?.revision();
    let mut updated = original.to_vec();
    *updated.first_mut().ok_or("fixture payload is empty")? ^= 1;
    for id in 0..28 {
        store
            .update_binary(
                Native::COLLECTION.into(),
                uuid::Uuid::from_u128(id).to_string(),
                BTreeMap::from([(
                    "payload".into(),
                    BinaryValue::new(descriptor.clone(), updated.clone())?,
                )]),
                BinaryWriteCondition::Replace,
            )
            .await?;
    }
    let deleted = (28..32)
        .map(|id| uuid::Uuid::from_u128(id).to_string().into())
        .collect();
    assert_eq!(
        store
            .delete_matching(
                QueryBuilder::new(Native::COLLECTION)
                    .filter("id", QueryOperator::In(deleted))
                    .build()
            )
            .await?,
        4
    );
    let snapshot = store.binary_snapshot(Native::COLLECTION).await?;
    let limit = NonZeroUsize::new(256).ok_or("positive replay page size")?;
    for delta in [false, true, true, false] {
        let start = Instant::now();
        let mut transferred = 0usize;
        let mut selected = 0usize;
        let mut bytes = Vec::new();
        if delta {
            let mut changes = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            let mut after = None;
            loop {
                bytes.clear();
                changes.clear();
                after = snapshot
                    .changes_into(
                        &NativeDeltaRequest {
                            field: "payload",
                            since,
                            after,
                            limit,
                        },
                        NativeDeltaOutput {
                            bytes: &mut bytes,
                            changes: &mut changes,
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                for change in &changes {
                    let id = change.cursor.id.as_u128();
                    assert!(seen.insert(id), "duplicate delta identity");
                    match &change.value {
                        NativeDeltaValue::Upsert {
                            descriptor: found,
                            bytes: range,
                        } => {
                            assert!(id < 28);
                            assert_eq!(found, descriptor);
                            assert_eq!(&bytes[range.clone()], updated.as_slice());
                        }
                        NativeDeltaValue::Remove => assert!((28..32).contains(&id)),
                    }
                }
                selected += changes.len();
                transferred += bytes.len();
                if after.is_none() {
                    break;
                }
            }
            assert_eq!(selected, 32);
            assert_eq!(seen, (0..32).collect());
            assert_eq!(transferred, 28 * original.len());
        } else {
            let mut ids = Vec::new();
            let mut after = None;
            loop {
                bytes.clear();
                ids.clear();
                let found = snapshot
                    .page_into(
                        &BinaryPageRequest {
                            field: "payload",
                            after,
                            limit,
                        },
                        BinaryPageOutput {
                            bytes: &mut bytes,
                            ids: &mut ids,
                        },
                    )
                    .await?;
                assert_eq!(
                    found.as_ref(),
                    if ids.is_empty() {
                        None
                    } else {
                        Some(descriptor)
                    }
                );
                for (id, row) in ids.iter().zip(bytes.chunks_exact(original.len())) {
                    let id = id.as_u128();
                    assert!(!(28..32).contains(&id));
                    assert_eq!(
                        row,
                        if id < 28 {
                            updated.as_slice()
                        } else {
                            original
                        }
                    );
                }
                selected += ids.len();
                transferred += bytes.len();
                if ids.len() < limit.get() {
                    break;
                }
                after = ids.last().copied();
            }
            assert_eq!(selected, 10003 - 4);
            assert_eq!(transferred, (10003 - 4) * original.len());
        }
        println!("NATIVE_SYNC_REPLAY kind={} initial_rows=10003 updates=28 deletes=4 selected={} payload_bytes={} elapsed_ms={:.3} state=filesystem_warm snapshot_acquisition_excluded=true validation_included=true", if delta { "delta" } else { "full" }, selected, transferred, start.elapsed().as_secs_f64()*1000.0);
    }
    Ok(())
}
