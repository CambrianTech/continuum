# Command Infrastructure: Field Manual

> **Premise** (Joel, 2026-05-30): *"We have the entire picture now. We have our grid, our chat protocols, bus, one built for the needs of continuum AND current and future systems. Let's make sure we have detailed designs for this command infrastructure into modules and properly built from the ground up by using our own generators."*

This is the field manual for module authors. The architectural **why** lives in [MODULE-ARCHITECTURE.md](MODULE-ARCHITECTURE.md), the runtime contract lives in [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md), and the **which modules exist** survey lives in [MODULE-CATALOG.md](MODULE-CATALOG.md). This document is the operational **how**: substrate API, module template, concurrency doctrine, migration discipline, generator usage.

If you're sitting down to author a new module right now, read this. If you want to understand the principle behind the architecture, read the three above.

---

## 1. The system in one sentence

> Continuum is exactly three primitives — **Commands**, **Events**, **Persona** — in Rust. airc handles grid (peer discovery + signing + delivery). Widgets are thin event-subscribers + command-callers. Everything else is supporting cast.

This isn't aspiration; it's the working model from PRs #1483–#1492. Every module either provides commands, emits events, or is consumed by a persona. If a proposed module doesn't map onto one of those three, push back on the design.

## 2. Substrate primitives (quick reference)

The substrate gives every module the same four building blocks. Reach for them before reinventing anything.

### 2.1 `ServiceModule` trait — the floor

Every module implements one trait:

```rust
#[async_trait]
pub trait ServiceModule: Send + Sync {
    fn config(&self) -> ModuleConfig;
    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String>;
    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String>;
    fn as_any(&self) -> &dyn std::any::Any;
}
```

`ModuleConfig` declares the module's `name`, `command_prefixes` (e.g. `["chat/", "collaboration/chat/"]`), `event_subscriptions`, `priority`, and optional `tick_interval`. The runtime registry routes any command whose prefix matches to this module's `handle_command`.

`as_any` lets the runtime downcast to the concrete module type when needed (test infra, runtime control queries).

**Reference:** `core/continuum-core/src/runtime/service_module.rs`

### 2.2 `CommandRequest<P>` / `CommandResponse<T>` — typed envelopes

Every new handler parses its inbound `Value` into a typed `CommandRequest`, runs the logic on typed params, and materializes a typed `CommandResponse` at the exit:

```rust
"chat/poll" | "collaboration/chat/poll" => {
    let req = CommandRequest::<ChatPollParams>::from_value(params)?;
    let result = self.poll(req.params).await?;
    CommandResponse::ok(result).into_command_result()
}
```

The envelope carries the command-specific `params` flattened with cross-cutting fields the kernel can populate: `handle: Option<HandleRef>`, `session_id: Option<Uuid>`, `user_id: Option<Uuid>`. The response envelope flattens `data: T` with `success: bool`, `error: Option<String>`, `handle: Option<HandleRef>`.

**Why typed envelopes**: handlers stop re-parsing the cross-cutting bits themselves. The cross-cutting fields become free.

