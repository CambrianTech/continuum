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
use uuid::Uuid;

use crate::runtime::{
    command_executor::{self, CommandExecutor},
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

pub mod types;

use types::{
    ChatPollParams, ChatPollResult, ChatSendParams, ChatSendResult, CHAT_MESSAGES_COLLECTION,
    DEFAULT_POLL_LIMIT,
};

/// Adapter handle the chat module reads/writes against. `"main"` is the
/// kernel-wide convention for the primary continuum database — the
/// data module resolves it to either `$DATABASE_URL` (when set) or
/// `$HOME/.continuum/database/main.db` (the local SQLite default).
/// Centralized here so a future migration to per-room adapters is a
/// single-edit move.
const CHAT_DATA_HANDLE: &str = "main";

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

    /// `chat/send` — persist a chat message locally, then broadcast it.
    ///
    /// Two cross-module calls in sequence, NOT one merged write. The
    /// substrate has no built-in transaction across modules; this
    /// handler is the canonical demonstration of how to compose two
    /// effects with explicit partial-failure semantics.
    ///
    /// # Ordering: data first, airc second
    ///
    /// Local persistence is the ground truth. The reverse order would
    /// risk publishing a message to peers that this node doesn't know
    /// about — and a peer reading back that message would find no
    /// local record. With data-first, the worst case is *we have the
    /// message but peers don't* — a degradation, not a divergence.
    ///
    /// # Partial-failure semantics
    ///
    /// | data | airc | handler returns                                          |
    /// |------|------|----------------------------------------------------------|
    /// | ok   | ok   | `Ok(result with message_id + event_id)`                  |
    /// | ok   | fail | `Ok(result with message_id, event_id=None, warning=...)` |
    /// | fail | —    | `Err(...)` — no airc publish attempted                   |
    ///
    /// **An airc-only failure is NOT command-level failure.** The
    /// message IS stored locally; consumers see it via `chat/poll`.
    /// A future retry/sync mechanism heals the broadcast. Surfacing
    /// this as `Err` would tell the caller "your write didn't happen",
    /// which is wrong — half of the write did. The `warning` field is
    /// the right shape: degraded success.
    ///
    /// # Idempotency (known gap, deferred)
    ///
    /// A retried `chat/send` (network glitch on the caller side)
    /// currently produces two stored messages. This matches today's
    /// TS behavior and is out of scope for the first migration.
    /// Future PR can add a `client_dedup_id` param + a TTL'd map in
    /// the chat module; the substrate is ready for it (`HandleRef`
    /// could be the dedup id) but the design conversation is its
    /// own scope.
    pub async fn send(&self, params: ChatSendParams) -> Result<ChatSendResult, String> {
        let executor = self.executor();
        let message_id = Uuid::new_v4();
        let now_ms = now_ms();
        let now_iso = now_iso(now_ms);

        // ── Step 1: persist locally (ground truth) ───────────────────
        //
        // Build the entity payload matching `ChatMessageEntity`'s
        // expected shape on the TS side — text-only content for this
        // first migration, `metadata.source: "user"`, status sent.
        // Media + replyToId threading + system messages are deferred.
        let entity_data = json!({
            "id": message_id.to_string(),
            "roomId": params.room_id.to_string(),
            "senderId": params.sender_id.to_string(),
            "timestamp": now_iso,
            "content": { "text": params.text },
            "replyToId": params.reply_to_id.map(|u| u.to_string()),
            "metadata": { "source": "user" },
            "status": "sent",
        });

        let create_params = json!({
            "dbPath": CHAT_DATA_HANDLE,
            "collection": CHAT_MESSAGES_COLLECTION,
            "id": message_id.to_string(),
            "data": entity_data,
        });

        // Hard failure: data layer didn't store the message. No airc
        // publish is attempted — the message doesn't exist locally,
        // so broadcasting it would create the bad-divergence case.
        // Surface as command-level Err.
        let create_result = executor
            .execute_json("data/create", create_params)
            .await
            .map_err(|e| format!("chat/send: data/create failed: {e}"))?;

        // The data module's `data/create` returns
        // `{success: true|false, error?: "..."}`. A success=false
        // path is the "stored the request but the write didn't land"
        // case (validation, unique constraint, etc.) — still hard
        // failure from chat's perspective.
        if !create_result
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let inner = create_result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("data module returned success=false without an error message");
            return Err(format!(
                "chat/send: data/create returned success=false: {inner}"
            ));
        }

        // ── Step 2: broadcast (best-effort) ─────────────────────────
        //
        // Build an AIRC realtime envelope carrying the chat
        // transcript schema. Construction stays at the wire-shape
        // level (json!) rather than importing the airc-realtime
        // typed structs — chat depends on airc through the command
        // surface, not through internal types. If airc changes its
        // wire shape, its `airc/realtime-publish` handler will
        // surface a parse error and the test
        // `send_envelope_matches_airc_publish_wire_shape` will
        // catch the drift.
        let publish_envelope = json!({
            "eventId": Uuid::new_v4().to_string(),
            "roomId": params.room_id.to_string(),
            "sourceId": params.sender_id.to_string(),
            "createdAtMs": now_ms,
            // Delivery must match the payload's semantics — see
            // `AircRealtimePayload::delivery()`. ExistingSchema/
            // ChatTranscript → Durable.
            "delivery": "durable",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "chat_transcript",
                    "inline": {
                        "messageId": message_id.to_string(),
                        "text": params.text,
                        "senderId": params.sender_id.to_string(),
                        "replyToId": params.reply_to_id.map(|u| u.to_string()),
                    }
                }
            },
        });

        let publish_params = json!({ "envelope": publish_envelope });

        // Partial failure path: data succeeded, airc failed. Return
        // success with a warning naming what happened. The caller can
        // surface a UI warning, retry, or just log.
        match executor
            .execute_json("airc/realtime-publish", publish_params)
            .await
        {
            Ok(publish_result) => {
                let event_id = publish_result
                    .get("eventId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Ok(ChatSendResult {
                    message_id,
                    event_id,
                    warning: None,
                })
            }
            Err(airc_err) => Ok(ChatSendResult {
                message_id,
                event_id: None,
                warning: Some(format!(
                    "airc/realtime-publish failed: {airc_err}. Message stored locally (id={message_id}) but not broadcast to peers."
                )),
            }),
        }
    }
}

