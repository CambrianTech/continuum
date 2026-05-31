# `chat` module — Design

> **Status**: chat/poll + chat/send shipped in PR #1489 (Rust); chat/analyze + chat/export still on TS pending follow-up migrations.
>
> **File**: `src/workers/continuum-core/src/modules/chat/` (mod.rs + types.rs)
>
> **Canonical reference**: [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)

## Role

**Persona's primary I/O surface.** Per the three-primitive framing ([COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md §1](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)), chat serves **Persona** by providing **Commands** (chat/send, chat/poll) and indirectly **Events** (via airc realtime broadcasts on send).

Personas subscribe to airc room events to see incoming messages, then call `chat/send` to respond. Widgets connect to the same surface (subscribe + execute) — chat is the canonical example of a module that bridges human and AI consumers through identical primitives.

## Command surface

| Command | Params type | Result type | Status | Notes |
|---|---|---|---|---|
| `chat/poll` | `ChatPollParams` | `ChatPollResult` | ✅ Rust (PR #1489) | Read messages by room / anchor / limit |
| `chat/send` | `ChatSendParams` | `ChatSendResult` | ✅ Rust (PR #1489) | Write message + broadcast (data-first dual-write) |
| `chat/analyze` | TBD | TBD | ❌ TS stub | Pending migration with HandleRef + event streaming (field manual §5.3) |
| `chat/export` | TBD | TBD | ❌ TS stub | Pending migration |

Both `chat/*` (canonical) and `collaboration/chat/*` (legacy) prefixes route to this module — consumers migrate at their own pace.

## Cross-module dependencies

- **`data/query`** — chat/poll reads from `chat_messages` collection
- **`data/create`** — chat/send writes to `chat_messages` (the persistence primary)
- **`airc/realtime-publish`** — chat/send broadcasts to airc (the delivery secondary)

All cross-module calls go through `executor.execute_json(...)`. Chat depends on data + airc through the command surface only — no Rust-type imports across module boundaries.

## State model

**Stateless.** The `ChatModule` struct carries only an optional executor override behind an `RwLock<Option<Arc<CommandExecutor>>>` for test injection. No per-resource locks; no in-memory caches; no shared mutable state across calls.

```rust
pub struct ChatModule {
    executor_override: RwLock<Option<Arc<CommandExecutor>>>,
}
```

If future migrations make chat stateful (e.g., a chat/analyze HandleRef map), the per-resource lock pattern from [field manual §4.1](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) applies. Today's surface doesn't need it.

## Events emitted

**Indirect via airc.** chat/send constructs an `AircRealtimeEnvelope` with `payload.kind = "existing_schema"` + `schema = "chat_transcript"` and publishes via `airc/realtime-publish`. Subscribers on the room (other personas, widgets, peers on the grid) see the message through airc's replay store.

The envelope's `inline` payload carries `{ messageId, text, senderId, replyToId }` — enough for subscribers to render the message without needing a separate data/query lookup.

**Future events** (when chat/analyze migrates per field manual §5.3):
- `chat:analyze:finding` — per-finding emission during a run
- `chat:analyze:complete` — run terminal event
- `chat:analyze:cancelled` — caller-initiated abort

## Concurrency contract

**Safe by construction.** The handler is `&self`, mints a fresh `Uuid` per send, and holds no shared mutable state. Multiple personas calling `chat/send` concurrently produce distinct messages with distinct ids; no per-call interference.

### Pinned invariants (multi-thread tests in `chat::tests`)

1. **`send_under_concurrent_load_stores_all_messages_with_distinct_ids`** — 50 concurrent sends; every message stored, every id distinct, stored set ≡ returned set (no losses, no phantoms)
2. **`send_preserves_per_call_ordering_under_concurrent_load`** — 25 concurrent sends; per-call `data/create` MUST precede per-call `airc/realtime-publish` across the interleaved global log
3. **`send_isolates_mixed_outcomes_under_concurrent_load`** — 30 concurrent sends with half airc-failing; each call's `warning` references THIS call's `message_id`, no cross-contamination
4. **`poll_isolates_results_under_concurrent_load`** — 30 concurrent polls each targeting a different room; every task receives ITS OWN room's result

Every test runs `flavor = "multi_thread", worker_threads = 4` so tasks preempt across OS threads. Single-threaded tokio would silently serialize and pass even if the handler had a data race.

### Dual-write partial-failure semantics (chat/send)

| Primary (data) | Secondary (airc) | Handler returns |
|---|---|---|
| ok | ok | `Ok(ChatSendResult { message_id, event_id: Some(...), warning: None })` |
| ok | fail | `Ok(ChatSendResult { message_id, event_id: None, warning: Some("airc/realtime-publish failed: ...") })` — degraded success |
| fail | — | `Err("chat/send: data/create failed: ...")` — secondary NEVER called |

**Data-first ordering** is the invariant that prevents bad-divergence (peers seeing a message the node didn't store). Pinned by `send_calls_data_before_airc`.

**airc-only failure is NOT command-level failure.** The message IS in the local store; consumers see it via chat/poll; a future retry/sync mechanism heals the broadcast. The `warning` field is the substrate's canonical shape for degraded success.

## Migration notes

**Rethink-not-port applied** per [field manual §5](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md):

| TS shape (`ChatSendServerCommand`) | Rust rethink | Why |
|---|---|---|
| Took `room: string` and resolved name → uuid inside the handler | Takes already-resolved `room_id: Uuid` | Name resolution belongs to caller/CLI (or future `channel/resolve` command) — kernel handler stays compositional |
| Sender priority chain (explicit → owner → fallback) inside handler | Takes already-resolved `sender_id: Uuid` | Same — identity resolution belongs upstream |
| Returned `{ ok, eventId, roomId, error? }` with `eventId` always present | Returns `{ messageId, eventId?, warning? }` with `eventId` ONLY when broadcast succeeded | Degraded success has its own shape; caller distinguishes "stored + broadcast" from "stored only" |
| Synchronous full media externalization (base64 → blob storage) inside handler | Media externalization **deferred** | First migration scopes to the dual-write substrate stress; media is its own kink-finder |
| Vision pre-warming fire-and-forget | **Deferred** | Same scoping; will return when vision module migrates |

The command-name surface is preserved (`collaboration/chat/send` + `chat/send` both work) so TS consumers see no break.

### Deferred for follow-up PRs

- chat/analyze — migrate with HandleRef + `chat:analyze:*` events per field manual §5.3
- chat/export — straightforward read+format; low priority
- Sender resolution priority chain — when user module migrates
- Room name resolution — when channel module gets a `channel/resolve` command
- Media externalization — separate scope; needs MediaBlobService rethink
- Vision pre-warming — when vision module migrates
- Reply-to threading metadata richer than `replyToId` — when thread tracking design lands
- **Idempotency**: a retried `chat/send` currently produces two stored messages. Matches today's TS behavior. Future PR can add `client_dedup_id` + TTL'd dedup map; the substrate is ready for it but the design is its own scope.

## Kinks found

None at correctness level — the dual-write design + multi-thread tests caught the design space before it caused bugs. Substrate gaps flagged for potential future refinement:

1. **Hand-rolled airc envelope JSON.** chat hand-codes the `json!({...})` for `airc/realtime-publish`. If a second module needs to publish to airc from Rust, an `airc::realtime_publish_envelope(...)` builder would distill the wire shape. Flagged in PR #1489 commit message — waiting for second consumer before distilling.

2. **No typed cross-module command call.** chat uses `executor.execute_json(...)` with raw JSON in/out and parses responses via `.get("success")`. A typed `executor.execute_typed::<P, R>(...)` would catch wire-shape drift at compile time. Same shape as the `handle_id_or_legacy` refinement (PR #1491) solved for handle resolution. Flag for if/when a second consumer appears.

3. **No transaction primitive across modules.** chat hand-codes the data-first / airc-best-effort ordering inline. A substrate-level `dual_write!(primary => ..., best_effort => ...)` macro could centralize the partial-failure pattern if a second consumer appears.

The pattern across all three: **wait for the second consumer before distilling into substrate.** Single consumer = interesting; second consumer = pattern. Same rule that produced `expect_owned_by` + `handle_id_or_legacy` from the data-query consumer (PR #1491).

## References

- PR #1489 — ChatModule (chat/poll + chat/send + concurrency tests)
- PR #1486 — `CommandRequest<P>` / `CommandResponse<T>` envelopes used here
- PR #1485 — Cell shapes (HandleRef ready for chat/analyze migration)
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) §3 (Module Design Template), §4 (Concurrency doctrine), §5 (Migration playbook)
- Memory: `three-primitives-commands-events-persona`, `chat-extracts-to-airc`
