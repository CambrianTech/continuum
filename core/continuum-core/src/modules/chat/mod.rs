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

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::runtime::{
    command_executor::CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority,
    ServiceModule,
};
use crate::sdk_codegen::DynCommand;

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
    /// Substrate-wide command executor. Installed by `start_server`
    /// via `install_executor` (the same path every other module uses).
    /// `with_executor` lets tests inject a custom chain without
    /// installing into the global registry.
    executor_slot: Arc<LateBound<CommandExecutor>>,
}

impl ChatModule {
    /// Construct a chat module. The executor is installed later by
    /// `start_server` via `ServiceModule::install_executor` (task #224).
    pub fn new() -> Self {
        Self {
            executor_slot: Arc::new(LateBound::new("chat::executor")),
        }
    }

    /// Test-only constructor — inject an explicit executor instance so
    /// the test owns its dispatch chain (commonly a registry with a
    /// stub DataModule). Lets the chat module's tests exercise the
    /// real cross-module call path without going through `start_server`.
    #[cfg(test)]
    pub fn with_executor(executor: Arc<CommandExecutor>) -> Self {
        let executor_slot = Arc::new(LateBound::new("chat::executor"));
        executor_slot.install(executor);
        Self { executor_slot }
    }

    /// Rebuild a chat module over an EXISTING late-bound executor slot.
    /// A `ChatModule` is stateless apart from that slot, so the typed
    /// `chat/*` [`ActionCommand`](crate::sdk_codegen::ActionCommand)s
    /// capture the slot at `commands()` time and reconstruct the module
    /// per call to reach the canonical [`poll`](Self::poll) /
    /// [`send`](Self::send) implementation — there is ONE body, shared by
    /// the command surface and the module's own tests. The slot is an
    /// `Arc`, so this is a pointer clone, and it stays the SAME slot
    /// `start_server` installs the executor into.
    pub(crate) fn from_slot(executor_slot: Arc<LateBound<CommandExecutor>>) -> Self {
        Self { executor_slot }
    }

    /// Resolve the executor for the current call. Panics if the
    /// executor was never installed — that's a boot ordering bug
    /// (`start_server` must call `install_executor_on_all` BEFORE
    /// any chat command can dispatch). Per [[no-fallbacks-ever]]:
    /// the panic message names the contract so the operator sees
    /// the actual problem.
    fn executor(&self) -> Arc<CommandExecutor> {
        self.executor_slot.cloned().expect(
            "ChatModule: CommandExecutor not installed — \
             start_server must call install_executor_on_all \
             before any chat command can dispatch (task #224)",
        )
    }

    /// Resolve a pagination anchor to its stored `timestamp` (the field
    /// both cursor directions filter on) via `data/query` (limit 1,
    /// filter on id). Fails loud when the anchor doesn't exist — silently
    /// returning unfiltered history would hand the caller the wrong page.
    async fn anchor_timestamp(&self, anchor_id: Uuid) -> Result<String, String> {
        let anchor_query = json!({
            "dbPath": "main",
            "collection": CHAT_MESSAGES_COLLECTION,
            "filter": { "id": { "$eq": anchor_id.to_string() } },
            "limit": 1,
        });

        let anchor_result = self
            .executor()
            .execute_json("data/query", anchor_query)
            .await
            .map_err(|e| format!("chat/poll: anchor lookup failed: {e}"))?;

        extract_first_record_field(&anchor_result, "timestamp").ok_or_else(|| {
            // Matches the TS impl's "Message not found" path.
            format!("chat/poll: anchor message not found: {anchor_id}")
        })
    }