impl Default for ChatModule {
    fn default() -> Self {
        Self::new()
    }
}

// ── time helpers ─────────────────────────────────────────────────────
//
// Wall-clock reads centralized here so chat's handlers stay free of
// `SystemTime` calls scattered through their bodies. Both use the same
// epoch instant so a stored timestamp and an airc envelope's
// `createdAtMs` from the same `send()` call agree by construction
// (rather than risking a tiny skew between two separate reads).

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_iso(unix_ms: u64) -> String {
    // The TS ChatMessageEntity carries `timestamp` as an ISO-8601
    // string (matches how the TS impl writes it via
    // `new Date().toISOString()`). Format it from the same epoch we
    // pass to the airc envelope so the two surfaces agree on the
    // same moment.
    let secs = (unix_ms / 1000) as i64;
    let nsec_part = ((unix_ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nsec_part)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
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
            // ── Migrated commands ───────────────────────────────────
            //
            // Every arm follows the same three-line pattern:
            //   1. parse the envelope
            //   2. run the typed handler
            //   3. materialize the typed response

            "chat/poll" | "collaboration/chat/poll" => {
                let req = CommandRequest::<ChatPollParams>::from_value(params)?;
                let result = self.poll(req.params).await?;
                CommandResponse::ok(result).into_command_result()
            }

            "chat/send" | "collaboration/chat/send" => {
                let req = CommandRequest::<ChatSendParams>::from_value(params)?;
                let result = self.send(req.params).await?;
                CommandResponse::ok(result).into_command_result()
            }

            // ── Staged migration stubs ──────────────────────────────
            //
            // The remaining commands still own their TS
            // implementations until their own follow-up PRs land. The
            // kernel router currently sees `chat/` claim these names
            // (per `command_prefixes` above) but the handler returns
            // a typed error so consumers know to keep using the TS
            // path until migration completes. The back-compat
            // `collaboration/chat/*` strings reach the same TS impl
            // through the existing CommandRouterServer bridge.
            //
            // When each migration PR lands, swap the stub arm for a
            // real handler using the envelope pattern above.

            "chat/analyze" | "collaboration/chat/analyze" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),
            "chat/export" | "collaboration/chat/export" => Err(format!(
                "{}: not yet migrated — TS implementation still owns this command (follow-up PR to issue #57)",
                command
            )),

            other => Err(format!(
                "{other}: not handled by chat module — known commands are chat/poll, chat/send, chat/analyze (stub), chat/export (stub)"
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

    /// Stub data module: handles any `data/*` command by returning a
    /// canned response built by the test's closure. The closure
    /// receives BOTH the command name and the params so tests can
    /// branch on command (`data/query` vs `data/create` etc.) or
    /// inspect the inbound shape.
    ///
    /// `chat/poll` tests use the params-only `Self::query_only`
    /// constructor (back-compat); `chat/send` tests use the full
    /// `Self::new` constructor with command-aware dispatch.
    struct StubDataModule {
        responder: Box<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>,
    }

    impl StubDataModule {
        fn new<F>(responder: F) -> Self
        where
            F: Fn(&str, Value) -> Result<Value, String> + Send + Sync + 'static,
        {
            Self {
                responder: Box::new(responder),
            }
        }

        /// Construct a stub that only handles `data/query` and runs
        /// the given params-only closure on inbound params. Asserts
        /// the command name to catch unintended calls. Convenience
        /// for chat/poll tests that pre-date dual-command testing.
        fn query_only<F>(responder: F) -> Self
        where
            F: Fn(Value) -> Value + Send + Sync + 'static,
        {
            Self::new(move |command, params| {
                assert_eq!(
                    command, "data/query",
                    "query_only stub received unexpected command: {command}"
                );
                Ok(responder(params))
            })
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
            (self.responder)(command, params).map(CommandResult::Json)
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

    // ── Unmigrated stubs still name the follow-up PR ─────────────────
    //
    // chat/send migrated in this PR; analyze + export still on TS.

    #[tokio::test]
    async fn unmigrated_commands_fail_loud_and_name_followup() {
        let chat = chat_with_stubs(vec![]);
        for cmd in [
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_p| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|params| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |params| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |params| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_p| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_p| {
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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_p| {
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

    // ════════════════════════════════════════════════════════════════
    // chat/send: dual-write composition stress tests
    // ════════════════════════════════════════════════════════════════
    //
    // The chat module's first multi-cross-module-call handler:
    // chat → data (persist) then chat → airc (publish). Each test
    // pins one cell of the (data ok/fail × airc ok/fail) matrix,
    // plus the wire-contract invariants the dual-write design
    // promised.

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Stub airc module: handles `airc/realtime-publish` by returning
    /// either a canned success Value or a fail-loud Err. Lets each
    /// chat/send test pick the airc outcome independently of data's.
    struct StubAircModule {
        publish_responder: Box<dyn Fn(Value) -> Result<Value, String> + Send + Sync>,
    }

    impl StubAircModule {
        fn ok(canned: Value) -> Self {
            Self {
                publish_responder: Box::new(move |_p| Ok(canned.clone())),
            }
        }

        fn err(message: impl Into<String>) -> Self {
            let msg = message.into();
            Self {
                publish_responder: Box::new(move |_p| Err(msg.clone())),
            }
        }

        fn with<F>(responder: F) -> Self
        where
            F: Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
        {
            Self {
                publish_responder: Box::new(responder),
            }
        }
    }

    #[async_trait]
    impl ServiceModule for StubAircModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "airc",
                priority: ModulePriority::Normal,
                command_prefixes: &["airc/"],
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
                command, "airc/realtime-publish",
                "chat/send must only reach airc via realtime-publish, got {command}"
            );
            (self.publish_responder)(params).map(CommandResult::Json)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// Build a chat/send params instance with sensible defaults. Tests
    /// override only the fields they care about.
    fn sample_send_params() -> ChatSendParams {
        ChatSendParams {
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            text: "hello world".into(),
            reply_to_id: None,
        }
    }

    /// Standard "airc broadcast succeeded" canned response. Mirrors
    /// the actual `AircRealtimePublishResult` wire shape (camelCase,
    /// `eventId` field).
    fn airc_ok_response(event_id: &str) -> Value {
        json!({
            "ok": true,
            "eventId": event_id,
            "roomId": Uuid::new_v4().to_string(),
            "delivery": "durable",
            "storedForReplay": true,
            "replayDepth": 0,
            "activePresenceCount": 0,
            "activeSubscriptionCount": 0,
            "activePeerManifestCount": 0,
        })
    }

    // ── Happy path: both succeed ─────────────────────────────────────

    #[tokio::test]
    async fn send_happy_path_returns_message_id_and_event_id() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|cmd, _p| {
                assert_eq!(cmd, "data/create", "happy path only writes (no other data ops)");
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-happy-001"))),
        ]);

        let result = chat
            .send(sample_send_params())
            .await
            .expect("happy path must succeed");

        // Both surfaces' ids are present: message stored locally AND
        // airc event id returned for broadcast correlation.
        assert!(!result.message_id.is_nil(), "message_id must be a real UUID");
        assert_eq!(
            result.event_id.as_deref(),
            Some("evt-happy-001"),
            "happy path must surface the airc-side event id"
        );
        assert!(
            result.warning.is_none(),
            "no warning on happy path: {result:?}"
        );
    }

    // ── Partial failure: data ok + airc fail ─────────────────────────

    #[tokio::test]
    async fn send_with_airc_failure_returns_warning_and_null_event_id() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| Ok(json!({ "success": true })))),
            Arc::new(StubAircModule::err(
                "airc daemon socket unreachable: ENOENT",
            )),
        ]);

        let result = chat
            .send(sample_send_params())
            .await
            .expect("airc-only failure must be degraded success, NOT command-level Err");

        assert!(
            !result.message_id.is_nil(),
            "message_id present — local store succeeded"
        );
        assert!(
            result.event_id.is_none(),
            "event_id absent when broadcast didn't land"
        );
        let warning = result.warning.as_deref().expect("warning must be set");
        assert!(
            warning.contains("airc/realtime-publish failed"),
            "warning names the failing surface: {warning}"
        );
        assert!(
            warning.contains("ENOENT"),
            "warning surfaces the underlying error so the caller can diagnose: {warning}"
        );
        assert!(
            warning.contains("stored locally"),
            "warning reassures the caller the message wasn't lost: {warning}"
        );
        assert!(
            warning.contains(&result.message_id.to_string()),
            "warning names the message id so the caller can correlate logs: {warning}"
        );
    }

    // ── Hard failure: data fail ──────────────────────────────────────

    #[tokio::test]
    async fn send_with_data_executor_failure_propagates_as_err_and_skips_airc() {
        // Track whether airc was called — it must NOT be when data
        // failed (publishing without a local record creates the
        // bad-divergence case the ordering was designed to prevent).
        let airc_calls = Arc::new(AtomicUsize::new(0));
        let airc_calls_tracker = airc_calls.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Err("sqlite is locked".to_string())
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_calls_tracker.fetch_add(1, Ordering::SeqCst);
                Ok(airc_ok_response("should-never-be-called"))
            })),
        ]);

        let err = chat
            .send(sample_send_params())
            .await
            .expect_err("data executor failure must propagate as command-level Err");

        assert!(
            err.contains("chat/send: data/create failed"),
            "error must name the failing surface: {err}"
        );
        assert!(
            err.contains("sqlite is locked"),
            "error must surface the underlying cause: {err}"
        );
        assert_eq!(
            airc_calls.load(Ordering::SeqCst),
            0,
            "airc MUST NOT be called when data failed — the ordering invariant"
        );
    }

    #[tokio::test]
    async fn send_with_data_success_false_propagates_as_err_and_skips_airc() {
        // Subtle path: the data executor returns Ok (no transport
        // failure) but with success=false (validation error, unique
        // constraint, etc.). Still hard failure from chat's
        // perspective — the message isn't stored.
        let airc_calls = Arc::new(AtomicUsize::new(0));
        let airc_calls_tracker = airc_calls.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(json!({
                    "success": false,
                    "error": "unique constraint violated on (id)",
                }))
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_calls_tracker.fetch_add(1, Ordering::SeqCst);
                Ok(airc_ok_response("should-never-be-called"))
            })),
        ]);

        let err = chat
            .send(sample_send_params())
            .await
            .expect_err("success=false from data must propagate as Err");

        assert!(
            err.contains("success=false"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("unique constraint"),
            "error must surface the underlying cause: {err}"
        );
        assert_eq!(
            airc_calls.load(Ordering::SeqCst),
            0,
            "success=false also blocks the airc publish — same ordering invariant"
        );
    }

    // ── Ordering invariant: data called BEFORE airc ──────────────────

    #[tokio::test]
    async fn send_calls_data_before_airc() {
        // Pin the call order via shared timestamp markers. The
        // ordering invariant is the CORE of the dual-write design;
        // if it ever flips, the bad-divergence case becomes
        // reachable.
        let call_log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let data_log = call_log.clone();
        let airc_log = call_log.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, _p| {
                if cmd == "data/create" {
                    data_log.lock().unwrap().push("data/create");
                }
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_log.lock().unwrap().push("airc/realtime-publish");
                Ok(airc_ok_response("evt-order-001"))
            })),
        ]);

        chat.send(sample_send_params())
            .await
            .expect("happy path must succeed");

        let calls = call_log.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec!["data/create", "airc/realtime-publish"],
            "data MUST be called before airc — the dual-write ordering invariant"
        );
    }

    // ── Wire contract: what chat sends to data ───────────────────────

    #[tokio::test]
    async fn send_writes_chat_messages_collection_with_canonical_entity_shape() {
        // The data write must match the TS `ChatMessageEntity` shape
        // so existing TS readers (and chat/poll's response parser)
        // see a consistent entity. Pin every field the TS readers
        // depend on.
        let room_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();
        let reply_to_id = Uuid::new_v4();

        let observed_create: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let observer = observed_create.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, params| {
                if cmd == "data/create" {
                    *observer.lock().unwrap() = Some(params);
                }
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-wire-001"))),
        ]);

        let result = chat
            .send(ChatSendParams {
                room_id,
                sender_id,
                text: "wire contract message".into(),
                reply_to_id: Some(reply_to_id),
            })
            .await
            .expect("send must succeed");

        let create = observed_create
            .lock()
            .unwrap()
            .clone()
            .expect("data/create must have been called");

        assert_eq!(create["dbPath"], "main", "writes go to the main adapter handle");
        assert_eq!(create["collection"], "chat_messages");
        assert_eq!(
            create["id"], result.message_id.to_string(),
            "create.id matches the returned message_id"
        );

        let entity = &create["data"];
        assert_eq!(entity["id"], result.message_id.to_string());
        assert_eq!(entity["roomId"], room_id.to_string());
        assert_eq!(entity["senderId"], sender_id.to_string());
        assert_eq!(entity["content"]["text"], "wire contract message");
        assert_eq!(entity["replyToId"], reply_to_id.to_string());
        assert_eq!(
            entity["metadata"]["source"], "user",
            "default source is 'user' (system messages will need their own param)"
        );
        assert_eq!(entity["status"], "sent");
        assert!(
            entity["timestamp"].is_string(),
            "timestamp is an ISO-8601 string (matches TS ChatMessageEntity)"
        );
        assert!(
            entity["timestamp"]
                .as_str()
                .unwrap()
                .ends_with('Z'),
            "timestamp is UTC"
        );
    }

    // ── Wire contract: what chat sends to airc ───────────────────────

    #[tokio::test]
    async fn send_envelope_matches_airc_publish_wire_shape() {
        // Pin the envelope shape chat hands to airc/realtime-publish.
        // If airc's wire contract ever changes, this test catches
        // the drift even though chat doesn't import airc's typed
        // structs.
        let room_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();

        let observed_publish: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let observer = observed_publish.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| Ok(json!({ "success": true })))),
            Arc::new(StubAircModule::with(move |params| {
                *observer.lock().unwrap() = Some(params);
                Ok(airc_ok_response("evt-envelope-001"))
            })),
        ]);

        let result = chat
            .send(ChatSendParams {
                room_id,
                sender_id,
                text: "envelope shape test".into(),
                reply_to_id: None,
            })
            .await
            .expect("send must succeed");

        let publish = observed_publish
            .lock()
            .unwrap()
            .clone()
            .expect("airc/realtime-publish must have been called");

        let envelope = &publish["envelope"];
        // Top-level envelope fields per AircRealtimeEnvelope.
        assert!(
            envelope["eventId"].as_str().is_some(),
            "envelope must carry an eventId (chat mints its own UUID)"
        );
        assert_eq!(envelope["roomId"], room_id.to_string());
        assert_eq!(envelope["sourceId"], sender_id.to_string());
        assert!(envelope["createdAtMs"].is_number());
        assert_eq!(
            envelope["delivery"], "durable",
            "chat transcript → durable delivery (matches the airc payload's delivery() semantics)"
        );

        // Payload tagged-enum shape: AircRealtimePayload::ExistingSchema.
        let payload = &envelope["payload"];
        assert_eq!(
            payload["kind"], "existing_schema",
            "serde-tagged payload variant for the schema-ref shape"
        );
        let inner = &payload["payload"];
        assert_eq!(
            inner["schema"], "chat_transcript",
            "chat messages carry the ChatTranscript schema tag"
        );

        let inline = &inner["inline"];
        assert_eq!(inline["messageId"], result.message_id.to_string());
        assert_eq!(inline["text"], "envelope shape test");
        assert_eq!(inline["senderId"], sender_id.to_string());
        assert!(
            inline["replyToId"].is_null(),
            "no thread anchor for this message"
        );
    }

    // ── End-to-end through handle_command ────────────────────────────

    #[tokio::test]
    async fn handle_command_routes_chat_send_through_typed_envelope() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| Ok(json!({ "success": true })))),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-dispatch-001"))),
        ]);

        let raw = json!({
            "roomId": Uuid::new_v4().to_string(),
            "senderId": Uuid::new_v4().to_string(),
            "text": "via handle_command",
        });
        let result = chat
            .handle_command("chat/send", raw)
            .await
            .expect("typed dispatch must succeed");

        let CommandResult::Json(value) = result else {
            panic!("chat/send must return CommandResult::Json");
        };
        assert_eq!(value["success"], true);
        assert!(
            value["messageId"].as_str().is_some(),
            "messageId at top level (flattened from ChatSendResult)"
        );
        assert_eq!(value["eventId"], "evt-dispatch-001");
        assert!(
            value.get("warning").is_none(),
            "no warning on happy path: {value}"
        );
    }

    #[tokio::test]
    async fn handle_command_chat_send_accepts_legacy_collaboration_prefix() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| Ok(json!({ "success": true })))),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-legacy-001"))),
        ]);

        let raw = json!({
            "roomId": Uuid::new_v4().to_string(),
            "senderId": Uuid::new_v4().to_string(),
            "text": "via legacy prefix",
        });
        let result = chat
            .handle_command("collaboration/chat/send", raw)
            .await
            .expect("legacy prefix must work for chat/send too");
        let CommandResult::Json(value) = result else {
            panic!("must return Json variant");
        };
        assert_eq!(value["success"], true);
    }

    // ════════════════════════════════════════════════════════════════
    // Multi-persona concurrency stress tests
    // ════════════════════════════════════════════════════════════════
    //
    // Per Joel 2026-05-30: "Each persona exists in its own threads."
    // The kernel registers ONE ChatModule instance; every persona's
    // thread invokes its `&self` methods concurrently. The tests
    // below PIN the invariants the substrate is designed to uphold
    // under that load — they are not exercising rare paths, they are
    // the production scenario.
    //
    // # Runtime flavor
    //
    // Every concurrency test runs on `flavor = "multi_thread",
    // worker_threads = 4` so the tasks actually preempt each other on
    // distinct OS threads rather than cooperatively interleaving on
    // one. Single-threaded tokio would silently serialize the test
    // and pass even if the substrate had a data race.

    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    /// `chat/send` under N concurrent persona threads, all sharing the
    /// same `ChatModule` instance through the same executor:
    /// - every send must complete (no panics, no lost work)
    /// - every send must return a DISTINCT `message_id` (no UUID
    ///   collision; no shared mutable state holding the id)
    /// - every send's `message_id` must appear in the data layer
    ///   exactly once (no duplicate writes, no phantom writes)
    /// - the SET of stored ids must equal the SET of returned ids
    ///   (no lost writes)
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_under_concurrent_load_stores_all_messages_with_distinct_ids() {
        const PARALLEL: usize = 50;

        let writes: Arc<StdMutex<Vec<Uuid>>> = Arc::new(StdMutex::new(Vec::new()));
        let writes_tracker = writes.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, params| {
                if cmd == "data/create" {
                    let id_str = params["id"]
                        .as_str()
                        .expect("data/create must carry an id");
                    let id = Uuid::parse_str(id_str).expect("id must be a UUID");
                    writes_tracker.lock().unwrap().push(id);
                }
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-conc-001"))),
        ]);
        let chat = Arc::new(chat);

        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let chat = chat.clone();
            tasks.push(tokio::spawn(async move {
                chat.send(ChatSendParams {
                    room_id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    text: format!("concurrent message {i}"),
                    reply_to_id: None,
                })
                .await
                .expect("send must succeed")
            }));
        }

        let results: Vec<ChatSendResult> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        // Every send completed.
        assert_eq!(
            results.len(),
            PARALLEL,
            "every concurrent send task must complete"
        );

        // Every send wrote.
        assert_eq!(
            writes.lock().unwrap().len(),
            PARALLEL,
            "every concurrent send must have called data/create exactly once"
        );

        // Returned ids are all distinct.
        let mut returned_ids: Vec<Uuid> = results.iter().map(|r| r.message_id).collect();
        returned_ids.sort();
        let count_before_dedup = returned_ids.len();
        returned_ids.dedup();
        assert_eq!(
            returned_ids.len(),
            count_before_dedup,
            "concurrent sends must produce distinct message_ids (UUID collision OR shared mutable state)"
        );

        // Stored ids == Returned ids. No lost writes, no phantom writes.
        let mut stored = writes.lock().unwrap().clone();
        stored.sort();
        assert_eq!(
            stored, returned_ids,
            "stored ids must equal returned ids — no message gets persisted that the caller doesn't know about, no returned id is missing from the store"
        );
    }

    /// Per-call ordering invariant under concurrency: even when N
    /// concurrent calls interleave globally, EACH call's own
    /// `data/create` must precede its own `airc/realtime-publish`. The
    /// dual-write design's bad-divergence safety net depends on this.
    ///
    /// Strategy: tag every observation with the `message_id` (== the
    /// stored entity id == the airc inline message id). Group by id;
    /// assert per-call ordering.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_preserves_per_call_ordering_under_concurrent_load() {
        const PARALLEL: usize = 25;

        let log: Arc<StdMutex<Vec<(Uuid, &'static str)>>> =
            Arc::new(StdMutex::new(Vec::new()));
        let data_log = log.clone();
        let airc_log = log.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, params| {
                if cmd == "data/create" {
                    let id_str = params["id"].as_str().unwrap();
                    let id = Uuid::parse_str(id_str).unwrap();
                    data_log.lock().unwrap().push((id, "data/create"));
                }
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::with(move |params| {
                let inline_id = params["envelope"]["payload"]["payload"]["inline"]["messageId"]
                    .as_str()
                    .expect("envelope must carry the message id");
                let id = Uuid::parse_str(inline_id).unwrap();
                airc_log
                    .lock()
                    .unwrap()
                    .push((id, "airc/realtime-publish"));
                Ok(airc_ok_response("evt-order-conc"))
            })),
        ]);
        let chat = Arc::new(chat);

        let mut tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            let chat = chat.clone();
            tasks.push(tokio::spawn(
                async move { chat.send(sample_send_params()).await },
            ));
        }
        futures::future::join_all(tasks).await;

        // Walk the global log, group event indices by message_id.
        let observed = log.lock().unwrap().clone();
        let mut per_call: HashMap<Uuid, Vec<(usize, &'static str)>> = HashMap::new();
        for (idx, (id, event)) in observed.iter().enumerate() {
            per_call.entry(*id).or_default().push((idx, *event));
        }

        assert_eq!(
            per_call.len(),
            PARALLEL,
            "every concurrent call must contribute its own correlation id (no aliasing)"
        );

        for (id, events) in per_call {
            assert_eq!(
                events.len(),
                2,
                "each call must produce exactly 2 events (data + airc) for id={id}"
            );
            // Sort by the GLOBAL log index so we know the call-internal
            // order rather than insertion order into the per-call vec.
            let mut sorted = events.clone();
            sorted.sort_by_key(|(idx, _)| *idx);
            assert_eq!(
                sorted[0].1, "data/create",
                "per-call ordering: data MUST come before airc for id={id}, observed={sorted:?}"
            );
            assert_eq!(
                sorted[1].1, "airc/realtime-publish",
                "per-call ordering: airc MUST come after data for id={id}, observed={sorted:?}"
            );
        }
    }

    /// Mixed outcomes under concurrent load: half the calls have airc
    /// fail, half succeed. Each call's result must reflect ITS OWN
    /// outcome — no cross-contamination between concurrent calls.
    ///
    /// The airc stub branches on a flag embedded in the message text
    /// so it can decide per-call. Critical invariant: the warning
    /// string for a failed call must reference THIS call's
    /// `message_id`, not a sibling concurrent call's id.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn send_isolates_mixed_outcomes_under_concurrent_load() {
        const PARALLEL: usize = 30;

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(json!({ "success": true }))
            })),
            Arc::new(StubAircModule::with(|params| {
                // Drive the airc outcome from the inline message text.
                let text = params["envelope"]["payload"]["payload"]["inline"]["text"]
                    .as_str()
                    .unwrap();
                if text.contains("FAIL") {
                    Err(format!("simulated airc failure for: {text}"))
                } else {
                    Ok(airc_ok_response("evt-mixed-ok"))
                }
            })),
        ]);
        let chat = Arc::new(chat);

        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let chat = chat.clone();
            let text = if i % 2 == 0 {
                format!("OK call {i}")
            } else {
                format!("FAIL call {i}")
            };
            let label = text.clone();
            tasks.push(tokio::spawn(async move {
                let result = chat
                    .send(ChatSendParams {
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        text,
                        reply_to_id: None,
                    })
                    .await
                    .expect("send must succeed (degraded success counts)");
                (label, result)
            }));
        }
        let results: Vec<(String, ChatSendResult)> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        let (mut ok_count, mut fail_count) = (0usize, 0usize);
        for (label, result) in &results {
            if label.contains("FAIL") {
                fail_count += 1;
                assert!(
                    result.event_id.is_none(),
                    "{label}: airc failed → event_id must be None"
                );
                let warning = result
                    .warning
                    .as_ref()
                    .expect(&format!("{label}: airc failed → warning must be set"));
                // Cross-contamination check: the warning's message_id
                // must match THIS call's result.message_id (not a
                // sibling call's id that ran concurrently).
                assert!(
                    warning.contains(&result.message_id.to_string()),
                    "{label}: warning must name THIS call's message_id ({}), not a sibling's. warning={}",
                    result.message_id, warning
                );
                // The underlying airc error must surface unchanged.
                assert!(
                    warning.contains(label.as_str()),
                    "{label}: warning must surface the airc-side error text, got: {warning}"
                );
            } else {
                ok_count += 1;
                assert!(
                    result.event_id.is_some(),
                    "{label}: airc ok → event_id must be Some"
                );
                assert!(
                    result.warning.is_none(),
                    "{label}: airc ok → warning must be None"
                );
            }
        }
        assert_eq!(ok_count, PARALLEL / 2, "half the calls should succeed");
        assert_eq!(
            fail_count,
            PARALLEL / 2,
            "half the calls should report degraded success"
        );
    }

    /// `chat/poll` under N concurrent persona threads, each polling a
    /// DIFFERENT room: every task must get back its OWN room's
    /// messages, never a sibling task's. The stub echoes the
    /// requested `roomId` so we can prove the result didn't get
    /// swapped between concurrent calls.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn poll_isolates_results_under_concurrent_load() {
        const PARALLEL: usize = 30;

        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|params| {
            // Echo the requested roomId back in the synthetic result so
            // the caller can prove its own input flowed through.
            let echoed = params["filter"]["roomId"]["$eq"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            json!({
                "success": true,
                "data": [
                    {
                        "id": "echo",
                        "data": {
                            "id": "echo",
                            "roomId": echoed,
                            "timestamp": "2026-05-30T00:00:00Z",
                            "content": { "text": "echoed" },
                        }
                    }
                ],
            })
        }))]);
        let chat = Arc::new(chat);

        let mut tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            let chat = chat.clone();
            let my_room = Uuid::new_v4();
            tasks.push(tokio::spawn(async move {
                let result = chat
                    .poll(ChatPollParams {
                        room_id: Some(my_room),
                        ..Default::default()
                    })
                    .await
                    .expect("poll must succeed");
                (my_room, result)
            }));
        }
        let results = futures::future::join_all(tasks).await;

        for r in results {
            let (my_room, poll_result) = r.expect("task must not panic");
            assert_eq!(poll_result.count, 1, "each task gets one echoed message");
            let echoed = poll_result.messages[0]["roomId"].as_str().unwrap();
            assert_eq!(
                echoed,
                my_room.to_string(),
                "each task MUST get back its OWN room's result; no cross-talk between concurrent polls"
            );
        }
    }
}
