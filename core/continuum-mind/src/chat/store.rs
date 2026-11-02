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

/// What every chat module built over one executor shares: the late-bound executor, and whether its
/// store's chat collections exist yet (ensured once, on first use; a failure retries on the next).
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

    pub fn install(&self, executor: Arc<CommandExecutor>) {
        self.executor.install(executor);
    }

    pub fn cloned(&self) -> Option<Arc<CommandExecutor>> {
        self.executor.cloned()
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
        records(collection, &self.store("data/query", query).await?)
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
        .unwrap_or("success=false without an error");
    Some(format!("chat store: the store refused {command}: {reason}"))
}

/// The entities of a successful `data/query` response (`{ success, data: [{ id, data: <entity> }] }`).
/// Only a well-formed empty list means no rows: a missing list or a record without its entity is a
/// malformed answer, an error, never fewer rows (Codex on #5696).
fn records(collection: &str, query_result: &Value) -> Result<Vec<Value>, String> {
    let malformed = |what: &str| format!("chat store: malformed {collection} query answer: {what}");
    let records = query_result
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("no record list"))?;
    records
        .iter()
        .map(|record| {
            record
                .get("data")
                .cloned()
                .ok_or_else(|| malformed("a record without its entity"))
        })
        .collect()
}
