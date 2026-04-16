# ORM Idealism Plan — Diagnosis + Phased Fixes (postgres stays)

**Status:** plan, no code yet. Surfaced 2026-04-16 during PR891 work. Joel's directive: "survey the field, figure out WHY it sucks, introduce our idealism into the design." Postgres stays as the production backend; the work below is architectural fixes that help the abstraction regardless of backend.

This document supersedes earlier doc `ORM-REDESIGN.md` (reverted) which was generic prescription. This one is built from reading the actual code.

## Survey findings

The TS-side entity model is **already well-designed**. Decorators are real and rich:

- `ChatMessageEntity` (`src/system/data/entities/ChatMessageEntity.ts:182`) declares: SSoT collection name, field types via `@TextField`/`@JsonField`/`@DateField`/`@EnumField`, `@CompositeIndex(['roomId','timestamp'], DESC)`, `@Archive(sourceHandle='primary', destHandle='archive', maxRows=10000, rowsPerArchive=1000, orderByField='timestamp')`, per-entity pagination config.
- `EntityRegistry` (`src/daemons/data-daemon/server/EntityRegistry.ts`) registers ~50 entities with their classes, dynamically populated as entities import.

This isn't broken. The decorator system IS the SSoT for schema + intent + archive policy.

What IS broken — three concrete violations and five concrete bottlenecks — all from grep-verified findings, not pattern-matching:

### Violations

0. **SQL leaking outside the adapter boundary into a TS command.**
   `src/commands/data/schema/server/DataSchemaServerCommand.ts:280-311` generates raw `CREATE TABLE IF NOT EXISTS ...` and `ALTER TABLE ADD FOREIGN KEY ...` strings in TypeScript. SQL has escaped the adapter — this is the portability-killer. Couldn't swap to MongoDB / DynamoDB / S3 because that command speaks SQL directly to whatever's underneath. Schema generation must live INSIDE the adapter (or behind a typed `apply_schema(EntitySchema)` method on the adapter trait); the command should call the typed method, not build SQL.

   Test: `grep -rn "SELECT \|INSERT INTO\|UPDATE \|DELETE FROM\|CREATE TABLE\|ALTER TABLE" src/ --include="*.ts"` should return zero non-comment hits after fix. Same grep on `src/workers/continuum-core/src` outside `orm/` should also be zero.


1. **TS decorator metadata stops at the IPC boundary.**
   `grep -rn "Archive\|CompositeIndex\|orderByField\|sourceHandle" src/workers/continuum-core/src` → **zero matches**. The Rust ORM has no awareness that any entity has an archive policy, a composite index, or an indexed field. Decorators are documentation in TS land, ignored downstream. Indexes get created (or not) at runtime via the generic `ensure_schema` path which uses field-level `indexed: bool` only — no composite, no archive, no sort hints.

2. **`connection_string` is plumbed throughout Rust ORM internals.**
   10+ occurrences in `orm/{adapter,postgres,sqlite,migration,connection_manager}.rs`. Every adapter takes a `connection_string` in its config. Memento's recent sentinel fix (commit `d07c8bd2d`) sealed the IPC frame so TS sends `@main` instead of a URL — but inside Rust, the abstraction is still "every backend speaks connection_string." A future S3 / DynamoDB / json-file adapter would have to wrestle with "what's a connection string for me?" That's URL-typing, not abstraction.

3. **TS-side ORM resolves handle→db_path early then uses db_path internally.**
   `src/daemons/data-daemon/server/ORM.ts:110-138`: `const dbPath = registry.getDbPath(handle)`, then callers pass `dbPath` everywhere. The handle abstraction collapses inside TS even though the IPC frame to Rust is now sentinel-clean.

### Bottlenecks

1. **Per-request JSON parse on every data op.**
   Every `handle_create`/`handle_read`/`handle_query`/`handle_update`/`handle_delete` in `modules/data.rs` does `serde_json::from_value(params.clone())`. At 5 personas × ~100 reads/sec = 500 JSON parses/sec just to figure out which collection. Joel has flagged this previously; it keeps coming back because nothing in the architecture stops new handlers from adding more.

2. **Zero prepared statements anywhere.**
   `grep -rn "prepare\|prepared_statement" src/workers/continuum-core/src/orm/postgres.rs` → zero. Every postgres query string-builds → server parses → executes → resets. Hot paths (recent chat in room, persona inbox, memory recall) re-parse identical SQL on every call.

3. **No read-pool / write-pool split.**
   Single pool of 20 connections for "main" (`modules/data.rs:185`). Reads queue behind writes under concurrent persona load. This is the contention point under multi-persona workloads.

