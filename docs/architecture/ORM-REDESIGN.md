# ORM Redesign — Typed Entities, Routed Storage, Sealed Boundary

**Status:** design proposal. No code yet. Surfaced 2026-04-16 during PR891 work when the ORM's leaky abstraction (TS sending postgres URLs across IPC) bit the Mac Option B architecture. Joel's diagnosis: "you just throw it into the bucket and forget. None of you guys have really thought about performant data ORM given the design demands."

This document is the proposal for what a real data layer would look like. It is **not** the immediate fix for the IPC URL leak (memento has the sentinel-based shim landing now). It is the design pass that should follow once we ship.

## Problem statement

The current `data/*` IPC surface accepts arbitrary JSON blobs against named collections. Callers pick the backend (postgres/sqlite) by passing a connection string. There is no per-collection routing policy, no concurrency intent annotation, no schema enforcement, and no per-entity cache control. Net effect:

- **Dump pattern**: every new feature `data/store`s its blob into `main` because that's the path of least resistance. `main` becomes a single hot table everything contends on.
- **No optimization headroom**: hot paths (recent chat by room, persona inbox poll, vector recall) re-parse JSON filters per call, hit the same connection pool as cold writes, share a single LRU.
- **Boundary leaks**: TS-side `process.env.DATABASE_URL` flows into IPC payloads as connection strings. The "ORM" isn't actually an abstraction — outside callers know which backend they're hitting and how to dial it.
- **Concurrency contention**: 5+ personas writing concurrently to the same hot tables in `main` produce lock contention that the pool sizing can't absorb. Per-persona state isn't sharded.
- **No archival policy**: `chat_messages` grows unbounded in `main`; no automatic move to `longterm.db` once cold.

These are the symptoms of an API designed for "make it work for the next feature" rather than "design for the workload."

## Workload characteristics (the demand we're designing against)

- **Multi-persona hot writes**: 5+ personas concurrently writing inbox messages, memory entries, state updates. Sub-second response required so personas don't block each other.
- **Live chat reads**: sub-second filtered queries on `chat_messages` (by room, by recency, limit 50) even with 1M+ rows.
- **Vector recall**: hundreds-per-second similarity queries against the embedding store.
- **Per-persona state**: 100+ qps read/write for individual persona configs and inbox state, naturally locality-friendly per persona.
- **Background analytics**: training corpus reads, RAG context building, archival queries — large but cold, must not contend with hot paths.

## Proposed architecture

### 1. Typed entity API (no more arbitrary JSON)

Replace `data/store(collection, blob)` with typed entity operations. Every entity type registers a schema; the API only accepts known shapes.

```ts
// Today (anything goes):
await execute('data/store', { collection: 'chat_messages', data: { ...blob } });

// Proposed (typed, schema-validated):
await ChatMessages.append(message);          // entity-scoped, schema-checked
await Personas.update(id, partial);          // partial-update aware
await Memories.recall(query, k);             // operation-typed, query-shape-typed
```

Rust side mirrors via generated `EntityRegistry` — entities declared once in a `entities.toml` (or via `ts-rs` round-trip), generators emit typed methods on both sides. Outside callers cannot dump arbitrary JSON because there is no API surface for it.

### 2. Per-entity routing policy

Each entity declares its storage policy at registration time, not per-call:

```toml
[entities.chat_messages]
backend = "main"               # postgres MVCC for hot writes
ttl_days = 7                   # automatic move to longterm after N days
shard_by = "room_id"           # partition hot reads by room
indexes = ["room_id+ts DESC", "user_id"]
cache = { policy = "tail", capacity = 50, scope = "per_room" }

[entities.persona_state]
backend = "per_persona"        # each persona gets its own sqlite
shard_by = "persona_id"
write_pattern = "frequent_partial"   # supports partial-update optimization
cache = { policy = "write_through", scope = "per_entity" }

[entities.memories]
backend = "main"
indexes = ["embedding HNSW(384)", "ts DESC"]
write_pattern = "append_only"
ttl_days = "infinite"
cache = { policy = "lru", capacity = 10_000 }

[entities.training_corpus]
backend = "longterm"           # sqlite, append-only archive
write_pattern = "batch"
cache = { policy = "none" }
```

The ORM owns: pool sizing per backend, isolation level per write_pattern, cache policy per entity. Caller intent ("I want to append a chat message") fully determines all three.

### 3. Sealed IPC boundary

TS callers and Rust callers both speak the same typed entity API. Neither sends connection strings. The API speaks **logical handles** (`ChatMessages`, `Personas`, etc.) — the backend resolution happens entirely inside the ORM, behind the IPC.

This is what memento's sentinel fix is the first step toward. The full version: TS doesn't even pass `db_path` — it calls `ChatMessages.append(msg)` and the IPC frame is `{ entity: "chat_messages", op: "append", data: ... }`. Rust side dispatches to its own routing config.

