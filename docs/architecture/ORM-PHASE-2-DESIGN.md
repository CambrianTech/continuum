# ORM Phase 2 — Decorator Metadata to Rust

**Status:** Design, scoping-only. Implementation follows QW#3 (typed hot-handlers) so the two don't collide.
**Author:** memento (feature/inference-perf)
**Companion:** m5-test's ORM-IDEALISM-PLAN.md on feat/orm-architecture-design

## Goal

Make TS decorators the **one place** entity schema is authored, and make that authoring flow to Rust via a build-time artifact — not by shipping `CollectionSchema` over IPC at runtime.

After Phase 2:

- TS entities declare shape via decorators (unchanged)
- A build step walks `ENTITY_REGISTRY`, emits `src/shared/generated/entity_schemas.json`
- Rust `DataModule` loads this JSON at module init, keeps it in memory
- `data/ensure-schema` IPC carries only a **collection name** — no field list, no index list, no FK list
- Rust `ensure_schema(collection)` looks up the shape in its loaded map and builds backend-specific SQL inside the adapter

Enforcement test (merge gate): any net-new decorator option must flow through the codegen and land in Rust's loaded struct — no silent drops. If it flows into `FieldMetadata` in TS and not into the Rust `EntitySchema`, that's a regression.

## Current state (grep-verified)

TS has three global metadata registries in `src/system/data/decorators/FieldDecorators.ts`:

| Registry | Key | Value | Getter |
|---|---|---|---|
| `FIELD_METADATA` | `EntityConstructor` | `Map<fieldName, FieldMetadata>` | `getFieldMetadata` |
| `COMPOSITE_INDEXES` | `EntityConstructor` | `CompositeIndexMetadata[]` | `getCompositeIndexes` |
| `ARCHIVE_CONFIGS` | `EntityConstructor` | `ArchiveConfig` | `getArchiveConfig` |

`ENTITY_REGISTRY` in `src/daemons/data-daemon/server/EntityRegistry.ts` maps `collectionName -> EntityConstructor`.