4. **`ensure_schema` may run on the write path.**
   Called from data write paths (postgres.rs:1401-1423 per memento). Need to confirm it short-circuits when the table exists; if not, every write pays a system-table check.

5. **Zero cache layer in `DataModule`.**
   No `cache` references in `modules/data.rs`. Every read hits the DB even for hot, rarely-changing lookups (room metadata, user info, persona config). That's the path that benefits most from a cache because invalidation is rare.

### The pattern that explains all of it

TS has rich entity decorators (the SSoT for schema + intent). The Rust ORM is a generic doc store with `create/read/update/delete/query` against a `connection_string`. The two halves don't connect — decorator intent never propagates to the backend that could use it. That's why the abstraction looks idealistic on paper but produces dump-everything behavior in practice: anything goes, and nothing is enforced or optimized end-to-end.

## Hot loop discipline (the lens for everything below)

**Mental model from Joel:** the data layer is a render loop. Every per-call op (insert, update, select) is the inner pixel of the loop. A single `if`, a single string-build, a single round-trip you didn't need — those are wreckers when they multiply by every persona × every action × every event.

**Verified hot-loop violations in current code (grep-confirmed against `src/workers/continuum-core/src/orm/postgres.rs`):**

- **`ensure_table_exists_pg` runs on EVERY write.** Three callsites (lines 684, 824, 1107). Each one string-builds a `CREATE TABLE IF NOT EXISTS` and executes against postgres, walking the incoming `Value` to derive column types — even when the table has existed since process start. Should run **once per (collection, runtime)** at adapter init, then never again unless schema-evolves.
- **`SELECT * FROM ... WHERE id = $1`** (line 785). The amateur move — read-by-id pulls every column including arbitrarily-large JSONB blobs (a 50KB message body, a 4KB embedding vector, an entire image manifest) when the caller asked for one row. The entity decorators ALREADY declare every field; the read-by-id path should project the declared fields only. SELECT * for no reason at all is one of the costs Joel called out specifically. **The new architecture makes SELECT * impossible to write — adapter trait method `read_by_id(entity, id)` builds the projection from entity metadata, no string-builder caller can sneak `*` into it.**
- **`SELECT COUNT(*) FROM ...`** (line 1018). Scans every row in the collection for every paginated query. For large tables this is a multi-second cost. Should use `pg_class.reltuples` for estimated counts where exact isn't needed, or "fetch limit+1, return has_more flag" for "is there more" checks.
- **JSON re-parse per call** (`serde_json::from_value(params.clone())` everywhere in `modules/data.rs` — **grep count: 26 callsites**). The most egregious of all the violations because it does work for no reason at all on every single op. **Includes the cursor handlers** (`query-open`, `query-next`, `query-close`) — so paginating page-by-page through 200 results parses JSON 200 times. The cursor system optimized the SQL side and left the parse cost untouched, which means cursor pagination is no faster than full-scan in the parse-cost dimension. See Phase 1.
- **No prepared statements** — see Phase 3.

**Discipline applied to all phases below:** every phase asks "what work just moved OUT of the per-call hot path?" Schema check moves from per-write to once-per-runtime. SELECT * moves from always-pull-all to declared-projection. JSON parse moves from per-call to typed-once-at-IPC. Statement parse+plan moves from per-query to prepared-once-cached.

When the work is moved out, the inner loop shrinks. When the inner loop shrinks, the system can carry more concurrent personas without thrashing. That's the win — not "we added postgres" or "we added a cache" abstractly, but specifically "we removed N work-items from each of M ops/sec."

**The architecture-level commitment:** efficient-by-default means the path-of-least-resistance produces fast code, and the slow patterns (SELECT *, per-call JSON parse, per-write CREATE TABLE check) are not expressible in the new API surface. Future agents extending the system can't accidentally regress to the slow shape because the trait methods + typed structs + entity decorators don't leave room for it. Today's ORM accepts `Value` blobs and a free-form db_path and lets each handler reinvent how it does work — that's why the same mistakes keep returning. The fix is to remove the surface area where they could be made.

## Quick wins (land BEFORE the structural phases)

Three of the violations above can be fixed in a day each, no architectural changes required, with measurable perf wins. Worth landing as standalone PRs while phases 1-4 are designed/discussed.

### Quick win #1 — `ensure_table_exists_pg` short-circuit cache (~20 lines, ~1 hr)

Add a `DashMap<String, ()>` (collection name → "we ensured this runtime") to `PostgresAdapter`. Each callsite checks the map first; if present, skip the entire `CREATE TABLE IF NOT EXISTS` round-trip + Value walk. Cache invalidates on schema-evolve path (already exists). **Win: removes one full postgres round-trip + JSON walk per write.**

