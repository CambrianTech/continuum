//! A test adapter that can hold ONE write mid-flight, the way a real write runs.
//!
//! [`SeaOrmAdapter`](super::SeaOrmAdapter) runs every write on its detached transaction owner, so a
//! caller dropped while awaiting a write does not stop it: the write lands later, on its
//! own. [`GatedAdapter`] reproduces that shape deterministically. Every write runs on a
//! detached task; [`GatedAdapter::pause_next_write`] makes the next one wait at a gate
//! the test opens, so a test can cancel the caller, act, release, and observe what the
//! orphaned write did. Reads and schema calls pass straight through.

#![cfg(test)]

use super::storage_error::StorageError;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::future::BoxFuture;
use tokio::sync::{oneshot, Notify};

use super::adapter::{AdapterCapabilities, ClearAllResult, StorageAdapter};
use super::query::StorageQuery;
use super::seam::{BinaryWrite, BinaryWritten, Deleted, FieldRef, RowRef, Update, Written};
use super::types::{BatchOperation, BatchOutcome, CollectionSchema, CollectionStats, DataRecord, UUID};

/// The handles of one paused write: it signals `entered` when it reaches the gate,
/// waits for `release`, and signals `done` once the wrapped write has finished.
pub(crate) struct PausedWrite {
    pub entered: Arc<Notify>,
    pub release: Arc<Notify>,
    pub done: oneshot::Receiver<()>,
}

struct PendingWrite {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    done: oneshot::Sender<()>,
}

pub(crate) struct GatedAdapter {
    inner: Arc<dyn StorageAdapter>,
    next: Mutex<Option<PendingWrite>>,
}

impl GatedAdapter {
    pub(crate) fn new(inner: Arc<dyn StorageAdapter>) -> Self {
        Self {
            inner,
            next: Mutex::new(None),
        }
    }

    /// Hold the next write at a gate. Only that one write is held.
    pub(crate) fn pause_next_write(&self) -> PausedWrite {
        let (done_tx, done_rx) = oneshot::channel();
        let paused = PausedWrite {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            done: done_rx,
        };
        *self.next.lock().expect("gate lock") = Some(PendingWrite {
            entered: Arc::clone(&paused.entered),
            release: Arc::clone(&paused.release),
            done: done_tx,
        }); // test fixture: poison indicates a prior fixture panic
        paused
    }

    /// Run `write` on a detached task, held at the gate when one is armed.
    async fn write<R: Send + 'static>(
        &self,
        write: impl FnOnce(Arc<dyn StorageAdapter>) -> BoxFuture<'static, R> + Send + 'static,
    ) -> R {
        let gate = self.next.lock().expect("gate lock").take(); // test fixture: a poisoned gate means an earlier test thread panicked
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            if let Some(gate) = &gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            let out = write(inner).await;
            if let Some(gate) = gate {
                let _ = gate.done.send(()); // The observer may have been cancelled.
            }
            out
        })
        .await
        .expect("detached write task") // test fixture: a panicking write fails the test that caused it
    }
}