    /// `chat/poll` — return recent messages, optionally filtered by
    /// room and anchored to a pagination cursor in EITHER direction.
    ///
    /// Implementation strategy (mirrors the TS `ChatPollServerCommand`
    /// behavior):
    ///
    /// 1. If `after_message_id` is set: look up that message's
    ///    timestamp via `data/query` (limit 1, filter on id), use it as
    ///    a `$gt` filter on the main query. If `before_message_id` is
    ///    set (the scroll-back cursor): same lookup, used as a `$lt`
    ///    filter — the `limit` messages immediately preceding the anchor.
    /// 2. Apply optional `room_id` filter.
    /// 3. Sort `asc` when polling after an anchor (chronological), else
    ///    `desc` (latest-N / the page just before the anchor).
    /// 4. Query via `data/query` against the `chat_messages` collection.
    /// 5. Normalize back to chronological order for display regardless
    ///    of query direction.
    pub async fn poll(&self, params: ChatPollParams) -> Result<ChatPollResult, String> {
        let executor = self.executor();
        let limit = params.limit.unwrap_or(DEFAULT_POLL_LIMIT);

        // The two anchors are opposite scroll directions — both at once
        // has no coherent ordering. Fail loud, never guess.
        if params.after_message_id.is_some() && params.before_message_id.is_some() {
            return Err(
                "chat/poll: afterMessageId and beforeMessageId are mutually exclusive — \
                 page one direction at a time"
                    .to_string(),
            );
        }

        // ── Phase 1: resolve the anchor timestamp if the caller pinned
        //   a cursor (either direction) — the `$gt`/`$lt` bound below.
        let after_timestamp = match params.after_message_id {
            Some(anchor_id) => Some(self.anchor_timestamp(anchor_id).await?),
            None => None,
        };
        let before_timestamp = match params.before_message_id {
            Some(anchor_id) => Some(self.anchor_timestamp(anchor_id).await?),
            None => None,
        };

        // ── Phase 2: build the main query. Filter on room +/- anchor
        //   timestamp; sort direction follows whether we have an anchor.
        let mut filter = serde_json::Map::new();
        if let Some(room_id) = params.room_id {
            filter.insert("roomId".to_string(), json!({ "$eq": room_id.to_string() }));
        }
        if let Some(ts) = after_timestamp.clone() {
            filter.insert("timestamp".to_string(), json!({ "$gt": ts }));
        }
        if let Some(ts) = before_timestamp.clone() {
            filter.insert("timestamp".to_string(), json!({ "$lt": ts }));
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
            before_message_id: params.before_message_id,
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

    /// Persist one `chat:posted` projection into the durable chat store (#140).
    ///
    /// The projection is the ONE seam every room line crosses — a persona's
    /// `say()` and a human/web `chat/send` alike — so persisting here makes the
    /// transcript durable for BOTH speakers with one writer. Idempotent by id:
    /// a human line was already stored by `send()` under the same `messageId`
    /// (the envelope carries it through the projection), so the second write
    /// reports `success=false` (unique id) and is skipped with a probe — an
    /// EXPECTED duplicate, never an error. A persona line's id is airc's
    /// `event_id` (stable across replay), so restarts can't double-store either.
    pub async fn persist_posted(&self, payload: Value) -> Result<(), String> {
        let executor = self.executor();
        let field = |k: &str| -> Result<String, String> {
            payload
                .get(k)
                .and_then(Value::as_str)
                .map(String::from)
                .ok_or_else(|| format!("chat:posted payload missing `{k}`: {payload}"))
        };
        let message_id = field("messageId")?;
        let room_id = field("roomId")?;
        let sender_id = field("senderId")?;
        let content = field("content")?;
        let occurred_at_ms = payload
            .get("timestamp")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("chat:posted payload missing `timestamp`: {payload}"))?;
        let iso = now_iso(occurred_at_ms);

        // Same ChatMessageEntity shape `send()` writes — one entity, two writers
        // converging on one row id. `metadata.source: "user"` for both: personas
        // are citizens (users), not system lines; WHO spoke is `senderId`.
        let entity_data = json!({
            "id": message_id,
            "roomId": room_id,
            "senderId": sender_id,
            "timestamp": iso,
            "content": { "text": content },
            "replyToId": null,
            "metadata": { "source": "user" },
            "status": "sent",
        });
        let create_params = json!({
            "dbPath": CHAT_DATA_HANDLE,
            "collection": CHAT_MESSAGES_COLLECTION,
            "id": message_id,
            "data": entity_data,
        });
        let result = executor
            .execute_json("data/create", create_params)
            .await
            .map_err(|e| format!("chat:posted persist: data/create failed: {e}"))?;
        if result.get("success").and_then(Value::as_bool) != Some(true) {
            // Almost always the send()-already-stored duplicate (same id). Keep it
            // visible-but-calm: one probe line, never a hard error — a projector
            // that errors on every human message would drown real faults.
            let detail = result
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("success=false");
            crate::probe!(
                class = "chat.persist.skipped",
                message_id = message_id.as_str(),
                detail = detail,
                "chat:posted row not written (duplicate id or store refusal)"
            );
        }
        Ok(())
    }

    /// #265: seed the repetition-perception speech rings from the durable
    /// transcript at boot. The rings (`record_room_speech` /
    /// `record_own_speech`) are in-process statics, so a reboot wipes them at
    /// the exact moment they're most needed — the post-boot wake round, where
    /// every persona re-emits its greeting 2+ times with the repetition and
    /// [settled] facts blind (observed live 2026-07-30, twice). The durable
    /// store (#140) already holds the history; this replays the latest
    /// `RING_HYDRATION_SCAN` lines (chronological, all rooms — each ring keeps
    /// only its own bounded tail) so the facts have priors from the first tick.
    ///
    /// Waits boundedly for the executor slot — this runs from the persist
    /// listener spawned in `initialize`, which can race
    /// `install_executor_on_all` (the #201 ordering); a missing executor here
    /// is a not-yet, not a bug, so we poll the slot instead of panicking.
    pub async fn hydrate_speech_rings(&self) -> Result<(), String> {
        const RING_HYDRATION_SCAN: usize = 200;
        const EXECUTOR_WAIT: std::time::Duration = std::time::Duration::from_secs(15);
        const EXECUTOR_POLL: std::time::Duration = std::time::Duration::from_millis(250);

        let deadline = std::time::Instant::now() + EXECUTOR_WAIT;
        while self.executor_slot.cloned().is_none() {
            if std::time::Instant::now() >= deadline {
                return Err(
                    "executor not installed within boot window — rings start cold".to_string(),
                );
            }
            tokio::time::sleep(EXECUTOR_POLL).await;
        }

        let result = self
            .poll(ChatPollParams {
                room_id: None,
                after_message_id: None,
                before_message_id: None,
                limit: Some(RING_HYDRATION_SCAN),
            })
            .await?;

        let mut seeded = 0usize;
        for msg in &result.messages {
            // Stored ChatMessageEntity shape (see `persist_posted`): roomId,
            // senderId, content.text. Malformed rows are skipped, not fatal —
            // hydration is best-effort priors, never a boot gate.
            let room = msg
                .get("roomId")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok());
            let text = msg
                .get("content")
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str);
            let (Some(room), Some(text)) = (room, text) else {
                continue;
            };
            crate::cognition::deliberation_budget::record_room_speech(room, text);
            if let Some(sender) = msg
                .get("senderId")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                crate::cognition::deliberation_budget::record_own_speech(
                    crate::identity::PeerId::from_uuid(sender),
                    text,
                );
            }
            seeded += 1;
        }
        crate::probe!(
            class = "speech.rings.hydrated",
            seeded = seeded,
            scanned = result.count,
            "repetition-fact rings seeded from durable transcript (#265)"
        );
        Ok(())
    }
}