Same fix applies to `SqliteAdapter`. Same shape.

### Quick win #2 — replace `SELECT COUNT(*)` with `LIMIT N+1` for "has_more" checks (~half day)

Most pagination callers don't need exact count — they need "is there a next page?" Switch from `SELECT COUNT(*)` (full table scan) to `SELECT ... LIMIT $1+1` and the handler returns `has_more = rows.len() > $1`. Where exact counts ARE needed (admin views, analytics), keep COUNT(*) but tag those callsites so they're explicit.

For postgres, also consider `pg_class.reltuples::bigint AS estimated_count` for fast approximate counts on hot pagination paths. **Win: pagination on a 1M-row table goes from full-scan to index-seek + 1-row overshoot.**

### Quick win #4 — move TS schema-SQL generator INTO the adapter (~half day)

`DataSchemaServerCommand.ts` builds `CREATE TABLE` and `ALTER TABLE` strings in TS. Move the schema generation logic to the Rust adapter behind a typed `apply_schema(EntitySchema)` method. The TS command becomes a one-line call into Rust over IPC. Now SQL is fully sealed inside the adapter; future agents physically can't leak it back out.

Branch: `fix/orm-tsschema-sql-leak`

### Quick win #3 — typed param structs for the 5 most-called handlers (~1 day)

Don't wait for full Phase 1. Just convert the top 5 hot handlers (create, read, update, delete, query) from `params: Value` to concrete structs. Replace `serde_json::from_value(params.clone())` at the IPC dispatch with a single typed deserialization. Other 21 handlers can wait for Phase 1 proper. **Win: removes ~80% of the per-call JSON parse cost since those 5 handlers carry the hot traffic.**

These three quick wins land independently, on their own short-lived branches:
- `fix/orm-ensure-schema-cache`
- `fix/orm-pagination-no-count`
- `fix/orm-typed-hot-handlers`

Each ships with its own benchmark commit showing the win. They make the system measurably faster while phases 1-4 are still in design discussion.

## Idealism applied: 4 phases, speed-impact ordered

Each phase is backend-agnostic, independently shippable, and benchmarkable on its own.

### Phase 1 — Kill the IPC re-parse loop

**Win:** removes 500+ JSON parses/sec at hot load. Helps every backend.

- Use `ts-rs` (already in `Cargo.toml`) to round-trip Rust IPC param structs to TS so the wire schema is shared.
- Replace `serde_json::from_value(params.clone())` per handler with typed deserialization at IPC ingress. The handler receives `CreateParams`, not `Value`.
- Optional follow-on: switch the wire from JSON to `postcard` or `bincode` for zero-parse-cost. Keep JSON for first cut; binary upgrade later if benchmarks show it matters.

**Self-enforcing property:** new handlers added in the future cannot accept arbitrary `Value` because the IPC dispatch is typed. The bad pattern is no longer expressible.

**Acceptance:** benchmarked latency drop on `data/list` cold call ≥ 30% on M5; no `serde_json::from_value(params.clone())` calls remain in `modules/data.rs`.

### Phase 2 — Decorator metadata reaches Rust (the load-bearing change)

**Win:** every entity's archive policy, composite indexes, indexed fields, default sort, pagination — all enforced at the storage layer, not advisory in TS.

- Build-time generator (or `ts-rs`-driven extension) emits a Rust `EntityRegistry` from TS decorators. Same SSoT, mirrored.
- Rust `DataModule` consumes the registry at init: declares all indexes + composite indexes via `ensure_schema`, registers the archive job per entity, knows the default sort order.
- `StorageAdapter` trait gains lightweight methods to apply the metadata: `apply_indexes`, `register_archive_policy`, `prepared_statements_for_hot_paths`.

**Self-enforcing property:** an entity declared with `@Archive(...)` automatically gets the archive job — no separate caller has to wire it up. Adding a new entity requires ONE decorator change, not five.

**Acceptance:** `@Archive` and `@CompositeIndex` decorators on a new test entity automatically create the index + archive scheduler with no caller-side changes; "add S3 adapter" becomes a 200-line task because the entity model is now portable across backends.

### Phase 3 — Prepared statement cache

**Win:** per-call DB latency drops by the parse-plan portion. ~5-15% on postgres for short queries; bigger relative gain on SQLite where statement parse is hotter.

- Adapter init registers prepared statements for each entity's known hot paths (declared in Phase 2's metadata: recent_by_index, full_scan_paginated, by_id).
- Per-call dispatch uses cached prepared statement instead of building SQL.
- Custom queries via `data/query` still go through the build path (uncommon by definition).

**Acceptance:** p99 latency for `chat_messages.recent_in_room(roomId, 50)` drops measurably vs current build-and-execute path.

