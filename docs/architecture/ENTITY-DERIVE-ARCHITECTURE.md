# Entity Derive Architecture — The Rust-First, Generated-Everywhere ORM

**Status:** Foundation landed (PRs `758fa1cf8`, `7d5034b9d`). Engram migrated. AdmissionState wire-up + RecallMetadata derive in progress (`#168`).
**Doctrine:** [[orm-everything-not-hand-edited-files]], [[no-sql-everything-through-orm-entities]], [[organization-purity-as-we-migrate]] (E=mc² compression).

## The thesis

**Entities are defined exactly once, in a Rust struct. Everything else is generated.**

Per Joel 2026-06-03: "Entities need to be defined, and in one place, rust (we are headless), then generated for all places, easy to do this elegantly like we did with decorators in ts, but for rust."

The Rust struct is the single source of truth for what an entity IS. The ORM schema, the TS wire type, the JSON export/import shape, the persistence layer, and the future-grid-replicated copy all fall out of that one struct. **Drift between the struct and any consumer is a structural impossibility, not a discipline problem.**

This supersedes the TS-decorators-as-canonical approach documented in [ORM-PHASE-2-DESIGN.md](ORM-PHASE-2-DESIGN.md) — the substrate is headless and Rust-native, so the canonical authoring path is Rust-first. TS bindings are generated downstream by ts-rs.

## The three load-bearing layers

```
┌───────────────────────────────────────────────────────────────────┐
│  Rust struct + attribute annotations  (the SINGLE source of truth) │
│  use #[derive(Debug, Clone, Serialize, Deserialize, TS, Entity)]   │
│  #[entity(collection = "engrams")]                                 │
│  pub struct Engram { ... }                                         │
└────────────┬───────────────────┬──────────────────┬───────────────┘
             │                   │                  │
             ▼                   ▼                  ▼
   ┌──────────────────┐  ┌────────────────┐  ┌──────────────────┐
   │ ts-rs generates  │  │ Entity derive  │  │ serde gives JSON │
   │ TS type bindings │  │ generates ORM  │  │ + future formats │
   │ (camelCase)      │  │ schema         │  │ (CBOR/YAML/etc.) │
   │                  │  │                │  │                  │
   │ shared/generated │  │ OrmStore<T>    │  │ Portability for  │
   │ /persona/        │  │ persists to    │  │ grid-distributed │
   │ Engram.ts        │  │ SQLite/Postgres│  │ persona migration│
   └──────────────────┘  └────────────────┘  └──────────────────┘
```

Adding a field to the Rust struct propagates everywhere automatically. Removing a field surfaces failures at compile time + at the schema-migration check. **The canonical commit unit is "edit the struct"**, not "edit the struct AND edit the schema AND edit the TS binding AND edit the persistence shim."

## The derive macro shape

`#[derive(Entity)]` lives in the `continuum-orm-derive` crate. It reads the struct + attribute annotations and emits `impl OrmEntity for #name`. The Rust analogue of TS class decorators — additive, composes with other derives, no surprise auto-inclusion.

### Struct-level attributes

- `#[entity(collection = "name")]` — REQUIRED. The collection name (table name in SQL backends).
- `#[entity(index(name = "...", fields = ["a", "b"], unique = true))]` — composite multi-field index. Repeat for multiple.

### Field-level attributes

- `#[entity(primary_key)]` — declares this field IS the BaseEntity id (a bare `Uuid`). Pulls in `base_entity_fields()` (id + createdAt + updatedAt + version) and skips emitting the field separately. Use when the entity carries its primary key as a top-level `Uuid` rather than embedding a `BaseEntity` struct.
- `#[entity(indexed)]` — single-field B-tree index.
- `#[entity(unique)]` — unique constraint.
- `#[entity(nullable)]` — explicit nullable (auto-applied for `Option<T>`).
- `#[entity(json)]` — force JSON column (override the inferred type). Useful for tagged-union enums and nested structs.
- `#[entity(skip)]` — exclude from schema (in-memory only). Pair with `#[serde(skip)]` to also keep out of the wire payload.
- `#[entity(foreign_key("collection.field", on_delete = "cascade", on_update = "restrict"))]` — declare a real FK reference. Cascade keywords: `"restrict" | "cascade" | "set_null" | "no_action"`. Defaults to `Restrict` on both.

### The two BaseEntity patterns

