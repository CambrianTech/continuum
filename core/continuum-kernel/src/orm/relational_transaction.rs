//! One committing owner for mutations, independent of the requesting task's lifetime.
use super::storage_error::StorageError;
use super::{query::StorageQuery, relational::RelationalStore, types::DataRecord};
use sea_orm::{DatabaseTransaction, TransactionTrait};
use std::{future::Future, pin::Pin};

type WriteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;

impl RelationalStore {
    pub async fn write<T, F>(&self, operation: F) -> Result<T, StorageError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a RelationalStore, &'a DatabaseTransaction) -> WriteFuture<'a, T>
            + Send
            + 'static,
    {
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move { Ok((operation(store, transaction).await?, Vec::new())) })
        })
        .await
    }

    pub async fn write_with_revisions<T, F>(&self, operation: F) -> Result<T, StorageError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a RelationalStore,
                &'a DatabaseTransaction,
            ) -> WriteFuture<'a, (T, Vec<Revision>)>
            + Send
            + 'static,
    {
        let store = self.clone();
        tokio::spawn(async move {
            let _guard = store.writer_gate.lock().await;
            let transaction = store.db.begin().await?;
            let (result, revisions) = operation(&store, &transaction).await?;
            transaction.commit().await?;
            for revision in revisions {
                store.committed(revision.domain, &revision.collection, revision.high);
            }
            Ok(result)
        })
        .await
        ?
    }

    pub async fn create(&self, record: DataRecord) -> Result<DataRecord, StorageError> {
        self.write(move |store, transaction| {
            Box::pin(async move {
                store.insert(transaction, &record).await?;
                Ok(record)
            })
        })
        .await
    }

    pub async fn update(
        &self,
        collection: String,
        id: String,
        data: serde_json::Value,
        increment: bool,
    ) -> Result<DataRecord, StorageError> {
        self.write(move |store, transaction| {
            Box::pin(async move {
                if !store
                    .update_on(transaction, &collection, &id, data, increment)
                    .await?
                {
                    return Err(StorageError::NotFound { collection: collection.to_string(), id: id.clone() });
                }
                store
                    .read_on(transaction, &collection, &id)
                    .await?
                    .ok_or(StorageError::Corrupt { what: "an updated row vanished inside its own transaction" })
            })
        })
        .await
    }

    pub async fn upsert_monotonic(
        &self,
        record: DataRecord,
        field: String,
    ) -> Result<bool, StorageError> {
        self.write(move |store, transaction| {
            Box::pin(async move { store.monotonic_on(transaction, &record, &field).await })
        })
        .await
    }

    pub async fn delete_matching(&self, query: StorageQuery) -> Result<usize, StorageError> {
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                let deleted = store.delete_on(transaction, query).await?;
                Ok((deleted.count, deleted.revisions))
            })
        })
        .await
    }
}

pub(super) struct Revision {
    pub domain: super::relational_sequence::Domain,
    pub collection: String,
    pub high: u64,
}
pub(super) struct Deletion {
    pub count: usize,
    pub revisions: Vec<Revision>,
}
