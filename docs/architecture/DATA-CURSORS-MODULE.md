# `data/query` cursors — Design

> **Scope**: this doc covers the cursor surface only — `data/query-open` / `data/query-next` / `data/query-close`. The data module has other concerns (CRUD, vector search, migration, batch ops) which are out of scope here; each will get its own design page as it migrates.
>
> **Status**: HandleRef migration + per-cursor mutex fix shipped in PR #1490.
>
> **File**: `src/workers/continuum-core/src/modules/data.rs` (single-file module; cursor surface is one of several concerns)
>
> **Canonical reference**: [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)

## Role

**Commands** primitive, serving **persona / widget consumers that need bounded pagination over arbitrary collections**. The cursor surface is the **first real consumer of HandleRef** — the mint-handle-then-poll pattern Joel called out for inference / training / hosting / ORM. Validating it on the data layer proved the substrate's promise before any other module reached for it.

## Command surface

| Command | Params type | Result type | Role |
|---|---|---|---|
| `data/query-open` | `QueryOpenParams` | (returns `{success, data: {queryId, ...}, handle}`) | Mint a cursor — returns BOTH the typed HandleRef AND the legacy queryId string for the same underlying UUID |
| `data/query-next` | `CommandRequest<QueryNextParams>` (handle OR queryId) | (returns `{success, data: {items, pageNumber, ...}}`) | Advance the cursor; resolve cursor id from envelope handle (preferred) or legacy field (back-compat) |
| `data/query-close` | `CommandRequest<QueryCloseParams>` (handle OR queryId) | (returns `{success, queryId}`) | Release cursor state |

### Dual-shape resolution

Per [field manual §2.5](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md), every additive migration of a stringly-typed id to a typed HandleRef uses one resolver:

```rust
let cursor_id = req.handle_id_or_legacy(
    DATA_MODULE_OWNER,        // "data"
    QUERY_CURSOR_TYPE_TAG,    // "data::QueryCursor"
    "queryId",
    &req.params.query_id,
    "data/query-next",
)?;
```

