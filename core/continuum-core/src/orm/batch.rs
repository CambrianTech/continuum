//! Typed, all-or-nothing writes that span entity types.
//!
//! [`OrmStore<T>`](super::store::OrmStore) is generic over ONE entity, so it
//! cannot express "write this parent and its children, or write nothing" —
//! the parent and the children are different types. [`OrmBatch`] is that
//! missing surface: each staged operation is typed at the call site, and the
//! whole set commits through [`StorageAdapter::batch`], which is atomic on
//! every adapter.
//!
//! ## Why this exists rather than two `save` calls
//!
//! Two sequential `save`s can leave the parent committed and the children
//! absent — a row asserting a relationship that no longer has a counterpart.
//! For card 0d51573a that shape is specifically load-bearing: a `StagedCredit`
//! whose `StagedCreditGeneration` rows were lost is a credit record claiming
//! provenance it cannot produce, which is the exact defect the card exists to
//! prevent.
//!
//! ## Schemas are NOT ensured here
//!
//! `OrmStore::new` is what calls `ensure_schema`; this type deliberately does
//! not, because it has no single `T` to derive a schema from. **Construct the
//! `OrmStore` for each entity type you batch before committing one.** It
//! matters more than it looks: sqlite's create path auto-creates a missing
//! table from the data's shape, so a batch against an unregistered collection
//! SUCCEEDS while silently producing a table with none of the entity's
//! declared indexes — including unique ones. A dropped unique index is not a
//! visible failure, it is a constraint that stops being enforced.

use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use super::adapter::StorageAdapter;
use super::entity::OrmEntity;
use super::store::OrmStoreError;
use super::types::{BatchOperation, BatchOperationType};

/// A set of typed writes that commit together or not at all.
///
/// Built by chaining; nothing touches storage until [`commit`](Self::commit).
pub struct OrmBatch {
    adapter: Arc<dyn StorageAdapter>,
    operations: Vec<BatchOperation>,
}

impl OrmBatch {
    /// Start an empty batch against an adapter.
    pub fn new(adapter: Arc<dyn StorageAdapter>) -> Self {
        Self {
            adapter,
            operations: Vec::new(),
        }
    }

    /// Stage a create. Typed per call, which is what lets one batch span
    /// entity types — the collection comes from `T`, never from a string the
    /// caller passes.
    pub fn create<T>(mut self, id: Uuid, entity: &T) -> Result<Self, OrmStoreError>
    where
        T: OrmEntity + Serialize,
    {
        // Serialized the same way OrmStore::save does, so a row written through
        // a batch is byte-identical to one written singly.
        let data = serde_json::to_value(entity).map_err(OrmStoreError::SerializeFailed)?;
        self.operations.push(BatchOperation {
            operation_type: BatchOperationType::Create,
            collection: T::COLLECTION.to_string(),
            id: Some(id.to_string()),
            data: Some(data),
        });
        Ok(self)
    }

    /// Stage an update.
    pub fn update<T>(mut self, id: Uuid, entity: &T) -> Result<Self, OrmStoreError>
    where
        T: OrmEntity + Serialize,
    {
        let data = serde_json::to_value(entity).map_err(OrmStoreError::SerializeFailed)?;
        self.operations.push(BatchOperation {
            operation_type: BatchOperationType::Update,
            collection: T::COLLECTION.to_string(),
            id: Some(id.to_string()),
            data: Some(data),
        });
        Ok(self)
    }

    /// Stage a delete. No serialization, so this cannot fail.
    pub fn delete<T: OrmEntity>(mut self, id: Uuid) -> Self {
        self.operations.push(BatchOperation {
            operation_type: BatchOperationType::Delete,
            collection: T::COLLECTION.to_string(),
            id: Some(id.to_string()),
            data: None,
        });
        self
    }

