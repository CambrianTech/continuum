//! A detached committing task owns the transaction and its revision publication.
use super::storage_error::StorageError;
use super::{
    query::QueryBuilder,
    types::{Appended, BatchOperation, BatchOutcome, DataRecord, RecordMetadata, RowWrite},
};

fn record(row: RowWrite) -> DataRecord {
    DataRecord { id: row.id, collection: row.collection, data: row.data, metadata: RecordMetadata::default() }
}

impl super::relational::RelationalStore {
    /// Run `operations` in one write transaction: all commit, or none do. The operations are owned
    /// because the committing task outlives the caller's borrow and moves each record into its write.
    pub async fn batch(&self, operations: Vec<BatchOperation>) -> Result<Vec<BatchOutcome>, StorageError> {
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                let mut outcomes = Vec::with_capacity(operations.len());
                let mut committed = Vec::new();
                for operation in operations {
                    let outcome = match operation {
                        BatchOperation::Read(row) => {
                            let record = store
                                .read_on(transaction, &row.collection, &row.id)
                                .await?
                                .ok_or(StorageError::NotFound { collection: row.collection.into(), id: row.id })?;
                            BatchOutcome::Read { record }
                        }
                        BatchOperation::Delete(row) => {
                            let deleted = store
                                // bounded: deletes the one row with this primary key, in the store, loading none
                                .delete_on(transaction, QueryBuilder::new(&row.collection).filter_eq("id", row.id).build())
                                .await?;
                            committed.extend(deleted.revisions);
                            if deleted.count == 0 { BatchOutcome::Absent } else { BatchOutcome::Removed }
                        }
                        BatchOperation::Update(row) => {
                            if !store.update_on(transaction, super::seam::RowRef { collection: &row.collection, id: &row.id }, row.data, super::seam::Versioning::Increment).await? {
                                return Err(StorageError::NotFound { collection: row.collection.into(), id: row.id });
                            }
                            BatchOutcome::Updated
                        }
                        BatchOperation::Create(row) => {
                            let record = record(row);
                            store.insert(transaction, &record).await?;
                            BatchOutcome::Created { record }
                        }
                        BatchOperation::Ensure(row) => {
                            if store.ensure_on(transaction, &record(row)).await? {
                                BatchOutcome::Ensured
                            } else {
                                BatchOutcome::AlreadyPresent
                            }
                        }
                        BatchOperation::Append { row, spec } => {
                            let collection = row.collection.clone();
                            let appended = store.append_on(transaction, record(row), &spec).await?;
                            if let Appended::Fresh(high) = appended {
                                committed.push(super::relational_transaction::Revision {
                                    domain: super::relational_sequence::Domain::Append,
                                    collection,
                                    high,
                                });
                            }
                            BatchOutcome::Appended { appended }
                        }
                    };
                    outcomes.push(outcome);
                }
                Ok((outcomes, committed))
            })
        })
        .await
    }
}