impl Default for ChatModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn the durable-transcript writer (#140): a bus-receiver task (same shape as
/// `cognition::dispatch_listener::spawn`) that persists every `chat:posted` projection —
/// the ONE seam a persona's `say()` and a human/web `chat/send` both cross — into the
/// chat store via [`ChatModule::persist_posted`]. This is a receiver task rather than a
/// module `event_subscription` because `chat:posted` is published with
/// `publish_async_only`, which feeds only the broadcast channel; module subscriptions
/// are dispatched exclusively by the synchronous `publish(..., registry)` path.
///
/// Spawned from `ChatModule::initialize` on the ModuleContext's runtime HANDLE —
/// registration runs on a non-tokio thread, and a bare `tokio::spawn` there panics the
/// boot ("no reactor running", observed live 2026-07-16). Idempotent per process:
/// initialize runs once per module.
pub fn spawn_persist_listener(
    handle: &tokio::runtime::Handle,
    bus: Arc<crate::runtime::message_bus::MessageBus>,
    module: Arc<ChatModule>,
) {
    let mut rx = bus.receiver();
    handle.spawn(async move {
        // #265: seed the repetition-fact speech rings from the durable
        // transcript BEFORE consuming live events — a reboot wipes the
        // in-process rings exactly when the wake-greeting round needs them.
        // The subscription above is already live, so no line is missed while
        // we hydrate; a hydration failure degrades to cold rings, loudly.
        if let Err(error) = module.hydrate_speech_rings().await {
            tracing::warn!(
                error,
                "speech-ring hydration failed — repetition facts start cold this boot (#265)"
            );
        }
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                // Lagged (slow consumer) — keep going; a persisted line missed under lag
                // is recoverable on the next send, and the store is not the wire.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            };
            if event.name != crate::ipc::positron_source::CHAT_POSTED {
                continue;
            }
            if let Err(error) = module.persist_posted(event.payload).await {
                // Loud but non-fatal: one malformed payload must not kill the
                // transcript writer for the rest of the process lifetime.
                tracing::warn!(error, "chat:posted persist failed (#140)");
            }
        }
    });
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
            // Chat doesn't use module event_subscriptions. The durable-
            // transcript writer (#140) rides `spawn_persist_listener` (a bus
            // receiver task, same shape as cognition::dispatch_listener)
            // because `chat:posted` is published via `publish_async_only`,
            // which feeds ONLY the broadcast channel — module subscriptions
            // are dispatched exclusively by the synchronous `publish(...,
            // registry)` path, so a subscription here would be dead wiring
            // (verified against message_bus.rs before wiring; the
            // "deferred tier" its docs mention is not built).
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        // #140: the durable-transcript writer — persists every `chat:posted`
        // projection (persona say + human chat/send, the one seam both cross).
        // Spawned here (inside the runtime) rather than at registration, which
        // runs on a non-tokio thread. `from_slot` shares this module's
        // late-bound executor, filled by install_executor_on_all before any
        // event can arrive from a live room.
        spawn_persist_listener(
            &ctx.runtime,
            ctx.bus.clone(),
            Arc::new(ChatModule::from_slot(self.executor_slot.clone())),
        );
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let _ = params;
        match command {
            // ── Migrated to the typed object registry ───────────────
            //
            // `chat/poll` + `chat/send` are now typed self-routing
            // `ActionCommand`s (see `crate::commands::chat`) that route
            // via `route_object` — the module contributes them through
            // `commands()`, capturing this module's shared executor slot.
            // Reaching this legacy path means a descriptor failed to
            // register; fail loud naming the canonical command rather
            // than silently re-handling. (Retired wholesale when
            // Registry A's trait default becomes fail-loud — #63.)
            "chat/poll" | "collaboration/chat/poll" | "chat/send"
            | "collaboration/chat/send" => Err(format!(
                "'{command}' is a migrated, typed chat command (chat/poll, chat/send) — it \
                 must route via the object registry (route_object), not the legacy \
                 handle_command path. Reaching here means its descriptor failed to register."
            )),

            // ── Staged migration stubs ──────────────────────────────
            //
            // The remaining commands still own their TS implementations
            // until their own follow-up PRs land. The kernel router
            // currently sees `chat/` claim these names (per
            // `command_prefixes` above) so the chat module's handler
            // returns a typed error with the upstream TS command name —
            // callers that need this surface go through the explicit
            // `CommandExecutor::execute_ts_json` API per
            // [[no-fallbacks-ever]] (task #219). The implicit dispatch
            // chain no longer crosses the bridge silently; the chat
            // module owns the prefix and emits a deterministic error.
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

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        // `LateBound::install` silently no-ops when the slot is already
        // filled, so a `with_executor()`-injected test executor is never
        // clobbered — the original install wins, same contract as before.
        self.executor_slot.install(executor);
    }

    /// The migrated `chat/poll` + `chat/send` commands as typed
    /// self-routing objects on the ONE registry. Both capture this
    /// module's shared late-bound executor slot (they reach `data/*` +
    /// `airc/*` through it at run time) and delegate to the canonical
    /// [`ChatModule::poll`] / [`ChatModule::send`] bodies. Their
    /// `CommandSpec` descriptors flow into `command_registry()` → the
    /// persona tool surface + grid ACL. (`chat/analyze` + `chat/export`
    /// still own their TS implementations until their own follow-up PRs
    /// — they are NOT contributed here.)
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::chat::command_objects(self.executor_slot.clone())
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

        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
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

    // what this catches: #140/#177 — the durable-transcript writer. A projected
    // chat:posted line (a persona's spoken reply — the shape that was an
    // airc+positron-only ghost, invisible to chat/poll, LIGHTHOUSE diagnosis
    // 2026-07-16) must land in the chat store as a ChatMessageEntity row under the
    // projection's messageId, with the logical sender and ISO timestamp. If this
    // write disappears, persona speech stops surviving the process again.
    #[tokio::test]
    async fn chat_posted_event_persists_a_persona_line_into_the_store() {
        let seen: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        let data = StubDataModule::new(move |command, params| {
            assert_eq!(command, "data/create", "persist path must write, not query");
            seen_in.lock().unwrap().push(params);
            Ok(json!({ "success": true }))
        });
        let chat = chat_with_stubs(vec![Arc::new(data)]);

        let message_id = Uuid::new_v4();
        let sender = Uuid::new_v4();
        let room = Uuid::new_v4();
        chat.persist_posted(json!({
            "messageId": message_id,
            "roomId": room,
            "senderId": sender,
            "content": "I see the word LIGHTHOUSE in the room.",
            "timestamp": 1_784_227_923_000u64,
        }))
        .await
        .expect("persist path succeeds");

        let calls = seen.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "exactly one durable write per projected line"
        );
        let p = &calls[0];
        assert_eq!(p["collection"], CHAT_MESSAGES_COLLECTION);
        assert_eq!(p["id"], message_id.to_string());
        assert_eq!(
            p["data"]["senderId"],
            sender.to_string(),
            "logical speaker attributed"
        );
        assert_eq!(
            p["data"]["content"]["text"],
            "I see the word LIGHTHOUSE in the room."
        );
        assert!(
            p["data"]["timestamp"]
                .as_str()
                .unwrap_or("")
                .starts_with("2026-"),
            "airc occurred_at_ms rendered as the ISO timestamp the entity carries: {}",
            p["data"]["timestamp"]
        );
    }

    // what this catches: idempotence of the two-writer convergence. A HUMAN line is
    // stored by send() first and then arrives again via its own projection under the
    // SAME messageId — the store refuses the duplicate (success=false) and the
    // projector must treat that as the expected no-op, never an error (an erroring
    // projector would fire on every human message and drown real faults).
    #[tokio::test]
    async fn duplicate_projection_of_a_sent_message_is_a_calm_no_op() {
        let data = StubDataModule::new(move |command, _params| {
            assert_eq!(command, "data/create");
            Ok(json!({ "success": false, "error": "unique constraint: id exists" }))
        });
        let chat = chat_with_stubs(vec![Arc::new(data)]);
        chat.persist_posted(json!({
            "messageId": Uuid::new_v4(),
            "roomId": Uuid::new_v4(),
            "senderId": Uuid::new_v4(),
            "content": "already stored by send()",
            "timestamp": 1_784_227_923_000u64,
        }))
        .await
        .expect("a duplicate row is an EXPECTED no-op, not an error");
    }

    // what this catches: THE WIRING, not just the logic (the dead-subscription trap this
    // slice nearly shipped). `chat:posted` is published via `publish_async_only`, which
    // NEVER dispatches module event_subscriptions — so the durable-transcript writer must
    // be a live bus-receiver task. This publishes through the exact same call the
    // projector uses and asserts the row write actually happens. If someone "simplifies"
    // the listener back into event_subscriptions, this goes red instead of persona speech
    // silently going ghost again.
    #[tokio::test]
    async fn persist_listener_receives_async_published_chat_posted() {
        use std::time::Duration;
        let seen: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        let data = StubDataModule::new(move |command, params| {
            assert_eq!(command, "data/create");
            seen_in.lock().unwrap().push(params);
            Ok(json!({ "success": true }))
        });
        let chat = Arc::new(chat_with_stubs(vec![Arc::new(data)]));
        let bus = Arc::new(crate::runtime::message_bus::MessageBus::new());
        // receiver created before publish; the tokio::test runtime is the handle
        spawn_persist_listener(&tokio::runtime::Handle::current(), bus.clone(), chat);

        bus.publish_async_only(
            crate::ipc::positron_source::CHAT_POSTED,
            json!({
                "messageId": Uuid::new_v4(),
                "roomId": Uuid::new_v4(),
                "senderId": Uuid::new_v4(),
                "content": "spoken by a persona",
                "timestamp": 1_784_227_923_000u64,
            }),
        );

        for _ in 0..100 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "the listener must persist the async-published room line"
        );
    }

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
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(
            |_p| json!({ "success": true, "data": [] }),
        ))]);

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

    // ── chat/poll: before_message_id path (the scroll-back cursor) ────

    // what this catches: the endless-scroll page — `beforeMessageId` must
    // resolve the anchor's timestamp, filter `$lt` (strictly OLDER), and
    // query DESC (the page immediately preceding the anchor, not the
    // oldest N in history). A regression here turns scroll-back into
    // either a forward page or a jump to the beginning of time.
    #[tokio::test]
    async fn poll_with_before_anchor_filters_lt_and_queries_desc() {
        let anchor_id = Uuid::new_v4();
        let anchor_str = anchor_id.to_string();
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |params| {
            let filter = &params["filter"];

            // Anchor lookup: filter on `id`, limit 1.
            if let Some(id_filter) = filter.get("id") {
                assert_eq!(id_filter["$eq"], anchor_str);
                return json!({
                    "success": true,
                    "data": [{
                        "id": anchor_str,
                        "data": { "id": anchor_str, "timestamp": "2026-05-30T12:00:00Z" }
                    }]
                });
            }

            // Main query: `$lt` bound from the anchor's timestamp, DESC.
            assert_eq!(filter["timestamp"]["$lt"], "2026-05-30T12:00:00Z");
            assert_eq!(params["sort"][0]["direction"], "desc");
            json!({
                "success": true,
                "data": [
                    { "id": "old-2", "data": { "id": "old-2", "timestamp": "2026-05-30T11:59:00Z" } },
                    { "id": "old-1", "data": { "id": "old-1", "timestamp": "2026-05-30T11:58:00Z" } }
                ]
            })
        }))]);

        let result = chat
            .poll(ChatPollParams {
                before_message_id: Some(anchor_id),
                ..Default::default()
            })
            .await
            .expect("before-anchor poll must succeed when the anchor exists");
        assert_eq!(result.count, 2);
        assert_eq!(result.before_message_id, Some(anchor_id));
        // Chronological normalization holds for the backward page too.
        assert_eq!(result.messages[0]["id"], "old-1");
        assert_eq!(result.messages[1]["id"], "old-2");
    }

    // what this catches: the two cursors are opposite scroll directions —
    // accepting both would silently produce an incoherent page. Must
    // fail loud, and must fail BEFORE any storage round-trip.
    #[tokio::test]
    async fn poll_rejects_both_cursor_directions_at_once() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_| {
            panic!("mutually-exclusive cursors must be rejected before any data/query");
        }))]);
        let err = chat
            .poll(ChatPollParams {
                after_message_id: Some(Uuid::new_v4()),
                before_message_id: Some(Uuid::new_v4()),
                ..Default::default()
            })
            .await
            .expect_err("both cursors at once must be rejected");
        assert!(err.contains("mutually exclusive"), "got: {err}");
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

    // ── chat/poll + chat/send: the typed object registry path ─────────
    //
    // These commands now route via `route_object` as typed
    // `ActionCommand`s (`crate::commands::chat`), NOT the legacy
    // `handle_command` envelope. The tests below prove the module
    // contributes exactly those two objects, that invoking one reaches
    // the shared executor slot and runs the canonical `ChatModule::poll`
    // body, and that the retired legacy arms fail loud.

    #[test]
    fn commands_contributes_poll_and_send_objects() {
        // what this catches: a regression that drops a command from the
        // module's `commands()` (so it never reaches `command_registry()`,
        // the persona tool surface, or the ACL) — or renames its wire key
        // away from the canonical `chat/poll` / `chat/send`.
        let names: Vec<&str> = ChatModule::new()
            .commands()
            .iter()
            .map(|c| c.name())
            .collect();
        assert!(names.contains(&"chat/poll"), "missing chat/poll: {names:?}");
        assert!(names.contains(&"chat/send"), "missing chat/send: {names:?}");
        assert_eq!(
            names.len(),
            2,
            "chat contributes exactly poll + send: {names:?}"
        );
    }

    #[tokio::test]
    async fn typed_chat_poll_object_invokes_over_shared_slot() {
        // what this catches: the typed `chat/poll` object must resolve the
        // module's shared late-bound executor (via `from_slot`) and delegate
        // to the canonical `ChatModule::poll` body — a regression that broke
        // the shared slot (empty executor) or the delegation would fail here.
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(
            |_p| json!({ "success": true, "data": [] }),
        ))]);
        let poll = chat
            .commands()
            .into_iter()
            .find(|c| c.name() == "chat/poll")
            .expect("chat/poll object must be contributed");

        let result = poll
            .invoke(json!({ "limit": 7 }), None)
            .await
            .expect("typed chat/poll must succeed");
        let CommandResult::Json(value) = result else {
            panic!("chat/poll must return CommandResult::Json");
        };
        assert_eq!(value["count"], 0);
        assert!(value["messages"].is_array());
    }

    #[tokio::test]
    async fn legacy_chat_arms_fail_loud() {
        // what this catches: the legacy `handle_command` path for the
        // migrated verbs must fail loud naming the command (never silently
        // re-handle), so a descriptor that failed to register surfaces
        // instead of a second implementation forking off the typed object.
        let chat = ChatModule::new();
        for command in [
            "chat/poll",
            "collaboration/chat/poll",
            "chat/send",
            "collaboration/chat/send",
        ] {
            let err = chat
                .handle_command(command, json!({}))
                .await
                .expect_err("migrated chat arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
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

        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
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
                assert_eq!(
                    cmd, "data/create",
                    "happy path only writes (no other data ops)"
                );
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
        assert!(
            !result.message_id.is_nil(),
            "message_id must be a real UUID"
        );
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
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(json!({ "success": true }))
            })),
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

        assert_eq!(
            create["dbPath"], "main",
            "writes go to the main adapter handle"
        );
        assert_eq!(create["collection"], "chat_messages");
        assert_eq!(
            create["id"],
            result.message_id.to_string(),
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
            entity["timestamp"].as_str().unwrap().ends_with('Z'),
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
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(json!({ "success": true }))
            })),
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

    // The `chat/send` typed-object path (module `commands()` contribution,
    // shared-slot invocation, legacy fail-loud) is proven alongside `chat/poll`
    // in the "typed object registry path" block above — both verbs share the
    // same `command_objects` family and the same `handle_command` fail-loud arm.

    // ════════════════════════════════════════════════════════════════
    // Multi-persona concurrency stress tests — gated behind the
    // `stress-tests` cargo feature so default `cargo test` doesn't
    // pay the compile cost of the multi-thread runtime + 50-task
    // futures::join_all bodies. Periodic CI runs:
    //     cargo test -p continuum-core --features stress-tests
    //
    // Per Joel 2026-06-08: "Yes half the battle is tests and we
    // wrote all this infra. Need to stop forgetting." Same shape as
    // `test-fixtures` — compile-time gating, not `#[ignore]`.
    // ════════════════════════════════════════════════════════════════
    //
    // Per Joel 2026-05-30: "Each persona exists in its own threads."
    // The kernel registers ONE ChatModule instance; every persona's
    // thread invokes its `&self` methods concurrently. The tests
    // below PIN the invariants the substrate is designed to uphold
    // under that load — they are not exercising rare paths, they are
    // the production scenario.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
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
                        let id_str = params["id"].as_str().expect("data/create must carry an id");
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

            let log: Arc<StdMutex<Vec<(Uuid, &'static str)>>> = Arc::new(StdMutex::new(Vec::new()));
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
                    airc_log.lock().unwrap().push((id, "airc/realtime-publish"));
                    Ok(airc_ok_response("evt-order-conc"))
                })),
            ]);
            let chat = Arc::new(chat);

            let mut tasks = Vec::with_capacity(PARALLEL);
            for _ in 0..PARALLEL {
                let chat = chat.clone();
                tasks.push(tokio::spawn(async move {
                    chat.send(sample_send_params()).await
                }));
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
    } // end mod stress

    // ── #265: ring hydration from the durable transcript ─────────────

    #[tokio::test]
    async fn hydrate_speech_rings_seeds_room_and_own_rings_from_durable_store() {
        // what this catches: reboot wipes the in-process speech rings, so the
        // repetition/[settled] facts are blind exactly during the post-boot
        // wake-greeting round (observed live 2026-07-30 — every persona
        // re-greeted 2+ times, facts silent). Hydration must replay the
        // durable transcript into BOTH the per-room and per-sender rings.
        // Fresh UUIDs per run: the rings are process-global, so unique keys
        // keep this isolated under full-suite parallelism (the #7 lesson).
        let room = Uuid::new_v4();
        let persona = Uuid::new_v4();
        let room_s = room.to_string();
        let persona_s = persona.to_string();

        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |_p| {
            // Store returns DESC (latest-first); poll normalizes chronological.
            json!({
                "success": true,
                "data": [
                    { "id": "m2", "data": { "id": "m2", "roomId": room_s, "senderId": persona_s,
                        "timestamp": "2026-07-30T12:00:01Z",
                        "content": { "text": "Hello everyone! I'm Benchy, back on the grid." } } },
                    { "id": "m1", "data": { "id": "m1", "roomId": room_s, "senderId": persona_s,
                        "timestamp": "2026-07-30T12:00:00Z",
                        "content": { "text": "the wordstats tests are green, posting output" } } }
                ]
            })
        }))]);

        chat.hydrate_speech_rings()
            .await
            .expect("hydration over a healthy store must succeed");

        let room_ring = crate::cognition::deliberation_budget::recent_room_speech(room);
        assert_eq!(
            room_ring.len(),
            2,
            "both durable lines must land in the room ring"
        );
        assert!(
            room_ring[1].contains("back on the grid"),
            "chronological order: the newest line is last (ring tail)"
        );

        let own_ring = crate::cognition::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(persona),
        );
        assert_eq!(
            own_ring.len(),
            2,
            "the sender's own-speech ring must carry her durable lines — this is \
             what lets own_repetition/inbound_restates fire on a post-boot re-greeting"
        );
    }
}