    /// How many operations are staged.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Whether nothing is staged.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Commit every staged operation as one unit.
    ///
    /// An empty batch is a no-op that does NOT reach the adapter — committing
    /// nothing is success, and opening a transaction to do nothing is waste.
    pub async fn commit(self) -> Result<(), OrmStoreError> {
        if self.operations.is_empty() {
            return Ok(());
        }

        // Named for the error message before the vec is consumed.
        let mut collections: Vec<&str> =
            self.operations.iter().map(|o| o.collection.as_str()).collect();
        collections.sort_unstable();
        collections.dedup();
        let collections = collections.join(", ");

        let result = self.adapter.batch(self.operations).await;
        if result.success {
            Ok(())
        } else {
            Err(OrmStoreError::AdapterFailed {
                operation: "batch",
                collection: collections,
                detail: result
                    .error
                    .unwrap_or_else(|| "adapter reported failure with no detail".to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::entity::OrmEntity;
    use crate::orm::sqlite::SqliteAdapter;
    use crate::orm::store::OrmStore;
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::types::{CollectionSchema, FieldType, SchemaField};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Parent {
        label: String,
    }
    impl OrmEntity for Parent {
        const COLLECTION: &'static str = "batch_parent";
        fn collection_schema() -> CollectionSchema {
            CollectionSchema {
                collection: Self::COLLECTION.to_string(),
                fields: vec![SchemaField {
                    name: "label".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            }
        }
    }

    // camelCase to match the declared schema field, exactly as #[derive(Entity)]
    // types do. The adapter round-trips column names, so a struct whose serde
    // naming disagrees with its schema loses the field on read.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Child {
        parent_id: String,
    }
    impl OrmEntity for Child {
        const COLLECTION: &'static str = "batch_child";
        fn collection_schema() -> CollectionSchema {
            CollectionSchema {
                collection: Self::COLLECTION.to_string(),
                fields: vec![SchemaField {
                    name: "parentId".to_string(),
                    field_type: FieldType::String,
                    indexed: true,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            }
        }
    }

    async fn adapter() -> (Arc<dyn StorageAdapter>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut a = SqliteAdapter::new();
        a.initialize(AdapterConfig {
            connection_string: dir.path().join("batch.db").to_str().unwrap().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
        (Arc::new(a), dir)
    }

    // what this catches: a cross-type batch silently degrading into per-row writes.
    // The whole reason OrmBatch exists is that OrmStore<T> is single-type, so a parent
    // and its children cannot be written together through it. If commit() ever stopped
    // going through the atomic adapter.batch — or if someone "simplified" it into a
    // loop of saves — a failing child would leave the parent committed, which for card
    // 0d51573a is a credit row asserting provenance whose receipts are gone.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_batch_spanning_two_entity_types_commits_as_one_unit() {
        let (adapter, _dir) = adapter().await;
        let parents = OrmStore::<Parent>::new(Arc::clone(&adapter)).await.unwrap();
        let children = OrmStore::<Child>::new(Arc::clone(&adapter)).await.unwrap();

        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        OrmBatch::new(Arc::clone(&adapter))
            .create(
                parent_id,
                &Parent {
                    label: "credit".to_string(),
                },
            )
            .unwrap()
            .create(
                child_id,
                &Child {
                    parent_id: parent_id.to_string(),
                },
            )
            .unwrap()
            .commit()
            .await
            .expect("a well-formed cross-type batch must commit");

        // BOTH types landed — proving the batch really spanned them.
        assert_eq!(
            parents.find_by_id(parent_id).await.unwrap(),
            Some(Parent {
                label: "credit".to_string()
            }),
            "the parent row is missing after a committed batch"
        );
        assert_eq!(
            children.find_by_id(child_id).await.unwrap(),
            Some(Child {
                parent_id: parent_id.to_string()
            }),
            "the child row is missing after a committed batch"
        );

        // A duplicate primary key fails the second operation, so the FIRST must not
        // survive. This is the rollback the type exists to guarantee, exercised
        // through the typed surface rather than the adapter directly.
        let survivor_id = Uuid::new_v4();
        let err = OrmBatch::new(Arc::clone(&adapter))
            .create(
                survivor_id,
                &Parent {
                    label: "rolled-back".to_string(),
                },
            )
            .unwrap()
            .create(
                parent_id, // already exists — collides
                &Parent {
                    label: "collision".to_string(),
                },
            )
            .unwrap()
            .commit()
            .await;
        assert!(err.is_err(), "a batch with a colliding id must not report success");
        assert_eq!(
            parents.find_by_id(survivor_id).await.unwrap(),
            None,
            "the first row survived a failed batch — commit() is not atomic"
        );

        // POSITIVE CONTROL: the pre-existing parent is untouched by the rollback,
        // so the assertion above is about the failed batch and not about the store
        // being empty or unreadable.
        assert!(
            parents.find_by_id(parent_id).await.unwrap().is_some(),
            "the rollback must not have removed rows committed by an earlier batch"
        );
    }

    // what this catches: an empty batch opening a transaction (or erroring) instead of
    // being a no-op. Callers stage conditionally — a turn with no receipts stages no
    // children — and an empty commit must be success without touching storage.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_empty_batch_commits_without_touching_the_adapter() {
        let (adapter, _dir) = adapter().await;
        let batch = OrmBatch::new(Arc::clone(&adapter));
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        batch.commit().await.expect("an empty batch is trivially successful");
    }
}