**Reference:** `core/continuum-core/src/runtime/command_envelope.rs` (PR #1486)

### 2.3 `HandleRef` + four cell shapes — long-running state

Commands return one of four cell shapes:

| Shape | Use for | Status |
|---|---|---|
| `Value` (`CommandResult::Json` / `Binary`) | Immediate typed result | Mainline |
| `Handle` (`CommandResult::Handle(HandleRef)`) | Reference to producer-owned state | **Mainline (PR #1485)** |
| `Stream` | Async sequence of values | Reserved variant; wire protocol TBD |
| `Lambda` | Callable returned by a command | Reserved variant; protocol TBD |

`HandleRef` is the cell answer to long-running stateful work. The producer mints a UUID, stores its state under that UUID, returns the handle. Subsequent calls thread the handle; the producer's handler does an O(1) state-map lookup.

```rust
let id = Uuid::new_v4();
self.sessions.insert(id, SessionState::new(params));
CommandResponse::ok(StartData { first_token })
    .with_handle("ai/inference", id, "ai::InferenceSession")
    .into_command_result()
```

**The producer owns the lifetime.** Consumers holding a stale handle get a typed "handle not found" error from the producer. The kernel doesn't participate in handle lifetime management — that policy belongs to the producer.

**Cross-machine.** A handle minted on machine A is meaningful only on A. If a consumer on B calls a command taking that handle, the grid interceptor routes the call back to A (per `handle.owner`). The handle ID never leaves A's state map.

**Reference:** `core/continuum-core/src/runtime/cell_shapes.rs` (PR #1485)

### 2.4 `HandleRef::expect_owned_by` — handle validation

Every consumer that receives a `HandleRef` validates it before lookup:

```rust
let cursor_id = handle.expect_owned_by("data", "data::QueryCursor")
    .map_err(|e| format!("data/query-next: {e}"))?;
```

This is the canonical handle-validation entry point. Returns `Result<Uuid, String>` — the inner UUID on success, a typed error naming BOTH the offending value AND the expected value on mismatch. Owner mismatch is checked first (owner determines routing) with a hint about the grid interceptor's responsibility.

**Why this matters.** Without owner validation, a handle minted by module A reaching module B's handler would silently miss in B's state map ("not found") instead of surfacing as a routing bug. The fail-loud diagnostic turns a head-scratcher into a one-line fix.

**Reference:** `core/continuum-core/src/runtime/cell_shapes.rs::HandleRef::expect_owned_by` (PR #1491)

### 2.5 `CommandRequest::handle_id_or_legacy` — dual-shape resolver

For migrations from string-typed ids to typed handles, the substrate provides one resolver. Walks the envelope's `handle` first (validated via `expect_owned_by`), falls back to a legacy string field, errors loud when neither is present:

```rust
let cursor_id = req.handle_id_or_legacy(
    "data",                   // expected owner
    "data::QueryCursor",      // expected type_tag
    "queryId",                // legacy field name (for the error)
    &req.params.query_id,     // legacy field value
    "data/query-next",        // command name (for error prefix)
)?;
```

Both wire shapes resolve to the same id; the typed envelope wins when both are present. Use this anywhere you're migrating a stringly-typed resource id to a HandleRef while keeping back-compat.

**Reference:** `core/continuum-core/src/runtime/command_envelope.rs::CommandRequest::handle_id_or_legacy` (PR #1491)

### 2.6 Interceptor chain — transports as composable interceptors

Every command walks the same dispatch chain regardless of which language or machine implements it:

1. **Interceptors** in insertion order (`[airc, grid]` today). Each gets first look at `(command, params)`. Returns `Handled(result)` (short-circuits the chain), `Decline` (try next), or `Err` (propagates — no silent fallthrough).
2. **Local Rust module registry**. If no interceptor took the command, find a ServiceModule whose `command_prefixes` match.
3. **TypeScript via Unix socket**. Falls through to the existing CommandRouterServer for any TS-implemented command.

The chain is the same primitive for every transport: local Rust, remote Rust over grid, remote Rust over airc, TS over IPC. Adding a transport is adding an interceptor; no kernel changes needed.

**Reference:** `core/continuum-core/src/runtime/command_executor.rs`, `command_interceptor.rs` (PRs #1483/#1484)

### 2.7 Cross-module calls

Modules don't import each other's internal types. They communicate via commands through the kernel executor:

```rust
let executor = crate::runtime::command_executor::executor();
let result = executor.execute_json("data/query", json!({
    "dbPath": "main",
    "collection": "chat_messages",
    "filter": filter,
    "sort": [{ "field": "timestamp", "direction": "desc" }],
    "limit": 50,
})).await?;
```

That's it. Chat → data, chat → airc, persona → cognition — every cross-module call goes through the executor. No direct trait dependencies, no shared structs across module boundaries. Coupling lives at the wire surface, where it can be tested.

## 3. Module Design Template

Every ServiceModule follows the same shape. The generator (PR #1487) scaffolds modules in this shape; humans fill in handler bodies. The template:

```
core/continuum-core/src/modules/<name>/
├── mod.rs              // ServiceModule impl, command dispatch, public methods
├── types.rs            // CommandRequest/Response params + result types, ts-rs exports
├── DESIGN.md           // (future) Per-module design pinning the contract
└── README.md           // Author-facing scaffolded summary
```

`mod.rs` shape:

```rust
//! <Name>Module — <one-line purpose>.
//!
//! Per [MODULE-ARCHITECTURE.md](../../../../../../docs/architecture/MODULE-ARCHITECTURE.md):
//! [which of the three primitives this serves]
//!
//! # Cross-module dependencies
//! - data/* for persistence
//! - airc/* for broadcast
//! - <etc>

use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use crate::runtime::{
    command_executor::{self, CommandExecutor},
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

pub mod types;
use types::{...};

pub struct <Name>Module {
    /// Per-resource locks for any handler that holds mutable state
    /// across an `.await` or shared filesystem invariant.
    /// (Only present if the module has stateful handlers.)
    resource_locks: dashmap::DashMap<ResourceId, Arc<tokio::sync::Mutex<ResourceState>>>,

    /// Optional executor override for tests. Production uses the
    /// kernel-global; tests inject a registry with stub modules so
    /// cross-module calls are observable + assertable.
    executor_override: RwLock<Option<Arc<CommandExecutor>>>,
}

impl <Name>Module {
    pub fn new() -> Self { ... }

    #[cfg(test)]
    pub fn with_executor(executor: Arc<CommandExecutor>) -> Self { ... }

    fn executor(&self) -> Arc<CommandExecutor> {
        // tests: injected; production: kernel-global
    }

    /// Typed handlers as `&self` methods. Tests call them directly.
    pub async fn my_handler(&self, params: MyHandlerParams) -> Result<MyHandlerResult, String> {
        let executor = self.executor();
        // ... cross-module calls via executor.execute_json(...) ...
    }
}

#[async_trait]
impl ServiceModule for <Name>Module {
    fn config(&self) -> ModuleConfig { ... }
    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> { Ok(()) }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "<name>/<verb>" => {
                let req = CommandRequest::<MyHandlerParams>::from_value(params)?;
                let result = self.my_handler(req.params).await?;
                CommandResponse::ok(result).into_command_result()
            }
            other => Err(format!(
                "{other}: not handled by <name> module — known commands are <name>/<verb>"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
```

`types.rs` shape:

```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/<name>/MyHandlerParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct MyHandlerParams {
    #[ts(type = "string")]
    pub some_id: Uuid,
    pub some_text: String,
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub optional_anchor: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/<name>/MyHandlerResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct MyHandlerResult {
    #[ts(type = "string")]
    pub message_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warning: Option<String>,
}
```

**Rules:**
- **Every wire type carries `#[derive(TS)]`** — no hand-written types crossing the Rust↔TS boundary
- **`#[ts(type = "string")]` on UUIDs** — wire format is canonical string
- **`#[serde(skip_serializing_if = "Option::is_none")]` on optional output fields** — clean wire shape, missing = absent (not null)
- **`rename_all = "camelCase"`** on every params/result struct — matches the existing wire contract

**Reference modules to crib from:** `chat/`, `generator/` (scaffolded directories); `data/`, `airc/` (single-file modules — DESIGN.md docs forthcoming).

## 4. Concurrency doctrine

Per Joel 2026-05-30: *"Each persona exists in its own threads."* The kernel registers ONE module instance; every persona's thread invokes its `&self` methods concurrently against the same executor. The substrate's guarantees must hold under that load. Two real bugs were caught this session by enforcing this discipline (PR #1490 + PR #1487); the doctrine below is what catches them.

### 4.1 Per-resource locks, not module-wide

Every ServiceModule that holds per-resource mutable state across an `.await` MUST hold a per-resource lock for the read-then-async-then-write window. Module-wide locks are wrong (they serialize unrelated resources). Per-resource locks via `DashMap<Id, Arc<Mutex<State>>>` are the canonical pattern.

```rust
struct MyModule {
    // ✅ Per-resource: different ids stay parallel; same-id serialized.
    state_map: DashMap<ResourceId, Arc<tokio::sync::Mutex<ResourceState>>>,
}

async fn handler(&self, id: ResourceId) -> Result<(), String> {
    // Clone the Arc<Mutex> OUT of the DashMap shard's lock — cheap,
    // no contention beyond the brief shard read.
    let lock = self.state_map.get(&id)
        .map(|entry| entry.value().clone())
        .ok_or("not found")?;

    // Acquire the per-resource mutex for the full read-async-write window.
    let mut state = lock.lock().await;
    // ... read state ...
    let outcome = self.do_async_work(state.snapshot()).await?;
    state.apply(outcome);
    Ok(())
}
```

**`tokio::sync::Mutex` vs `std::sync::Mutex`:**
- Use `tokio::sync::Mutex` when the critical section holds an `.await` (the async work runs while the lock is held).
- Use `std::sync::Mutex` when the critical section is purely sync (filesystem, in-memory mutation, no async). Cheaper; doesn't risk task-park complexity.

**Module-wide locks are acceptable when:**
- Correctness is the priority and contention is low (e.g., `InMemoryAircRealtimeStore` for moment-of-truth scenarios — handful of personas)
- A future refactor to per-resource sharding is straightforward and flagged (e.g., shard by room_id when persona count grows)

### 4.2 Concurrency stress tests are mandatory

Every module with stateful handlers needs at least one multi-thread stress test pinning the per-resource invariants:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_handlers_dont_corrupt_state() {
    const PARALLEL: usize = 50;
    let module = Arc::new(MyModule::new());

    let mut tasks = Vec::with_capacity(PARALLEL);
    for _ in 0..PARALLEL {
        let module = module.clone();
        tasks.push(tokio::spawn(async move {
            module.handler(...).await
        }));
    }
    let results = futures::future::join_all(tasks).await;
    // Assert: no losses, distinct ids, ordering invariants per resource, etc.
}
```

**Why `flavor = "multi_thread", worker_threads = 4`:**
single-threaded tokio would silently serialize even genuinely racy code and pass. A multi-threaded runtime actually preempts across OS threads — race windows open. PR #1490's `same_cursor_concurrent_next_does_not_corrupt_state` test panicked with *"page 1 served 8 times — the cursor advanced through it MORE than once, indicating a lost serialization"*. Single-threaded tokio would have passed silently.

**Test patterns to copy:**
- **N parallel writers, assert no losses + distinct ids**: `chat/send` (PR #1489)
- **N parallel writers + concurrent readers, assert consistent snapshots**: `airc/realtime_store` (PR #1492)
- **Same-id parallel writers, assert serialization holds**: `data/query-next` (PR #1490)
- **N parallel ops on the same resource, assert one wins (with `force=false`) or consistent final state (with `force=true`)**: `generate/module` (PR #1487)

### 4.3 Partial-failure semantics (dual-write composition)

When a handler calls two cross-module commands in sequence (e.g., `chat/send` calls `data/create` then `airc/realtime-publish`), commit to explicit partial-failure semantics:

| Primary | Secondary | Handler returns |
|---|---|---|
| ok | ok | `Ok(result)` |
| ok | fail | `Ok(result with warning field)` — degraded success |
| fail | — | `Err(...)` — secondary NEVER called |

The ordering invariant (primary before secondary) must be pinned by a test. The "degraded success" pattern uses a `warning: Option<String>` field on the result type — naming the failing surface, surfacing the underlying error, confirming the primary write isn't lost.

**Reference:** `chat/send` in `core/continuum-core/src/modules/chat/mod.rs` (PR #1489), `send_calls_data_before_airc` + `send_with_airc_failure_returns_warning_and_null_event_id` tests.

## 5. Migration playbook: rethink, don't port

Per Joel 2026-05-30: *"We can just move the logic from nodejs by writing far better rust forms, rather than porting, by using them in airc for example, by command name and functionality/params/return rethought one at a time for efficiency and elegant patterns."*

The TS impl is a **reference for behavior to preserve**, not a template for shape. Every command migration is a small substrate win, not a translation.

### 5.1 Pre-migration checklist

Before typing any Rust, answer:

1. **Which of the three primitives does this serve?** (Commands / Events / Persona — if none, push back.)
2. **Should this be one call, or mint-handle-then-poll?** (If the work runs longer than ~100ms or produces incremental results, prefer a HandleRef.)
3. **Should the result be inline data or events the caller subscribes to?** (If subscribers other than the caller care about progress, prefer events.)
4. **Are the params already-resolved IDs (kernel-pure) or do they drag in name resolution (kernel-leaky)?** (Resolution belongs in browser/CLI or a future `*/resolve` command, not the kernel handler.)
5. **Does the response need a `warning` field for degraded success?** (Any handler that touches two cross-module calls almost always does.)

### 5.2 Substrate checklist (every Rust migration)

- [ ] `CommandRequest<P>` / `CommandResponse<T>` envelopes at handler entry + exit
- [ ] `HandleRef` for long-running state; `expect_owned_by` for validation
- [ ] Per-resource locks via `DashMap<Id, Arc<Mutex<State>>>` if handler holds mutable state across `.await`
- [ ] Multi-thread concurrency stress tests pinning invariants
- [ ] ts-rs bindings via `#[derive(TS)]` on every wire type
- [ ] camelCase serde rename on all wire structs
- [ ] Cross-module calls go through `executor.execute_json(...)` — no direct trait dependencies
- [ ] Per-module mod.rs + types.rs split (see Module Design Template above)

### 5.3 Worked example (chat/analyze, the next chat migration)

**TS impl today:** synchronous full-table scan of up to 500 messages, returns one blob of duplicates + timestamp anomalies. Fire-and-forget shape; no progress feedback; the analyzer holds the caller's thread for the whole scan.

**Rust rethought:**

```rust
// Mint a handle, return immediately
"chat/analyze" → CommandResponse::ok(AnalyzeStarted { started_at_ms, run_id })
    .with_handle("chat", run_id, "chat::AnalyzeRun")

// Stream findings via events while the analyzer chews through messages
events/emit "chat:analyze:finding" { runHandle, finding }

// Caller can poll for accumulated findings, or block until done
"chat/analyze/findings" { handle, since_cursor? } → list since cursor
"chat/analyze/complete" { handle } → blocks until run finishes
"chat/analyze/cancel" { handle } → aborts in-flight run
```

Per-handle `tokio::sync::Mutex` serializes concurrent polls on the same run. Same command-name namespace as TS preserves discoverability; entirely different (better) shape because the substrate now supports it. airc can publish the events to subscribers on other machines without any chat-specific protocol — it's just events on the room.

## 6. Generator usage

The GeneratorModule (PR #1487) scaffolds new ServiceModule directories. Eat your own dogfood — don't hand-author when the generator works.

```bash
./jtag generate/module \
  --name "chat-analyze" \
  --description "Long-running chat-message analysis with HandleRef + event streaming" \
  --commands "chat/analyze,chat/analyze/findings,chat/analyze/complete,chat/analyze/cancel" \
  --events-published "chat:analyze:finding,chat:analyze:complete,chat:analyze:cancelled" \
  --priority normal
```

Produces:

```
core/continuum-core/src/modules/chat_analyze/
├── mod.rs          // ServiceModule scaffold with command_prefixes + dispatch arms
└── README.md       // Author-facing summary + wire-up reminder
```

Generated `mod.rs` is compilable as soon as the author wires `pub mod chat_analyze;` into `modules/mod.rs` and registers `Arc::new(ChatAnalyzeModule::new())` at runtime startup. Each declared command's dispatch arm returns a typed "not yet implemented" `Err` — fill in the real handler.

**Generator concurrency invariants:** per-name lock serializes same-name concurrent generators (one wins without `--force`, consistent torn-free state with `--force`); different names stay fully parallel. Tested in `same_name_concurrent_generation_without_force_yields_one_winner` etc. (PR #1487).

### 6.1 Generator v2 roadmap (proposed, separate PR)

The current generator emits the bare minimum compilable scaffold. The next iteration enriches it to match the Module Design Template in §3:

- **types.rs scaffold** with envelope-pattern boilerplate (typed params/result with ts-rs)
- **tests module** with the multi-thread concurrency stress-test skeleton pre-primed
- **DESIGN.md scaffold** with section headers for the module's contract
- **Per-resource lock scaffold** when the spec declares stateful handlers (`--stateful` flag)
- **Cross-module dependency declarations** so the scaffold imports + tests stub the right downstream modules

Future commands the generator should provide:
- `generate/command` — add a command handler to an existing module (wires dispatch, emits types, adds test stub)
- `generate/refresh` — re-scan the modules tree and refresh manifests + barrels

## 7. Acceptance criteria for "module-ready"

A module is ready to merge when:

1. **Tests pass** — `cargo test --package continuum-core --lib --features metal,accelerate -- modules::<name>`
2. **ts-rs bindings land** — `npx tsx generator/generate-rust-bindings.ts` produces no drift
3. **At least one multi-thread concurrency stress test exists** if the module has stateful handlers
4. **Cross-module calls go through the executor** — no direct trait dependencies on other modules
5. **The module's wire contract is pinned by tests** — params shape, result shape, error format
6. **PR description names which of the three primitives the module serves**
7. **Substrate doctrine is followed end-to-end** (§5.2 checklist)

When all seven hold, the module is *concurrency-clean, wire-clean, and ready for the headless integration test.* That's the bar.

## 8. See also

- [MODULE-ARCHITECTURE.md](MODULE-ARCHITECTURE.md) — the architectural doctrine (every module is a package, addressed two ways, kernel has zero privileged operations)
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the RTOS-style runtime contract (concurrency, scheduling, memory + device pressure, telemetry, artifact handles, lifecycle)
- [MODULE-CATALOG.md](MODULE-CATALOG.md) — every Continuum concern as a focused ServiceModule, with line-count estimates
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — the artifact-sharing economy on top of the substrate
- Memory: `[[three-primitives-commands-events-persona]]`, `[[rethink-dont-port-commands-to-rust]]`, `[[headless-rust-must-work-soon]]`

## 9. PR references for everything cited

| Substrate piece | PR | File |
|---|---|---|
| `CommandInterceptor` chain | #1483 | `runtime/command_interceptor.rs` |
| `GridInterceptor` | #1484 | `runtime/grid_interceptor.rs` |
| `HandleRef` + cell shapes | #1485 (merged) | `runtime/cell_shapes.rs` |
| `CommandRequest` / `CommandResponse` | #1486 | `runtime/command_envelope.rs` |
| `GeneratorModule` (recursive bootstrap) | #1487 | `modules/generator/` |
| `HandleRef::expect_owned_by`, `CommandRequest::handle_id_or_legacy` | #1491 | `runtime/cell_shapes.rs`, `runtime/command_envelope.rs` |
| `ChatModule` (poll + send + concurrency tests) | #1489 | `modules/chat/` |
| `data/query` HandleRef migration + per-cursor mutex | #1490 | `modules/data.rs` |
| `airc/realtime` concurrency stress tests | #1492 | `airc/realtime_store.rs` |

This manual will be updated as the substrate evolves. When you change a primitive or land a new module pattern, update the relevant section here so the next author starts from the right floor.