- **Envelope `handle`** present → validated via `HandleRef::expect_owned_by`, returns inner UUID as string
- **Legacy `queryId`** string present → returned as-is
- **Neither** → typed error naming BOTH supported shapes
- **Both** → envelope wins (so consumers mid-migration don't diverge from new consumers)

## Cross-module dependencies

- **`orm::adapter::StorageAdapter`** (internal to the data module's substrate) — actual SQLite/Postgres execution
- **`orm::query::{StorageQuery, SortSpec, FieldFilter}`** — typed query AST

No cross-module command calls — the cursor surface is data-internal.

## State model

Per-cursor state under per-cursor lock:

```rust
pub struct DataModule {
    // ... other fields for CRUD, vector, migration ...
    paginated_queries: DashMap<String, Arc<tokio::sync::Mutex<PaginatedQueryState>>>,
}

struct PaginatedQueryState {
    db_path: String,
    collection: String,
    filter: Option<HashMap<String, FieldFilter>>,
    sort: Option<Vec<SortSpec>>,
    page_size: usize,
    total_count: u64,
    current_page: usize,
    cursor_id: Option<String>,
    has_more: bool,
    created_at: Instant,
}
```

DashMap key is the UUID string (canonical form). The HandleRef carries the same UUID; `to_string()` at the lookup boundary bridges the two representations.

**Lifetime**: producer-owned. Cursors live until `data/query-close` removes them or (future) a TTL eviction sweep fires. No global handle registry — each cursor's lifetime belongs to this module's state map.

## Events emitted

**None.** The cursor surface is request/response only.

## Concurrency contract

### The bug that drove the design

Original implementation (pre-PR #1490):

```rust
let snapshot = self.paginated_queries.get(&cursor_id).map(|s| (s.current_page, ...));
// ^ DashMap shard lock released HERE
// ... async adapter.query() runs with NO lock ...
self.paginated_queries.get_mut(&cursor_id).map(|mut s| s.current_page += 1);
```

Under N concurrent `query-next` calls on the SAME cursor (canonical multi-persona scenario, or one persona retrying), every call read `current_page=0`, queried the same first page, wrote `current_page=1`. 8 concurrent callers got `pageNumber=1` back; cursor advanced by 1.

Caught by `same_cursor_concurrent_next_does_not_corrupt_state` (PR #1490) — the test panicked with *"page 1 served 8 times — the cursor advanced through it MORE than once, indicating a lost serialization"*.

### The fix: per-cursor `tokio::sync::Mutex`

```rust
let state_lock = self.paginated_queries.get(&cursor_id)
    .map(|entry| entry.value().clone())   // cheap Arc clone out of shard lock
    .ok_or("handle not found ...")?;
let mut state = state_lock.lock().await;  // serialize SAME-cursor concurrent calls
// ... read state, run adapter query, update state — all under the lock ...
```

- **Different cursors stay fully parallel** — DashMap's per-shard locking; each cursor has its own Mutex
- **Same cursor serializes** — each non-tail page served at most once; cursor advances atomically

### Pinned invariants

1. **`cursors_are_isolated_under_concurrent_open_and_next`** — 20 personas open distinct cursors concurrently; every cursor mints a distinct UUID; each cursor's first page returns its own pageSize items
2. **`same_cursor_concurrent_next_does_not_corrupt_state`** — 8 concurrent next-calls on the SAME cursor; each non-tail page served EXACTLY once (regression net for the read-then-async-write race)
3. **`query_open_returns_handle_alongside_legacy_query_id`** — additive migration: legacy queryId AND typed handle in same response
4. **`query_next_rejects_handle_with_wrong_owner`** — cross-module handle confusion fails loud
5. **`query_next_rejects_handle_with_wrong_type_tag`** — within-module cross-resource confusion fails loud
6. **`query_next_with_unknown_handle_returns_handle_not_found`** — stale handle typed error with cause hints
7. **`full_round_trip_open_next_close_via_handles_only`** — end-to-end through the new canonical shape, 12 rows / 3 pages

All multi-thread tests use `flavor = "multi_thread", worker_threads = 4`.

### `query-close` race

`DashMap.remove()` is atomic. If a concurrent `query-next` holds the `Arc<Mutex>` mid-flight when `query-close` fires, the Arc keeps the Mutex alive; the next's mutation succeeds against an orphaned state map (never read again). From the caller's view: close said success; in-flight next returns its now-meaningless page; cursor unreachable for subsequent calls. Benign — callers shouldn't race close with next.

## Migration notes

**Migrated in PR #1490** from a hand-rolled string-id pattern to typed HandleRef. The migration was **additive** — the legacy `queryId` field stays in responses and inputs so existing TS consumers see no break. A follow-up drops `queryId` once every consumer threads the handle.

### Rethink-vs-port outcomes

| TS shape | Rust rethink | Why |
|---|---|---|
| `queryId: string` returned at top level | `queryId` nested in `data.{...}` PLUS top-level `handle: HandleRef` | Additive — legacy callers still parse `response.data.queryId`; new callers thread the typed handle |
| `{queryId: "..."}` flat in next/close inputs | `CommandRequest` envelope with `handle: HandleRef` OR legacy `queryId` field | Same — dual-shape during migration window |
| Generic "Query X not found" error | "handle not found — cursor X is unknown ... may have been closed via data/query-close, evicted by future TTL ..." | Callers self-diagnose without grepping source |
| No owner/type validation | `HandleRef::expect_owned_by` validates owner first (routing) then type_tag (within-module discriminator); both errors name offender + expected | Cross-module handle confusion impossible to detect with bare strings; typed HandleRef makes it impossible to miss |
| Empty params crashed with "missing field" | Both `handle` and `queryId` optional; resolver fails loud naming BOTH supported shapes | Empty case is now reachable; user-friendly diagnostic instead of serde panic |

## Kinks found

**Two real bugs, both caught by the multi-thread concurrency tests before merge:**

1. **Read-then-async-then-write race** (the page-1-served-8-times bug). Fix: per-cursor `tokio::sync::Mutex`. Doctrine: every ServiceModule holding per-resource mutable state across `.await` MUST use per-resource locks (field manual §4.1).

2. **Bare-string handles silenced cross-module routing bugs.** A handle minted by module X reaching module Y's handler would silently miss in Y's state map. Fix: typed `HandleRef::expect_owned_by` validates owner+type_tag, fails loud with diagnostic naming offender+expected. Substrate refinement landed in PR #1491.

**Substrate refinements distilled from this consumer** (PR #1491):

- `HandleRef::expect_owned_by(owner, type_tag) → Result<Uuid, String>` — canonical validation
- `CommandRequest::handle_id_or_legacy(...)` — dual-shape resolver for any migration

Both replaced ~35 lines of inline boilerplate per future migration with one method call each. The data cursor migration was the proving ground — refinements that came out of it benefit every future consumer.

## References

- PR #1490 — HandleRef migration + per-cursor mutex fix + concurrency tests
- PR #1491 — `expect_owned_by` + `handle_id_or_legacy` distilled from the cursor consumer
- PR #1485 — Cell shapes (HandleRef definition)
- PR #1486 — `CommandRequest<P>` / `CommandResponse<T>` envelopes
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md §2.3, §2.4, §2.5](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — HandleRef, expect_owned_by, handle_id_or_legacy
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md §4.1](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — per-resource locks
- [ORM-PHASE-2-DESIGN.md](ORM-PHASE-2-DESIGN.md) — broader ORM context the cursor surface lives in