### 4. Hot/warm/cold tiering with automatic archival

Entities declare a TTL. A background job moves rows older than TTL from `main` to `longterm`:

- `chat_messages` older than 7 days → `longterm.chat_messages_archive`
- `inbox_messages` older than 24 hours and processed → archive
- `vector_embeddings` for sessions ended >30 days ago → archive

Reads of "recent" data hit `main` only (small, hot, fast). Reads of historical data union with `longterm` only when caller explicitly asks (`{ include_archive: true }`).

### 5. Per-persona DB partitions

Persona state, persona memories, persona inbox — each persona gets its own sqlite file (`per_persona/{id}.db`). Reasons:

- **Locality**: a persona's hot reads/writes land in one file, no cross-persona contention.
- **Migration**: spinning up/down a persona means creating/archiving one file, not touching shared tables.
- **Concurrency**: 5 personas running concurrently = 5 independent DBs, zero lock contention between them.

The shared `main` postgres holds only cross-persona entities (`chat_messages` shared rooms, `users`, `rooms`, `vector_embeddings` for system-wide RAG).

### 6. Pre-compiled query plans for hot paths

The ORM pre-registers queries for the known hot paths:

```rust
ChatMessages::recent_in_room(room_id, limit)   // pre-prepared statement, no JSON parse
PersonaInbox::next_unprocessed(persona_id)
Memories::recall(query_embedding, k)
```

Custom queries still possible via `data/query` but pay the JSON-parse + plan-generation cost. Hot paths bypass that cost entirely.

### 7. Read pool / write pool split

Postgres supports replicas. Pool config per backend includes `read_endpoint` and `write_endpoint`. The ORM routes read-only queries to read replicas under load. For local dev (no replica), both point at the same primary.

### 8. Cache layer with explicit invalidation by entity

Per-entity caches with declared invalidation rules:

- `ChatMessages.append(msg)` → invalidates `recent_in_room(msg.room_id)` cache
- `Personas.update(id, ...)` → invalidates `Personas.get(id)` cache
- `Memories.append(m)` → cache stays (immutable by design)

No generic LRU that guesses what to evict. Cache misses are bugs, not normal operation.

## Migration path

This is not a one-shot rewrite. Path:

1. **Phase 1 — Entity registry & sentinel handles** (memento's current PR891 work is the seed). TS sends sentinels (`@main`, `@longterm`, `@persona:{id}`), Rust resolves them. Existing `data/*` API stays for now.
2. **Phase 2 — Entity declarations**. Add `entities.toml`, generate typed wrappers, but keep generic `data/*` working. Migrate hot-path callers (chat, persona, memory) to typed wrappers first.
3. **Phase 3 — Routing policies**. Per-entity backend / pool / cache config. Generic `data/*` still routes everything to `main` (back-compat). Typed wrappers honor the policy.
4. **Phase 4 — Per-persona partitions**. Persona-scoped entities migrate to per-persona sqlite files. Cross-persona stays in `main`.
5. **Phase 5 — Hot path pre-compilation**. Pre-prepared statements for top-N hot queries. Benchmarks gate the merge.
6. **Phase 6 — Archival job**. Background TTL-based mover from `main` to `longterm`.
7. **Phase 7 — Deprecate generic `data/*`**. Once all callers migrated to typed entities, the dump-anything path goes away.

Each phase is its own branch, its own PR, its own benchmark validation. Do not pile them on one branch.

## Acceptance benchmarks

The redesign has to demonstrably beat the current data layer on:

- **5-persona concurrent inbox poll**: target <50ms p99 per persona, current is ~200ms+ under contention.
- **Recent chat history (1M-row table)**: target <30ms for "last 50 in room X", current is ~500ms+ when `main` is hot.
- **Vector recall (10k-vector index)**: target <100ms p99, current depends on what else is happening to `main`.
- **Background archival job**: must move 100k chat_messages from `main` to `longterm` in <60s without measurably degrading hot-path latencies.

## Owner / coordination

This document is a proposal, not an assignment. Worth discussing among me, memento, and whoever else touches the data layer (vhsm-claude has done relay/IPC work that overlaps). Should land as an issue with this doc as the design reference, then a single owner picks it up phase by phase.

Cross-cutting enough that it should not block PR891. PR891 ships with memento's sentinel fix as the first half-step of Phase 1. The rest is a separate roadmap.

## Related

- PR891 — current Mac Option B work that exposed the URL-leak symptom
- `memory/` entries on continuum-core monolith fission — the ORM redesign is partially the data-layer face of that overall split
- Joel directives this builds on: ORM is an abstraction layer (no leaks across IPC); maintain good architecture; don't violate calls into the ORM over IPC
