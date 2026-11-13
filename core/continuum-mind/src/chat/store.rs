//! The chat store's one door (#5696): every read and write of the chat handle goes through
//! [`ChatModule::store`]. It ensures the chat module's own collections before the first call
//! (nothing else creates them: opening a store adds only the TS-declared entities' indexes), sets
//! the chat handle, and turns a refusal into an error, so absence is only ever an empty answer from
//! a store that succeeded, never a failed read mistaken for "no row".

use super::chat_data_handle::{ChatModule, CHAT_DATA_HANDLE};
use super::orm_entities::CHAT_ORM_COLLECTIONS;
use crate::orm::types::BatchOperation;
use crate::runtime::{command_executor::CommandExecutor, LateBound};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

/// What every chat module built over one executor shares: the executor, installed once late at
/// boot (the first install wins and is never replaced, so `collections_ready` always describes the
/// store of the executor that holds the slot), and whether that store's chat collections exist yet
/// (ensured once, on first use; a failure retries).
pub struct ChatSlot {
    executor: LateBound<CommandExecutor>,
    collections_ready: tokio::sync::OnceCell<()>,
}

impl ChatSlot {
    pub fn new() -> Self {
        Self {
            executor: LateBound::new("chat::executor"),
            collections_ready: tokio::sync::OnceCell::new(),
        }
    }

    /// A slot that already holds `executor` (tests and injected executors).
    pub fn installed(executor: Arc<CommandExecutor>) -> Self {
        let slot = Self::new();
        slot.executor.install(executor);
        slot
    }

    /// Install the executor. The same executor again is a no-op; a DIFFERENT one is refused (Codex
    /// on #5698): the slot and the store it ensured stay the first install's.
    pub fn install(&self, executor: Arc<CommandExecutor>) -> Result<(), String> {
        self.executor.install(Arc::clone(&executor));
        match self.executor.get() {
            Some(held) if Arc::ptr_eq(held, &executor) => Ok(()),
            _ => Err(
                "chat slot: a different executor is already installed; the first install stays"
                    .to_string(),
            ),
        }
    }

    pub fn cloned(&self) -> Option<Arc<CommandExecutor>> {
        self.executor.cloned()
    }

    /// The executor once boot installs it. For boot-internal readers only (the inbound seam, ring
    /// hydration); a command never waits here, it fails loud before install (#201).
    pub async fn ready(&self) -> Result<Arc<CommandExecutor>, String> {
        Ok(self.executor.ready().await)
    }
}

impl Default for ChatSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatModule {
    /// Run a `data/*` command against the chat handle: the module's ONE way into its store.
    pub(super) async fn store(&self, command: &str, mut params: Value) -> Result<Value, String> {
        let executor = self.executor()?;
        self.executor_slot
            .collections_ready
            .get_or_try_init(|| ensure_chat_collections(&executor))
            .await?;
        params["dbPath"] = json!(CHAT_DATA_HANDLE);
        let result = executor
            .execute_json(command, params)
            .await
            .map_err(|e| format!("chat store: {command} failed: {e}"))?;
        match refusal(command, &result) {
            Some(refused) => Err(refused),
            None => Ok(result),
        }
    }

    /// The rows of `collection` matching `filter`, optionally sorted and limited.
    pub(super) async fn query_rows(
        &self,
        collection: &str,
        filter: Value,
        sort: Option<Value>,
        limit: Option<usize>,
    ) -> Result<Vec<Value>, String> {
        let mut query = json!({ "collection": collection, "filter": filter });
        if let Some(sort) = sort {
            query["sort"] = sort;
        }
        if let Some(limit) = limit {
            query["limit"] = json!(limit);
        }
        records(collection, self.store("data/query", query).await?)
    }

