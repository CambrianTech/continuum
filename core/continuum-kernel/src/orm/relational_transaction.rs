//! One committing owner for mutations, independent of the requesting task's lifetime.
use super::{query::StorageQuery, relational::RelationalStore, types::DataRecord};
use sea_orm::{DatabaseTransaction, TransactionTrait};
use std::{future::Future, pin::Pin};

type WriteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

impl RelationalStore {
    pub async fn write<T, F>(&self, operation: F) -> Result<T, String>
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

    pub async fn write_with_revisions<T, F>(&self, operation: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a RelationalStore,
                &'a DatabaseTransaction,
            ) -> WriteFuture<'a, (T, Vec<(String, u64)>)>
            + Send
            + 'static,
    {
        let store = self.clone();
        tokio::spawn(async move {
            let _guard = store.writer_gate.lock().await;
            let transaction = store.db.begin().await.map_err(|e| e.to_string())?;
            let (result, revisions) = operation(&store, &transaction).await?;
            transaction.commit().await.map_err(|e| e.to_string())?;
            for (collection, sequence) in revisions {
                store.committed(&collection, sequence);
            }
            Ok(result)
        })
        .await
        .map_err(|e| format!("storage transaction task failed: {e}"))?
    }

    pub async fn create(&self, record: DataRecord) -> Result<DataRecord, String> {
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
    ) -> Result<DataRecord, String> {
        self.write(move |store, transaction| {
            Box::pin(async move {
                if !store
                    .update_on(transaction, &collection, &id, data, increment)
                    .await?
                {
                    return Err(format!("update row {collection}:{id} does not exist"));
                }
                store
                    .read_on(transaction, &collection, &id)
                    .await?
                    .ok_or_else(|| format!("updated row {collection}:{id} disappeared"))
            })
        })
        .await
    }

    pub async fn upsert_monotonic(
        &self,
        record: DataRecord,
        field: String,
    ) -> Result<bool, String> {
        self.write(move |store, transaction| {
            Box::pin(async move { store.monotonic_on(transaction, &record, &field).await })
        })
        .await
    }

    pub async fn delete_matching(&self, query: StorageQuery) -> Result<usize, String> {
        self.write(move |store, transaction| {
            Box::pin(async move { store.delete_on(transaction, query).await })
        })
        .await
    }
}