**Critical constraint (events stay correct):** writes must STILL emit `data:<collection>:created|updated|deleted` events at the same point in the pipeline. Personas (cognition module) wake on those events; UI websocket subscribes to them. A prepared statement that bypasses the event emission silently breaks AI response + UI updates. **Phase 3 work MUST run integration tests that assert events fire for writes routed through prepared statements.** Reads don't emit events, so the read-path cache is safe in this respect — but writes go through any path-altering optimization at the cost of events firing on every committed transaction.

### Phase 4 — Per-entity in-memory cache with declared invalidation

**Win:** 0-latency reads for the bulk of "what room is this", "who is this user", "what's persona X's config" lookups that today round-trip the DB.

- `DataModule` grows a cache keyed by `(entity, query-signature)`.
- Invalidation rules declared per entity (in the decorator extension from Phase 2): write path knows what to evict.
- Cache scopes: per-room, per-user, per-persona — declared per entity, not generic LRU.

**Self-enforcing property:** caching policy is part of the entity declaration. New entities declare their cache shape at the same place they declare their schema. Future agents can't add "untracked" cache because there's no `cache` API to call ad-hoc — caching is invisible to callers.

**Acceptance:** read-heavy hot lookups (rooms, users, persona configs) hit cache on second call; cache hit rate ≥ 90% for those entities under steady-state load; memory bound declared per-cache, not unlimited.

**Critical constraint (events stay correct):** the cache layer is READ-side only. Writes flow through the unchanged write path so `data:<collection>:created|updated|deleted` events fire at the existing emission point, then invalidate the relevant cache slice as a side-effect of the event firing — not as a replacement for it. Order matters: emit event first, invalidate cache second. Personas + UI must observe writes via the event bus the same way they do today; the cache must be invisible to them. **Integration test: chat_messages.append from one client must produce a `data:chat_messages:created` event that wakes personas AND pushes to the UI websocket, with the cached read for the same room reflecting the new message on next call.**

### Phase 5 — SQLite-with-our-concurrency assessment (OPTIONAL, evaluated after Phase 4)

**Decided after Phase 4 lands:** with the architecture fixes in, run the 5-persona concurrent workload against postgres and against SQLite-in-WAL-mode + our Rust-side write coordinator + per-persona sharding. Three outcomes:

- **SQLite ≥ 80% of postgres throughput on our workload** → propose deprecating the postgres adapter for Mac Carl install. Drops a Docker container, drops a port exposure, drops the postgres-specific install dance. Postgres adapter stays in-tree as an opt-in for grid/multi-host deployments. **Separate PR**, not part of phases 1-4.
- **SQLite is materially worse** → keep postgres as default, document why, move on. The architecture work in 1-4 wasn't wasted because it helps both backends.
- **Roughly tied** → keep postgres because changing the default is more risk than win at parity.

**This phase is explicitly conditional on Phase 4 first.** Joel's call: don't change the backend until the architecture above it can prove the abstraction is real.

## What's NOT in this plan (and why)

- **Generic "redesign the ORM."** The ORM concept is sound. The specific gaps above are what's broken; fixing them is the work, not a green-field rewrite.
- **Schema migration tooling.** Out of scope; existing `ensure_schema` path is fine once it gets the right metadata in Phase 2.
- **Phase 5 backend swap pre-decided.** It's evaluated AFTER Phase 4 with real numbers — not assumed up-front in either direction.

## Architectural test (the merge gate, conceptually)

After all four phases land: **adding an S3 adapter (or DynamoDB, or json-file, or anything else) should be ~200 lines of `impl StorageAdapter` and zero changes to entities or callers.** That's the test of whether the abstraction is real. Today it would require reinventing every entity's index/archive/sort policy from scratch in the new adapter — that's the un-portability symptom Joel called out.

## Coordination

- Each phase = its own branch named for what it does (`feat/orm-typed-ipc`, `feat/orm-decorator-rust-bridge`, `feat/orm-prepared-statements`, `feat/orm-entity-cache`).
- Each phase's PR includes a benchmark vs main showing the win.
- This doc lives at `docs/architecture/ORM-IDEALISM-PLAN.md` as the reference for whoever picks up each phase.
- Discussion before code: this is a proposal, open to redirect from Joel + memento before any phase starts.

## Related

- PR891 — current Mac Option B work that exposed the IPC URL leak symptom.
- Memento commit `d07c8bd2d` — TS sentinel handle, Phase 1's seed.
- Joel directives: ORM is an abstraction layer (no leaks across IPC); maintain good architecture; rust-truth + thin SDK per language; FAST + portable, not either-or.