    /// Only the identities needed for bounded reconciliation, ordered by the store.
    pub(super) async fn query_ids(&self, collection: &str, filter: Value, limit: usize) -> Result<Vec<Uuid>, String> {
        let response = self.store("data/query", json!({
            "collection": collection, "filter": filter, "select": ["id"],
            "sort": [{"field": "id", "direction": "asc"}], "limit": limit,
        })).await?;
        records(collection, response)?.into_iter().map(|row| {
            let id = row.get("id").and_then(Value::as_str).ok_or("chat identity projection omitted id")?;
            Uuid::parse_str(id).map_err(|e| format!("chat identity projection: {e}"))
        }).collect()
    }
    /// The row `id` names in `collection`, `None` only when the store confirms there is none.
    pub(super) async fn row<T: serde::de::DeserializeOwned>(
        &self,
        collection: &str,
        id: Uuid,
    ) -> Result<Option<T>, String> {
        let rows = self
            .query_rows(
                collection,
                json!({ "id": { "$eq": id.to_string() } }),
                None,
                Some(1),
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let parsed = serde_json::from_value(row); // storage boundary: the adapter's row format
        parsed
            .map(Some)
            .map_err(|e| format!("chat store: malformed {collection} row {id}: {e}"))
    }

    /// Run `operations` as one atomic batch.
    pub(super) async fn run_batch(&self, operations: Vec<BatchOperation>) -> Result<Value, String> {
        self.store("data/batch", json!({ "operations": operations }))
            .await
    }
}

/// Create the chat module's own collections with their declared indexes (the arrival's unique
/// sequence and message id among them). Idempotent.
async fn ensure_chat_collections(executor: &Arc<CommandExecutor>) -> Result<(), String> {
    for collection in CHAT_ORM_COLLECTIONS {
        let params = json!({ "collection": collection, "dbPath": CHAT_DATA_HANDLE });
        let result = executor
            .execute_json("data/ensure-schema", params)
            .await
            .map_err(|e| format!("chat store: ensuring {collection} failed: {e}"))?;
        if let Some(refused) = refusal("data/ensure-schema", &result) {
            return Err(refused);
        }
    }
    Ok(())
}

/// The store's refusal of `command`, if it refused.
fn refusal(command: &str, result: &Value) -> Option<String> {
    if result.get("success").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let reason = result
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("success=false without an error"); // diagnostic text only: the malformed receipt still fails
    Some(format!("chat store: the store refused {command}: {reason}"))
}

/// The entities of a successful `data/query` response (`{ success, data: [{ id, data: <entity> }] }`).
/// Only a well-formed empty list means no rows: a missing list or a record without its entity is a
/// malformed answer, an error, never fewer rows (Codex on #5696).
fn records(collection: &str, mut query_result: Value) -> Result<Vec<Value>, String> {
    let malformed = |what: &str| format!("chat store: malformed {collection} query answer: {what}");
    let records = query_result
        .as_object_mut()
        .and_then(|fields| fields.remove("data"))
        .ok_or_else(|| malformed("no record list"))?;
    let Value::Array(records) = records else {
        return Err(malformed("no record list"));
    };
    records
        .into_iter()
        .map(|mut record| {
            record
                .as_object_mut()
                .and_then(|fields| fields.remove("data"))
                .ok_or_else(|| malformed("a record without its entity"))
        })
        .collect()
}

#[cfg(test)]
mod owned_rows_tests {
    use super::*;

    #[test]
    fn transcript_payload_moves_without_copying() {
        let payload = "transcript".repeat(4096);
        let allocation = payload.as_ptr();
        let mut row = serde_json::Map::new();
        row.insert("data".into(), Value::String(payload));
        let mut response = serde_json::Map::new();
        response.insert("data".into(), Value::Array(vec![Value::Object(row)]));
        let rows = records("chat_messages", Value::Object(response)).expect("test: valid row");
        assert_eq!(rows[0].as_str().expect("test: text").as_ptr(), allocation);
    }

    #[test]
    fn malformed_rows_are_not_silently_omitted() {
        for response in [json!({}), json!({"data": null}), json!({"data": [{}]}),
                         json!({"data": [{"data": 1}, null]})] {
            assert!(records("chat_messages", response).is_err());
        }
        assert_eq!(records("chat_messages", json!({"data": []})).expect("test: empty page"), Vec::<Value>::new());
    }
}
