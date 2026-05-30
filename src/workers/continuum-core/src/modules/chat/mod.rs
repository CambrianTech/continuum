//! ChatModule — first proof-of-pattern module migration.
//!
//! Per Joel's directive:
//! > "Chat is gonna be airc man. So that's extracted period. Chat is of
//! > course a bonafide command though. Do not cheapen it. So the
//! > commands need to be or at least some to start, entirely rust."
//!
//! The split:
//! - **Substrate** (delivery, pub/sub, peers, signing) → airc.
//! - **Commands** (`chat/send`, `chat/poll`, `chat/analyze`, `chat/export`)
//!   → Continuum kernel-level ServiceModule, this module.
//!
//! This is the FIRST real module migration from a TS command to a
//! Rust `ServiceModule`, following every pattern the substrate floor
//! established in the recent PRs:
//! - `ServiceModule` trait (PR #1471)
//! - `CommandResult` cell shapes (PR #1485)
//! - `CommandRequest<P>` / `CommandResponse<T>` envelopes (PR #1486)
//! - Architecture from `docs/architecture/MODULE-ARCHITECTURE.md` (PR #1482)
//! - Scaffold shape from `GeneratorModule` (PR #1487)
//!
//! # Scope of this PR
//!
//! Only `chat/poll` ships in Rust today. The other three commands
//! (`chat/send`, `chat/analyze`, `chat/export`) are wired into the
//! dispatch table as fail-loud stubs that name follow-up PRs. The
//! TS implementations stay live on canary so consumers see no
//! regression; the kernel will start owning each command as its
//! follow-up PR lands.
//!
//! The reason for the staged migration: `chat/poll` is the cleanest
//! outlier (pure read, no airc, no media side-effects) which lets us
//! validate the cross-module call pattern (chat → data via the kernel
//! executor) without dragging substrate + media into the first
//! migration. Subsequent commands fold in real behavior incrementally.
//!
//! # Cross-module call pattern
//!
//! `chat/poll` doesn't open a database connection itself — it calls
//! `data/query` via the kernel executor (the same global executor any
//! other module reaches for at call time). Chat is blind to which
//! adapter implements the storage; the data module routes the query
//! per its own resolution rules. This is exactly the composition
//! pattern from `MODULE-ARCHITECTURE.md` §5: commands call commands;
//! modules don't know about each other beyond the command surface.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::runtime::{
    command_executor::{self, CommandExecutor},
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

pub mod types;

use types::{ChatPollParams, ChatPollResult, CHAT_MESSAGES_COLLECTION, DEFAULT_POLL_LIMIT};

/// The chat module. Owns the `chat/*` (and back-compat
/// `collaboration/chat/*`) command surface.
///
/// Stateless apart from an optional executor override used by tests to
/// inject a mocked dispatch chain — production wiring uses the global
/// kernel executor. The override lives behind an `RwLock<Option<...>>`
/// so it's set once at construction and read on the hot path; the
/// `RwLock` choice over `Mutex` is purely for read-side concurrency
/// when multiple commands fire concurrently.
pub struct ChatModule {
    /// Optional executor override. `None` in production — reads default
    /// to `command_executor::executor()` (the kernel-global).
    /// `Some(...)` in tests so each test can spin up its own registry
    /// without trampling the global `OnceLock`.
    executor_override: RwLock<Option<Arc<CommandExecutor>>>,
}

impl ChatModule {
    /// Construct a chat module that uses the kernel-global executor.
    /// This is the production constructor — register the resulting
    /// module at runtime startup with `Arc::new(ChatModule::new())`.
    pub fn new() -> Self {
        Self {
            executor_override: RwLock::new(None),
        }
    }

    /// Test-only constructor — inject an explicit executor instance so
    /// the test owns its dispatch chain (commonly a registry with a
    /// stub DataModule). Lets the chat module's tests exercise the
    /// real cross-module call path without standing up the global
    /// `OnceLock`.
    #[cfg(test)]
    pub fn with_executor(executor: Arc<CommandExecutor>) -> Self {
        Self {
            executor_override: RwLock::new(Some(executor)),
        }
    }