**Pattern A: bare `Uuid id` (Engram's pattern)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "engrams")]
pub struct Engram {
    #[entity(primary_key)]
    pub id: Uuid,
    // ... other fields
}
```

The id IS the BaseEntity.id semantically. Adapter manages createdAt / updatedAt / version transparently.

**Pattern B: embedded `BaseEntity` (TS-decorator analogue)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_messages")]
pub struct ChatMessage {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub author_id: Uuid,
    pub content: String,
}
```

The BaseEntity struct is embedded via `#[serde(flatten)]`. The derive recognizes it by type name and adds the base columns. Field access at the Rust level lets you read `message.base.created_at` directly. Use when the entity needs visible BaseEntity fields at the Rust call sites.

The two patterns are mutually exclusive per entity (the derive enforces this).

## Type inference (Rust type → FieldType)

| Rust type                       | FieldType  | Notes                                |
|---------------------------------|------------|--------------------------------------|
| `String` / `&str`               | `String`   |                                      |
| `Uuid` (by type-name match)     | `Uuid`     |                                      |
| `bool`                          | `Boolean`  |                                      |
| `u*` / `i*` / `f32` / `f64`     | `Number`   | SQLite uses NUMERIC affinity         |
| `Vec<_>` / `HashMap` / `BTreeMap` / `HashSet` | `Json` | JSON-serialized container |
| `Option<T>`                     | inner T's type + `nullable = true` |  unwrapped one level |
| Enum (variant-only)             | `Json`     | Tagged-union enums ride intact       |
| Any other named struct          | `Json`     | Override with `#[entity(...)]`       |

Override with `#[entity(json)]` to force any field to a JSON column.

## Relational schema — FKs are first-class

`SchemaField` carries:

```rust
pub struct SchemaField {
    pub name: String,
    pub field_type: FieldType,
    pub indexed: bool,
    pub unique: bool,
    pub nullable: bool,
    pub max_length: Option<usize>,
    pub foreign_key: Option<ForeignKeyRef>,
}

pub struct ForeignKeyRef {
    pub collection: String,
    pub field: String,
    pub on_delete: CascadeRule,
    pub on_update: CascadeRule,
}

pub enum CascadeRule { Restrict, Cascade, SetNull, NoAction }
```

Both `SqliteAdapter` and `PostgresAdapter` emit `FOREIGN KEY (...) REFERENCES ...(...) ON DELETE ... ON UPDATE ...` in `CREATE TABLE`. SQLite gets `PRAGMA foreign_keys=ON` so the constraints are enforced (SQLite parses but ignores FK by default).

**Real referential integrity at the DB layer.** Application code doesn't enforce "when this engram dies, its recall metadata dies too" — the DB does it. `[[no-fallbacks-ever]]` extended to relational invariants.

## The typed persistence rail: `OrmStore<T>`

`crate::orm::OrmStore<T: OrmEntity + Serialize + DeserializeOwned>` wraps an `Arc<dyn StorageAdapter>` and provides:

```rust
OrmStore::<T>::new(adapter).await?       // schema ensure, idempotent
store.save(id, &entity).await?
store.find_by_id(id).await? -> Option<T>
store.find_all().await? -> Vec<(Uuid, T)>
store.update(id, &entity).await?
store.delete(id).await? -> bool
```

**Compression principle in action.** ONE generic helper, every `T: OrmEntity` gets save/find/update/delete for free. Add `impl OrmEntity for FooEntity` (or just `#[derive(Entity)]`), use `OrmStore<FooEntity>` — works immediately. No per-entity store + migration + serializer trio.

## The portability payoff

Because the entity contract is `OrmEntity + Serialize + DeserializeOwned`:

- **JSON export/import** — every entity round-trips through `serde_json::to_value` / `from_value` for free.
- **TS bindings** — ts-rs generates the TS type from the same struct.
- **Schema-as-data** — `CollectionSchema` is itself `Serialize` + ts-rs-exported. Schemas can ship over the wire or land in a registry file.
- **Backend-agnostic** — SQLite today, Postgres tomorrow, future grid-replicated tier. All read the same `CollectionSchema`.
- **Format-agnostic** — swap `serde_json` for `serde_yaml` / `serde_cbor` / `postcard` to export entities in YAML / CBOR / binary with zero entity changes.

This is the substrate plumbing that makes **persona portability across continuums** structural rather than aspirational. A persona's engrams export as JSON, ship across the grid, import into another continuum's adapter — same shape on the receiving end because the entity definition is the same Rust struct everywhere. The grid-as-distributed-gene-pool from [[persona-breeding-substrate-supports-it]] falls out without redesign.

## Concrete adapters: what's migrated

- **`Engram`** — fully derive-driven (`#[derive(Entity)]` + per-field attrs). Round-trips through real SQLite via `OrmStore<Engram>`. Hand-written `impl OrmEntity` block deleted. Tests pass. PR `7d5034b9d`.

## Migration in progress

- **`RecallMetadata`** — needs `Serialize + Deserialize` derives added (currently a `Copy` hot-path struct in `DashMap`). Then `#[derive(Entity)]` with `#[entity(foreign_key("engrams.id", on_delete = "cascade"))]` on `engram_id`. **Real FK to engrams.** Cascading delete handled by the DB.
- **`AdmissionState`** — gains `Option<Arc<OrmStore<Engram>>>` + `Option<Arc<OrmStore<RecallMetadataRow>>>`. `admit()` writes through. `record_recall_hit` + `apply_decay` update. `load_at_boot()` rehydrates `Vec<Engram>` + `DashMap<Uuid, RecallMetadata>` from disk. **Engrams survive process restart for the first time in the substrate's history.**
- **`RoleTemplate`**, **`HwTierDescriptor`** — already declared their `OrmEntity` impls by hand (#123). Migration to `#[derive(Entity)]` is a sed-style mechanical change that deletes hundreds of lines.
- **Future entities** — every new entity authors as `#[derive(Entity)]` from day one. Per [[organization-purity-as-we-migrate]] no new hand-written `impl OrmEntity` blocks land in module code.

## Adapter dedup (a latent bug found and fixed)

Both `SqliteAdapter` and `PostgresAdapter` hardcoded the BaseEntity columns (`id / created_at / updated_at / version`) at the top of `CREATE TABLE`, then ALSO iterated `schema.fields`. When schemas declared BaseEntity columns via `base_entity_fields()` (the documented contract), CREATE TABLE crashed on duplicate column name.

Fix: `crate::orm::entity::is_base_entity_column()` helper. Both adapters skip schema.fields whose snake_case name matches. Single source of truth lives at the adapter level; schemas declare intent without forcing entities to know each backend's CREATE TABLE layout.

## SQLite numeric affinity (a second latent bug found and fixed)

SQLite stored `FieldType::Number` as REAL affinity, coercing integers to floats. The bug was latent because no OrmEntity had ever round-tripped a typed `i32`/`i64` until the derive's test entity used `delta: i32`. Changed to NUMERIC affinity — preserves integers as integers, floats as floats.

## Cross-cutting doctrines

- [[no-sql-everything-through-orm-entities]] — module code never touches raw SQL. The ORM owns the backend.
- [[no-fallbacks-ever]] — FK constraints enforce referential integrity at the DB, not at the application layer. Typed errors on every adapter call.
- [[observability-is-half-the-architecture]] — every entity is inspectable + exportable; the persistence layer is debuggable end-to-end.
- [[host-the-seemingly-impossible]] — the substrate hosts what would be expensive infrastructure on consumer hardware (SQLite locally, Postgres optionally, grid-replicated in the future).

## Files

- `src/workers/continuum-orm-derive/src/lib.rs` — the proc-macro crate. `#[derive(Entity)]` definition.
- `src/workers/continuum-core/src/orm/entity.rs` — `BaseEntity` + `OrmEntity` trait + registry + `is_base_entity_column` helper.
- `src/workers/continuum-core/src/orm/types.rs` — `CollectionSchema` + `SchemaField` + `SchemaIndex` + `ForeignKeyRef` + `CascadeRule`.
- `src/workers/continuum-core/src/orm/store.rs` — `OrmStore<T>` typed persistence rail.
- `src/workers/continuum-core/src/orm/sqlite.rs`, `postgres.rs` — adapters. Emit FK constraints + composite indexes + dedupe BaseEntity columns.
- `src/workers/continuum-core/src/orm/derive_test.rs` — end-to-end derive tests (composite index, FK cascade, round-trip).
- `src/workers/continuum-core/src/persona/engram.rs` — first production entity migrated to the derive.