#[async_trait]
impl StorageAdapter for GatedAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn capabilities(&self) -> AdapterCapabilities {
        self.inner.capabilities()
    }
    async fn close(&mut self) -> Result<(), StorageError> {
        Err(StorageError::Unsupported { adapter: "gated fixture", operation: "close" })
    }
    async fn create(&self, record: DataRecord) -> Result<DataRecord, StorageError> {
        self.write(move |a| Box::pin(async move { a.create(record).await })).await
    }
    async fn read(&self, row: RowRef<'_>) -> Result<DataRecord, StorageError> {
        self.inner.read(row).await
    }
    async fn query(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError> {
        self.inner.query(query).await
    }
    async fn scan(
        &self,
        query: &StorageQuery,
    ) -> Result<futures::stream::BoxStream<'_, Result<DataRecord, StorageError>>, StorageError> {
        self.inner.scan(query).await
    }
    async fn query_with_join(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError> {
        self.inner.query_with_join(query).await
    }
    async fn count(&self, query: &StorageQuery) -> Result<usize, StorageError> {
        self.inner.count(query).await
    }
    async fn update(&self, update: Update<'_>) -> Result<DataRecord, StorageError> {
        let (collection, id) = (update.row.collection.to_owned(), update.row.id.clone());
        let (data, versioning) = (update.data, update.versioning);
        self.write(move |a| {
            Box::pin(async move { a.update(Update { row: RowRef { collection: &collection, id: &id }, data, versioning }).await })
        })
        .await
    }
    async fn delete(&self, row: RowRef<'_>) -> Result<Deleted, StorageError> {
        let (collection, id) = (row.collection.to_owned(), row.id.clone());
        self.write(move |a| Box::pin(async move { a.delete(RowRef { collection: &collection, id: &id }).await })).await
    }
    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> Result<Written, StorageError> {
        let field = field.to_owned();
        self.write(move |a| Box::pin(async move { a.upsert_monotonic(record, &field).await })).await
    }
    async fn delete_matching(&self, query: StorageQuery) -> Result<usize, StorageError> {
        self.write(move |a| Box::pin(async move { a.delete_matching(query).await })).await
    }
    async fn batch(&self, operations: Vec<BatchOperation>) -> Result<Vec<BatchOutcome>, StorageError> {
        self.write(move |a| Box::pin(async move { a.batch(operations).await })).await
    }
    fn sequence_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.inner.sequence_revision(collection)
    }
    fn native_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.inner.native_revision(collection)
    }
    async fn ensure_schema(&self, schema: &CollectionSchema) -> Result<(), StorageError> {
        self.inner.ensure_schema(schema).await
    }
    fn collection_schema(&self, collection: &str) -> Result<Arc<CollectionSchema>, StorageError> {
        self.inner.collection_schema(collection)
    }
    async fn list_collections(&self) -> Result<Vec<String>, StorageError> {
        self.inner.list_collections().await
    }
    async fn binary_snapshot(&self, collection: &str) -> Result<super::BinarySnapshot, StorageError> {
        self.inner.binary_snapshot(collection).await
    }
    async fn create_binary(
        &self,
        record: DataRecord,
        payloads: std::collections::BTreeMap<String, super::binary::BinaryValue>,
    ) -> Result<UUID, StorageError> {
        self.write(move |a| Box::pin(async move { a.create_binary(record, payloads).await })).await
    }
    async fn update_binary(&self, write: BinaryWrite<'_>) -> Result<BinaryWritten, StorageError> {
        let (collection, id) = (write.row.collection.to_owned(), write.row.id.clone());
        let (payloads, condition) = (write.payloads, write.condition);
        self.write(move |a| {
            Box::pin(async move {
                a.update_binary(BinaryWrite { row: RowRef { collection: &collection, id: &id }, payloads, condition }).await
            })
        })
        .await
    }
    async fn numeric_array_stats(&self, field: FieldRef<'_>) -> Result<super::adapter::NumericArrayStats, StorageError> {
        self.inner.numeric_array_stats(field).await
    }
    async fn collection_stats(&self, collection: &str) -> Result<CollectionStats, StorageError> {
        self.inner.collection_stats(collection).await
    }
    async fn truncate(&self, collection: &str) -> Result<usize, StorageError> {
        let collection = collection.to_owned();
        self.write(move |a| Box::pin(async move { a.truncate(&collection).await })).await
    }
    async fn clear_all(&self) -> Result<ClearAllResult, StorageError> {
        self.write(move |a| Box::pin(async move { a.clear_all().await })).await
    }
    async fn cleanup(&self) -> Result<(), StorageError> {
        self.inner.cleanup().await
    }
}

#[tokio::test]
async fn detached_panic_is_a_terminal_completion_error() {
    let (inner, _directory) = super::store::fresh_adapter().await;
    let adapter = Arc::new(GatedAdapter::new(inner));
    let gate = adapter.pause_next_write();
    let request = tokio::spawn(async move {
        adapter
            .write::<()>(|_| Box::pin(async { panic!("injected write panic") }))
            .await;
    });
    gate.entered.notified().await;
    request.abort();
    assert!(request.await.is_err());
    gate.release.notify_one();
    assert!(
        gate.done.await.is_err(),
        "producer death must not look like successful completion"
    );
}