    /// Resolve the executor for the current call. Tests get the
    /// injected one; production gets the kernel-global.
    fn executor(&self) -> Arc<CommandExecutor> {
        if let Some(ex) = self
            .executor_override
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return ex;
        }
        command_executor::executor()
    }

    /// `chat/poll` — return recent messages, optionally filtered by
    /// room or anchored after a specific message id.
    ///
    /// Implementation strategy (mirrors the TS `ChatPollServerCommand`
    /// behavior):
    ///
    /// 1. If `after_message_id` is set: look up that message's
    ///    timestamp via `data/query` (limit 1, filter on id), use it as
    ///    a `$gt` filter on the main query.
    /// 2. Apply optional `room_id` filter.
    /// 3. Sort `asc` when polling after an anchor (chronological), else
    ///    `desc` (latest-N).
    /// 4. Query via `data/query` against the `chat_messages` collection.
    /// 5. Normalize back to chronological order for display regardless
    ///    of query direction.
    pub async fn poll(&self, params: ChatPollParams) -> Result<ChatPollResult, String> {
        let executor = self.executor();
        let limit = params.limit.unwrap_or(DEFAULT_POLL_LIMIT);

        // ── Phase 1: resolve the anchor timestamp if the caller
        //   pinned `after_message_id`. The data module returns the
        //   message record; we extract its `timestamp` field for the
        //   downstream `$gt` filter.
        let after_timestamp = if let Some(anchor_id) = params.after_message_id {
            let anchor_query = json!({
                "dbPath": "main",
                "collection": CHAT_MESSAGES_COLLECTION,
                "filter": { "id": { "$eq": anchor_id.to_string() } },
                "limit": 1,
            });

            let anchor_result = executor
                .execute_json("data/query", anchor_query)
                .await
                .map_err(|e| format!("chat/poll: anchor lookup failed: {e}"))?;

            let timestamp = extract_first_record_field(&anchor_result, "timestamp");
            match timestamp {
                Some(ts) => Some(ts),
                None => {
                    // Anchor not found — surface a typed error rather
                    // than silently returning all messages. Matches
                    // the TS impl's "Message not found" path.
                    return Err(format!(
                        "chat/poll: anchor message not found: {}",
                        anchor_id
                    ));
                }
            }
        } else {
            None
        };

        // ── Phase 2: build the main query. Filter on room +/- anchor
        //   timestamp; sort direction follows whether we have an anchor.
        let mut filter = serde_json::Map::new();
        if let Some(room_id) = params.room_id {
            filter.insert(
                "roomId".to_string(),
                json!({ "$eq": room_id.to_string() }),
            );
        }
        if let Some(ts) = after_timestamp.clone() {
            filter.insert("timestamp".to_string(), json!({ "$gt": ts }));
        }

        let sort_direction = if params.after_message_id.is_some() {
            "asc"
        } else {
            "desc"
        };

        let query = json!({
            "dbPath": "main",
            "collection": CHAT_MESSAGES_COLLECTION,
            "filter": filter,
            "sort": [{ "field": "timestamp", "direction": sort_direction }],
            "limit": limit,
        });

        let query_result = executor
            .execute_json("data/query", query)
            .await
            .map_err(|e| format!("chat/poll: query failed: {e}"))?;

        // ── Phase 3: extract message payloads from `DataRecord`
        //   envelopes the data module returns, then normalize to
        //   chronological order regardless of query direction.
        let messages = extract_records_as_data(&query_result);
        let mut sorted = messages;
        sorted.sort_by(|a, b| {
            let a_ts = a
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let b_ts = b
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            a_ts.cmp(b_ts)
        });

        Ok(ChatPollResult {
            count: sorted.len(),
            messages: sorted,
            after_message_id: params.after_message_id,
        })
    }
}

