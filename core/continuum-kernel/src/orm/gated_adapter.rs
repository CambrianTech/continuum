//! A test adapter that can hold ONE write mid-flight, the way a real write runs.
//!
//! [`SqliteAdapter`](super::SqliteAdapter) runs every write on `spawn_blocking`, so a
//! caller dropped while awaiting a write does not stop it: the write lands later, on its
//! own. [`GatedAdapter`] reproduces that shape deterministically. Every write runs on a
//! detached task; [`GatedAdapter::pause_next_write`] makes the next one wait at a gate
//! the test opens, so a test can cancel the caller, act, release, and observe what the
//! orphaned write did. Reads and schema calls pass straight through.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::Notify;

use super::adapter::{AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter};
use super::query::StorageQuery;
use super::types::{BatchOperation, CollectionSchema, CollectionStats, DataRecord, StorageResult, UUID};

/// The handles of one paused write: it signals `entered` when it reaches the gate,
/// waits for `release`, and signals `done` once the wrapped write has finished.
#[derive(Clone)]
pub(crate) struct PausedWrite {
    pub entered: Arc<Notify>,
    pub release: Arc<Notify>,
    pub done: Arc<Notify>,
}

pub(crate) struct GatedAdapter {
    inner: Arc<dyn StorageAdapter>,
    next: Mutex<Option<PausedWrite>>,
    unwind_next: std::sync::atomic::AtomicBool,
    fail_after_create: std::sync::atomic::AtomicBool,
}

impl GatedAdapter {
    pub(crate) fn new(inner: Arc<dyn StorageAdapter>) -> Self {
        Self { inner, next: Mutex::new(None), unwind_next: std::sync::atomic::AtomicBool::new(false), fail_after_create: std::sync::atomic::AtomicBool::new(false) }
    }

    pub(crate) fn unwind_next_write(&self) {
        self.unwind_next.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn fail_after_next_create(&self) {
        self.fail_after_create.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Hold the next write at a gate. Only that one write is held.
    pub(crate) fn pause_next_write(&self) -> PausedWrite {
        let paused = PausedWrite {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            done: Arc::new(Notify::new()),
        };
        *self.next.lock().expect("gate lock") = Some(paused.clone()); // test fixture: a poisoned gate means an earlier test thread panicked
        paused
    }

    /// Run `write` on a detached task, held at the gate when one is armed.
    async fn write<R: Send + 'static>(
        &self,
        write: impl FnOnce(Arc<dyn StorageAdapter>) -> BoxFuture<'static, R> + Send + 'static,
    ) -> R {
        if self.unwind_next.swap(false, std::sync::atomic::Ordering::SeqCst) {
            std::panic::resume_unwind(Box::new("injected storage unwind"));
        }
        let gate = self.next.lock().expect("gate lock").take(); // test fixture: a poisoned gate means an earlier test thread panicked
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            if let Some(gate) = &gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            let out = write(inner).await;
            if let Some(gate) = &gate {
                gate.done.notify_one();
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
        "gated"
    }
    fn capabilities(&self) -> AdapterCapabilities {
        self.inner.capabilities()
    }
    async fn initialize(&mut self, _config: AdapterConfig) -> Result<(), String> {
        Err("the gated fixture wraps an adapter that is already initialized".to_string())
    }
    async fn close(&mut self) -> Result<(), String> {
        Err("the gated fixture does not own its adapter".to_string())
    }
    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let fail = self.fail_after_create.swap(false, std::sync::atomic::Ordering::SeqCst);
        let result = self.write(move |a| Box::pin(async move { a.create(record).await })).await;
        if fail && result.success {
            StorageResult::err("injected loss of write acknowledgement after commit")
        } else {
            result
        }
    }
    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        self.inner.read(collection, id).await
    }
    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        self.inner.query(query).await
    }
    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        self.inner.query_with_join(query).await
    }
    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        self.inner.count(query).await
    }
    async fn update(&self, collection: &str, id: &UUID, data: Value, increment_version: bool) -> StorageResult<DataRecord> {
        let (collection, id) = (collection.to_string(), id.clone());
        self.write(move |a| Box::pin(async move { a.update(&collection, &id, data, increment_version).await }))
            .await
    }
    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let (collection, id) = (collection.to_string(), id.clone());
        self.write(move |a| Box::pin(async move { a.delete(&collection, &id).await })).await
    }
    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        self.write(move |a| Box::pin(async move { a.batch(operations).await })).await
    }
    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> StorageResult<bool> {
        let field = field.to_string();
        self.write(move |a| Box::pin(async move { a.upsert_monotonic(record, &field).await })).await
    }
    async fn delete_matching(&self, query: StorageQuery) -> StorageResult<usize> {
        self.write(move |a| Box::pin(async move { a.delete_matching(query).await })).await
    }
    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool> {
        self.inner.ensure_schema(schema).await
    }
    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        self.inner.list_collections().await
    }
    async fn collection_stats(&self, collection: &str) -> StorageResult<CollectionStats> {
        self.inner.collection_stats(collection).await
    }
    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        let collection = collection.to_string();
        self.write(move |a| Box::pin(async move { a.truncate(&collection).await })).await
    }
    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        self.write(move |a| Box::pin(async move { a.clear_all().await })).await
    }
    async fn cleanup(&self) -> Result<(), String> {
        self.inner.cleanup().await
    }
}