None of these three registries are read by Rust today (m5's survey: zero matches for `Archive`, `CompositeIndex`, or indexed-field metadata in `src/workers/continuum-core/src`). They are TS-side documentation that the wire layer transforms into `CollectionSchema` per-ensure-schema call.

## Target artifact

`src/shared/generated/entity_schemas.json`:

```jsonc
{
  "$schemaVersion": 1,
  "$generatedAt": "2026-04-16T15:30:00.000Z",
  "$sha256": "abc123...",
  "entities": {
    "users": {
      "collection": "users",
      "entityClass": "UserEntity",
      "primaryKey": "id",
      "fields": [
        {
          "fieldName": "id",
          "fieldType": "primary",
          "nullable": false,
          "unique": true
        },
        {
          "fieldName": "createdAt",
          "fieldType": "date",
          "nullable": false,
          "index": true
        }
        // ...
      ],
      "compositeIndexes": [
        {
          "name": "idx_room_timestamp",
          "fields": ["roomId", "timestamp"],
          "direction": "DESC"
        }
      ],
      "archive": {
        "sourceHandle": "main",
        "destHandle": "archive",
        "maxRows": 10000,
        "rowsPerArchive": 1000,
        "maxArchiveFileRows": 100000,
        "orderByField": "timestamp"
      }
    }
    // ... one entry per registered collection
  }
}
```

`$sha256` covers a canonical serialization of `entities` (sorted keys). Drift-detection at core start: read the file, hash it, compare to the embedded `$sha256`. Mismatch → hard-fail with a clear message ("entity_schemas.json modified after generation; rebuild TS").

## Codegen script — `scripts/generate-entity-schemas.ts`

```typescript
// Pseudo-code
import { ENTITY_REGISTRY } from '@daemons/data-daemon/server/EntityRegistry';
import { getFieldMetadata, getCompositeIndexes, getArchiveConfig } from '@system/data/decorators/FieldDecorators';

async function main() {
  await import('@daemons/data-daemon/server/EntityRegistry'); // triggers registration side-effects

  const entities: Record<string, EntitySchemaJSON> = {};
  for (const [collection, EntityClass] of ENTITY_REGISTRY.entries()) {
    entities[collection] = {
      collection,
      entityClass: EntityClass.name,
      fields: serializeFields(getFieldMetadata(EntityClass)),
      compositeIndexes: getCompositeIndexes(EntityClass),
      archive: getArchiveConfig(EntityClass) ?? null,
    };
  }

  const canonical = JSON.stringify({ entities }, (_k, v) => sortIfObject(v));
  const sha256 = await sha256hex(canonical);
  const output = { $schemaVersion: 1, $generatedAt: new Date().toISOString(), $sha256: sha256, entities };

  await writeFile('src/shared/generated/entity_schemas.json', JSON.stringify(output, null, 2));
}
```

Wire into `build:ts` chain:

```
generate-version → generate-config → generate-entity-schemas → tsc
```

Placed **before** `tsc` so a TS type can be generated alongside (`EntitySchemaJSON` interface matching the JSON shape, used by Rust's ts-rs-exported consumer types).

## Rust loader — `modules/data.rs`

```rust
// New field on DataModule:
schemas: OnceCell<HashMap<String, EntitySchemaJSON>>,

// Lazy-on-first-use to avoid startup cost if nothing uses it.
async fn get_schemas(&self) -> Result<&HashMap<String, EntitySchemaJSON>, String> {
    self.schemas.get_or_try_init(|| async {
        let path = resolve_entity_schemas_path()?; // ~/.continuum/... or next to binary
        let contents = std::fs::read_to_string(&path)?;
        let parsed: EntitySchemasFile = serde_json::from_str(&contents)?;
        verify_sha256(&parsed)?; // drift check
        Ok(parsed.entities)
    }).await
}

// In ensure_schema handler:
async fn handle_ensure_schema(&self, params: Value) -> Result<CommandResult, String> {
    let params: EnsureSchemaParams = serde_json::from_value(params)?;
    let schemas = self.get_schemas().await?;
    let schema = schemas.get(&params.collection)
        .ok_or_else(|| format!("Unknown collection '{}' — not in entity_schemas.json", params.collection))?;
    let adapter = self.get_adapter(&params.handle).await?;
    adapter.ensure_schema(schema.clone()).await?;
    CommandResult::json(&json!({ "success": true }))
}
```

`EntitySchemaJSON` struct in Rust can be generated via ts-rs from the TS `EntitySchemaJSON` interface (same pattern as our other shared types) — keeps the shape honest on both sides.

## IPC wire changes

**Before (Phase 1):**
```json
{ "command": "data/ensure-schema", "dbPath": "main", "schema": { "collection": "users", "fields": [ /* ... */ ], "indexes": [ /* ... */ ], "foreignKeys": [ /* ... */ ] } }
```

**After (Phase 2):**
```json
{ "command": "data/ensure-schema", "handle": "main", "collection": "users" }
```

Payload shrinks from ~2–10 KB per call to ~50 bytes. Rust has the shape already.

## Implementation sequence

Each step is independently landable:

1. **Codegen script + build wiring** — TS-only change, emits JSON, no consumer yet. Safe to land: it just adds an artifact to the build output.
2. **Rust loader + `EntitySchemaJSON` struct** — adds the loader, nothing calls it yet. Safe.
3. **Rust `ensure_schema` dual-mode** — accept either (legacy) inline `CollectionSchema` or (new) `collection` name. Transitional.
4. **TS `ensure_schema` caller switch** — send `collection` name only; drop `CollectionSchema` construction.
5. **Remove legacy inline path in Rust** — Phase 2 complete. Wire field shrinks.

Each step builds clean on its own; reviewable as separate PRs if we want.

## Open questions

1. **Where does `entity_schemas.json` ship?** Options: (a) in the repo under `src/shared/generated/`, checked in (same as `generated.ts` today); (b) generated at install, never committed. (a) keeps builds reproducible without network; (b) avoids merge conflicts on the generated file. Recommend (a) — consistent with existing generated artifacts.

2. **Do we ship the schema file with the Rust binary, or read from filesystem at runtime?** Runtime read wins flexibility; binary-embed wins one less file to manage. For Docker, the file is in the image via the build context. For native Mac, it's at `$REPO_ROOT/src/shared/generated/entity_schemas.json`. Probably runtime-read with a well-defined resolution order.

3. **What about the `validateData` path in `data/schema`?** Currently TS runs `entity.validate()` in-process. Phase 2 doesn't change this — validation is TS-side entity logic, not SQL. (Though Phase 3+ could argue validation should also move to Rust. Out of scope here.)

4. **What about dynamic / unregistered collections?** Training imports, CLI inspection, etc. With Phase 2, only registered entities get their schema shipped. Non-registered collections would fail `ensure_schema` with "Unknown collection" — which is correct behavior (they should never have had a schema to begin with; writes to them would be rejected). Legacy `data/schema` command's `inferSchemaFromData` code path becomes dead.

5. **Migration engine compatibility.** `MigrationEngine` streams records between adapters. It relies on `ensure_schema` on the target. After Phase 2, migration callsites must use collection names, not inline schemas — same as regular data ops. No architectural change, just a call-site update.

## Merge gate test (applies to this phase)

Before Phase 2 merges: **add a hypothetical `S3EntityAdapter` to Rust**. The only code that should change is the adapter registration. Zero changes to `entities/*`, zero changes to `callers` (data daemon, command handlers, personas). If adding the adapter touches anything above the adapter layer, the abstraction isn't sealed yet.

## Non-goals

- Not renaming the wire field `dbPath` → `handle` (cosmetic polish, separate PR).
- Not changing how TS decorators are authored (Phase 2 is consumption-side).
- Not touching `data/open` for custom runtime handles (those have their own ad-hoc config already; they're the non-entity path).
- Not SQLite-vs-Postgres decision — Phase 5 if at all, per m5's plan.