impl Default for ChatModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for ChatModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "chat",
            priority: ModulePriority::Normal,
            // Both prefixes route to this module — `chat/` is the
            // future-canonical surface, `collaboration/chat/` is the
            // legacy path that TS commands still use today and will
            // keep working through this module while consumers migrate.
            command_prefixes: &["chat/", "collaboration/chat/"],
            // Chat doesn't subscribe to events directly. Substrate
            // events (chat publish/receive) live on the airc module's
            // subscriptions; the chat module reaches the substrate by
            // calling airc commands, not by listening on its own.
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(
        &self,
        _ctx: &crate::runtime::ModuleContext,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            // The ONE command this PR migrates. Parse the envelope,
            // run the typed handler, materialize the typed response.
            // Every line here is the pattern we want to see at the top
            // of EVERY future command in this module.
            "chat/poll" | "collaboration/chat/poll" => {
                let req = CommandRequest::<ChatPollParams>::from_value(params)?;
                let result = self.poll(req.params).await?;
                CommandResponse::ok(result).into_command_result()
            }

            // Staged migration: the remaining three commands are still
            // owned by their TS implementations until their own
            // follow-up PRs land. The kernel router currently sees
            // `chat/` claim these names (per `command_prefixes` above)
            // but the handler returns a typed error so consumers know
            // to keep using the TS path until migration completes. The
            // back-compat `collaboration/chat/*` strings reach the same
            // TS impl through the existing CommandRouterServer bridge.
            //
            // When each migration PR lands, swap the stub arm for a
            // real handler using the same envelope pattern as
            // `chat/poll` above.
            "chat/send" | "collaboration/chat/send" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),
            "chat/analyze" | "collaboration/chat/analyze" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),
            "chat/export" | "collaboration/chat/export" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),

            other => Err(format!(
                "{other}: not handled by chat module — known commands are chat/poll, chat/send (stub), chat/analyze (stub), chat/export (stub)"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── helpers ──────────────────────────────────────────────────────────

/// Extract a single field from the first record in a data-module
/// `data/query` response. The data module returns
/// `{ success, data: [{ id, data: {...} }] }`, where each entry's
/// `data` is the entity payload. Returns the field as a JSON string
/// (which is the shape the TS impl threads downstream) or `None` if
/// the response shape doesn't have it.
fn extract_first_record_field(query_result: &Value, field: &str) -> Option<String> {
    let records = query_result.get("data")?.as_array()?;
    let first = records.first()?;
    let data = first.get("data")?;
    let value = data.get(field)?;
    match value {
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Extract message payloads from a data-module `data/query` response.
/// The response shape is `{ success, data: [{ id, data: <entity> }] }`;
/// we lift each `entity` out of its `DataRecord` envelope.
fn extract_records_as_data(query_result: &Value) -> Vec<Value> {
    query_result
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|record| record.get("data").cloned())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ModuleRegistry;
    use uuid::Uuid;

    /// Construct a `ChatModule` driving a freshly-built executor over a
    /// registry containing the given stub modules. The chat module's
    /// `with_executor` constructor takes the executor by `Arc`, so the
    /// resulting module routes all `executor()` calls through the
    /// in-test registry — no global `OnceLock` involvement.
    fn chat_with_stubs(stubs: Vec<Arc<dyn ServiceModule>>) -> ChatModule {
        let registry = Arc::new(ModuleRegistry::new());
        for module in stubs {
            registry.register(module);
        }
        let executor = Arc::new(CommandExecutor::new(registry));
        ChatModule::with_executor(executor)
    }

    /// Stub data module: handles `data/query` by returning a canned
    /// response. Each test instance gets its own canned result via the
    /// closure; lets us simulate "anchor found" / "anchor missing" /
    /// "messages returned" outcomes without standing up a real adapter.
    struct StubDataModule {
        // The response factory takes the inbound params so tests can
        // also assert on what got asked.
        responder: Box<dyn Fn(Value) -> Value + Send + Sync>,
    }

    impl StubDataModule {
        fn new<F>(responder: F) -> Self
        where
            F: Fn(Value) -> Value + Send + Sync + 'static,
        {
            Self {
                responder: Box::new(responder),
            }
        }
    }

    #[async_trait]
    impl ServiceModule for StubDataModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "data",
                priority: ModulePriority::Normal,
                command_prefixes: &["data/"],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }

        async fn initialize(
            &self,
            _ctx: &crate::runtime::ModuleContext,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn handle_command(
            &self,
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            assert_eq!(
                command, "data/query",
                "stub only handles data/query; got {command}"
            );
            Ok(CommandResult::Json((self.responder)(params)))
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // ── config + dispatch ────────────────────────────────────────────

    #[test]
    fn config_advertises_both_command_prefixes() {
        let chat = ChatModule::new();
        let config = chat.config();
        assert_eq!(config.name, "chat");
        // Both surfaces route to this module so consumers can migrate
        // off the legacy `collaboration/` prefix at their own pace.
        assert!(
            config.command_prefixes.contains(&"chat/")
                && config.command_prefixes.contains(&"collaboration/chat/"),
            "chat module must own BOTH prefixes during the migration window"
        );
    }

    #[tokio::test]
    async fn unknown_command_returns_loud_error_naming_supported_commands() {
        let chat = chat_with_stubs(vec![]);
        let err = chat
            .handle_command("chat/whatever", json!({}))
            .await
            .expect_err("unknown chat command must Err, not silently succeed");
        assert!(
            err.contains("not handled by chat module"),
            "error must name the module so the caller knows which layer failed: {err}"
        );
        assert!(
            err.contains("chat/poll"),
            "error must name the known commands so the caller can self-correct: {err}"
        );
    }

    // ── chat/send stubs name the follow-up PR ─────────────────────────

    #[tokio::test]
    async fn unmigrated_commands_fail_loud_and_name_followup() {
        let chat = chat_with_stubs(vec![]);
        for cmd in [
            "chat/send",
            "collaboration/chat/send",
            "chat/analyze",
            "collaboration/chat/analyze",
            "chat/export",
            "collaboration/chat/export",
        ] {
            let err = chat
                .handle_command(cmd, json!({}))
                .await
                .expect_err(&format!("{cmd}: unmigrated stub must Err"));
            assert!(
                err.contains("not yet migrated"),
                "stub error must announce the migration state: {err}"
            );
            assert!(
                err.contains("issue #57"),
                "stub error must point to the issue so the consumer can follow the migration: {err}"
            );
        }
    }

    // ── chat/poll: empty-result path ──────────────────────────────────

    #[tokio::test]
    async fn poll_returns_empty_result_when_data_module_returns_no_messages() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(|_p| {
            json!({ "success": true, "data": [] })
        }))]);

        let result = chat
            .poll(ChatPollParams::default())
            .await
            .expect("poll over empty data must succeed");
        assert_eq!(result.count, 0);
        assert!(result.messages.is_empty());
        assert!(result.after_message_id.is_none());
    }

    // ── chat/poll: latest-N path (no anchor) ──────────────────────────

    #[tokio::test]
    async fn poll_without_anchor_queries_data_desc_and_returns_chronological() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(|params| {
            // Validate the chat module built the expected query shape.
            assert_eq!(params["collection"], "chat_messages");
            assert_eq!(params["sort"][0]["direction"], "desc");
            // Caller didn't specify a limit → chat uses DEFAULT_POLL_LIMIT.
            assert_eq!(params["limit"], 50);
            // No filter fields set → empty filter map.
            assert_eq!(params["filter"], json!({}));

            json!({
                "success": true,
                "data": [
                    { "id": "id-2", "data": { "id": "id-2", "timestamp": "2026-05-30T15:00:00Z", "content": { "text": "second" } } },
                    { "id": "id-1", "data": { "id": "id-1", "timestamp": "2026-05-30T14:00:00Z", "content": { "text": "first" } } }
                ]
            })
        }))]);

        let result = chat
            .poll(ChatPollParams::default())
            .await
            .expect("latest-N poll must succeed");
        assert_eq!(result.count, 2);
        // Chronological normalization: even though data returned DESC,
        // chat sorts the result ASC for display.
        assert_eq!(
            result.messages[0]["timestamp"], "2026-05-30T14:00:00Z",
            "earliest message comes first after normalization"
        );
        assert_eq!(result.messages[1]["timestamp"], "2026-05-30T15:00:00Z");
    }

    // ── chat/poll: room filter applied ────────────────────────────────

    #[tokio::test]
    async fn poll_with_room_id_passes_filter_to_data_module() {
        let room_id = Uuid::new_v4();
        let room_str = room_id.to_string();
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(move |params| {
            assert_eq!(params["filter"]["roomId"]["$eq"], room_str);
            json!({ "success": true, "data": [] })
        }))]);

        chat.poll(ChatPollParams {
            room_id: Some(room_id),
            ..Default::default()
        })
        .await
        .expect("room-filtered poll must succeed");
    }

    // ── chat/poll: after_message_id path ──────────────────────────────

    #[tokio::test]
    async fn poll_with_anchor_looks_up_timestamp_then_filters_gt() {
        let anchor_id = Uuid::new_v4();
        let anchor_str = anchor_id.to_string();
        // Stub fires for BOTH queries (anchor lookup + main query); the
        // closure dispatches by inspecting the inbound filter shape.
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(move |params| {
            let filter = &params["filter"];

            // Anchor lookup: filter on `id`, limit 1.
            if let Some(id_filter) = filter.get("id") {
                assert_eq!(id_filter["$eq"], anchor_str);
                assert_eq!(params["limit"], 1);
                return json!({
                    "success": true,
                    "data": [{
                        "id": anchor_str,
                        "data": { "id": anchor_str, "timestamp": "2026-05-30T12:00:00Z" }
                    }]
                });
            }

            // Main query: must carry a `$gt` timestamp filter derived
            // from the anchor's timestamp, and must sort ASC.
            assert_eq!(filter["timestamp"]["$gt"], "2026-05-30T12:00:00Z");
            assert_eq!(params["sort"][0]["direction"], "asc");
            json!({
                "success": true,
                "data": [
                    { "id": "after-1", "data": { "id": "after-1", "timestamp": "2026-05-30T12:30:00Z" } }
                ]
            })
        }))]);

        let result = chat
            .poll(ChatPollParams {
                after_message_id: Some(anchor_id),
                ..Default::default()
            })
            .await
            .expect("anchor poll must succeed when the anchor exists");
        assert_eq!(result.count, 1);
        assert_eq!(result.after_message_id, Some(anchor_id));
    }

    // ── chat/poll: missing anchor fails loud ──────────────────────────

    #[tokio::test]
    async fn poll_with_anchor_returns_err_when_anchor_missing() {
        let anchor_id = Uuid::new_v4();
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(|_p| {
            // Empty data → anchor lookup yields no rows.
            json!({ "success": true, "data": [] })
        }))]);

        let err = chat
            .poll(ChatPollParams {
                after_message_id: Some(anchor_id),
                ..Default::default()
            })
            .await
            .expect_err("missing anchor must surface as an Err");
        assert!(
            err.contains("anchor message not found"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains(&anchor_id.to_string()),
            "error must name the offending id: {err}"
        );
    }

    // ── chat/poll: handler-level envelope wiring ──────────────────────

    #[tokio::test]
    async fn handle_command_routes_chat_poll_through_typed_envelope() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(|_p| {
            json!({ "success": true, "data": [] })
        }))]);

        let raw = json!({
            "limit": 7,
        });
        let result = chat
            .handle_command("chat/poll", raw)
            .await
            .expect("typed dispatch must succeed");

        let CommandResult::Json(value) = result else {
            panic!("chat/poll must return CommandResult::Json");
        };
        assert_eq!(value["success"], true);
        assert_eq!(value["count"], 0);
        assert!(value["messages"].is_array());
    }

    #[tokio::test]
    async fn handle_command_accepts_legacy_collaboration_prefix() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::new(|_p| {
            json!({ "success": true, "data": [] })
        }))]);

        // The legacy `collaboration/chat/poll` path must route to the
        // same handler — that's the back-compat contract that lets TS
        // consumers keep their existing wire calls working through the
        // migration window.
        let result = chat
            .handle_command("collaboration/chat/poll", json!({}))
            .await
            .expect("legacy prefix must work");
        let CommandResult::Json(value) = result else {
            panic!("must return Json variant");
        };
        assert_eq!(value["success"], true);
    }
}
