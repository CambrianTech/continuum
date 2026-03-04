# Data Adapter Architecture

> **⚠️ ARCHITECTURE UPDATE**: RustWorkerStorageAdapter has been removed. The data path is now:
> `ORM.ts` → `ORMRustClient.ts` → `/tmp/continuum-core.sock` → `DataModule` (Rust).
> See `workers/continuum-core/src/modules/data.rs` for the current implementation.

## Layer Overview

```
┌─────────────────────────────────────────────────────────────┐
│  ENTITIES (Decorators)                                       │
│  - Define fields, types, relationships generically          │
│  - @Field({ type: 'uuid' }), @Index(), @Relationship()      │
│  - All camelCase (TypeScript convention)                    │
│  - No knowledge of storage backend                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  DATA DAEMON / ORM (ORCHESTRATOR)                            │
│  - ORCHESTRATES all data operations                         │
│  - Interprets entity decorators                             │
│  - Works ONLY in generic concepts (entities, relationships) │
│  - HAS NO STORAGE DOMAIN KNOWLEDGE - knows NOTHING of SQL,  │
│    JSON, or any specific storage technology                 │
│  - BUT DOES KNOW: entities, their fields, field metadata    │
│    (types, indexes, relationships) from decorators          │
│  - Passes requirements to adapters in generic terms:        │
│    "index this field", "use this data type",                │
│    "you will return it in this data type"                   │
│  - Coordinates between multiple adapters if needed          │
│  - Handles caching, events, validation at this layer        │
│  - All camelCase                                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  ADAPTERS (SQL, JSON, Vector, etc.)                         │
│  - Receive generic requirements from daemon                 │
│  - Translate to NATIVE format (this is the boundary)        │
│  - Implement requirements in adapter-specific way           │
│  - "Store how you want, return in requested type"           │
└─────────────────────────────────────────────────────────────┘
```

## Adapter Responsibilities

### 1. Case Conversion at Boundary
- **Input**: Receive camelCase from ORM layer
- **Internal**: Work in native format (snake_case for SQL, kebab-case for JSON, etc.)
- **Output**: Convert back to camelCase for ORM layer

The adapter IS the translation boundary. Everything above it is generic camelCase.

### 2. Type Handling
- Decorator says: `@Field({ type: 'uuid' })`
- Adapter decides HOW to store:
  - PostgreSQL: native UUID type
  - SQLite: TEXT with format validation
  - JSON: string
- Adapter MUST return the requested type regardless of internal storage

### 3. Indexing
- Decorator says: `@Index()`
- Adapter implements in native way:
  - SQL: CREATE INDEX
  - Some adapters may not support indexing (that's fine)
- Adapters report capabilities via `getCapabilities()`

### 4. Schema/Table Building
- Adapters are like ORMs - they build tables from entity schemas
- NOT dumb blob storage (no dumping JSON into a `data` column)
- Real columns, real types, real indexes
- All derived from generic entity definitions

## Adapter Capability Variance

Adapters may lack capabilities. This is expected:

```typescript
interface StorageCapabilities {
  supportsTransactions: boolean;
  supportsIndexing: boolean;
  supportsFullTextSearch: boolean;
  supportsReplication: boolean;
  supportsNativeUUID: boolean;  // etc.
}
```

The ORM/Daemon layer can query capabilities and adjust behavior accordingly.

## SQL Adapter Specifics

For SQL-based adapters (SQLite, PostgreSQL, Rust worker):

- **Table names**: snake_case (e.g., `user_entities`)
- **Column names**: snake_case (e.g., `unique_id`, `display_name`)
- **Above adapter**: camelCase (`uniqueId`, `displayName`)

The conversion happens IN the adapter, not above or below.

## Rust Worker Adapter

The RustWorkerStorageAdapter must:

1. **Receive** camelCase data/queries from Data Daemon
2. **Convert to snake_case** before sending to Rust worker
3. **Rust worker** executes SQL (all snake_case internally)
4. **Convert results back to camelCase**
5. **Return** camelCase to Data Daemon

The Rust worker itself works in snake_case (natural for SQL). The TypeScript
RustWorkerStorageAdapter wrapper handles the boundary conversion.

---

## AUDIT: Legacy Violations Found

### Violation 1: Adapter Has Entity Knowledge

**Location**: `daemons/data-daemon/server/managers/SqliteQueryExecutor.ts:169-177`

```typescript
const entityClass = ENTITY_REGISTRY.get(query.collection);
if (entityClass && hasFieldMetadata(entityClass)) {
  return await this.queryFromEntityTable<T>(query, entityClass);
} else {
  return await this.queryFromSimpleEntityTable<T>(query);
}
```

**Problem**: The adapter directly accesses `ENTITY_REGISTRY` and `getFieldMetadata()`.
Per architecture: Daemon knows entities, Adapter is dumb.

**Fix**: Daemon should pass schema/field metadata WITH each request. Adapter receives
generic schema, doesn't look up entities itself.

### Violation 2: SQL Logic Outside Adapter

**Location**: `commands/data/schema/server/DataSchemaServerCommand.ts`

```typescript
private toSnakeCase(str: string): string { ... }
// Command has its own SQL naming converter!
```

**Location**: `system/data/core/FieldMapping.ts`
```typescript
// SQLite-specific column names (snake_case convention)
```

**Problem**: SQL-specific naming conventions leak outside the adapter boundary.
Commands and core data system know about snake_case = SQL boundary violated.

**Fix**: Only the SQL adapter should know about snake_case. Everything above
uses camelCase. The adapter translates at boundary.

### Violation 3: Two Incompatible Storage Formats

**Location**: `SqliteQueryExecutor.ts`

1. **Entity-specific tables**: Real columns, uses decorator metadata
2. **Simple entity tables**: JSON `data` column blob

**Problem**: Adapter decides which format based on entity registry lookup.
This decision should be made by daemon, not adapter.

**Fix**: One consistent format, or daemon explicitly tells adapter which format.

### Violation 4: Schema Parameter Too Loose

**Location**: `daemons/data-daemon/shared/DataStorageAdapter.ts:204`

```typescript
abstract ensureSchema(collection: string, schema?: unknown): Promise<...>
```

**Good**: Interface DOES have schema parameter for daemon to pass schema to adapter!
**Problem**: `schema?: unknown` is too loose. Not typed, not consistently used.

**Fix**: Define proper `CollectionSchema` type with fields, types, indexes.
Daemon passes schema. Adapter caches it, uses for query/write translation.

---

## Incremental Fix Strategy

1. **Define CollectionSchema type** - field names, types, indexes ✅ DONE
2. **Daemon passes schema on ensureSchema()** - before first use of collection ✅ DONE
3. **Adapter caches schema per collection** - no entity registry lookup ✅ DONE
4. **Remove ENTITY_REGISTRY from adapter** - adapter becomes dumb (in progress - new path works, legacy preserved)
5. **SQL naming conversion stays IN adapter** - but uses cached schema ✅ DONE

This is piecemeal - each step can be done and tested independently.

---

## FIX STATUS (2026-01-01)

### Completed: Schema-Based Path

The new architecture is now fully implemented across all managers:

1. **DataDaemon.ts** - Extracts schema from entity decorators ✅
   ```typescript
   private extractCollectionSchema(collection: string): CollectionSchema | undefined
   private mapFieldTypeToSchemaType(fieldType: FieldType): SchemaFieldType
   ```

2. **SqliteSchemaManager.ts** - Uses schema when provided ✅
   ```typescript
   private schemaCache = new Map<string, CollectionSchema>();
   getCachedSchema(collection: string): CollectionSchema | undefined
   private generateCreateTableFromSchema(schema: CollectionSchema): string
   private generateCreateIndexFromSchema(schema: CollectionSchema): string[]
   ```

3. **SqliteStorageAdapter.ts** - Wires up schema getters to managers ✅
   ```typescript
   // Wire up schema getters for managers (NEW ARCHITECTURE)
   const schemaGetter = (collection: string) => this.schemaManager.getCachedSchema(collection);
   this.writeManager.setSchemaGetter(schemaGetter);
   this.queryExecutor.setSchemaGetter(schemaGetter);
   ```

4. **SqliteQueryExecutor.ts** - Uses schema for read/query operations ✅
   ```typescript
   private getSchema: SchemaGetter | null = null;
   setSchemaGetter(getter: SchemaGetter): void
   private readFromSchema<T>(collection, id, schema): Promise<...>
   private queryFromSchema<T>(query, schema): Promise<...>
   private buildSchemaSelectQuery(query, schema): { sql, params }
   ```

5. **SqliteWriteManager.ts** - Uses schema for create/update/delete ✅
   ```typescript
   private getSchema: SchemaGetter | null = null;
   setSchemaGetter(getter: SchemaGetter): void
   private createFromSchema<T>(record, schema): Promise<...>
   private updateFromSchema<T>(collection, id, data, version, schema): Promise<...>
   ```

6. **Log Output** - Shows which path is used
   - `[SCHEMA-PATH]` - New architecture (daemon provides schema)
   - `[LEGACY-PATH]` - Old architecture (adapter looks up ENTITY_REGISTRY)

### Critical Bugs Fixed (2026-01-01)

1. **INSERT statements not persisting** (SqliteWriteManager.ts:171)
   - Bug: Used `runSql()` which is for SELECT queries
   - Fix: Changed to `runStatement()` which is for INSERT/UPDATE/DELETE
   - Symptom: Messages appeared in UI (via events) but disappeared on refresh

2. **Date fields stored as null** (SqliteWriteManager.ts:162-164, 367-369)
   - Bug: `createFromSchema()` and `updateFromSchema()` had no handling for `date` field type
   - Fix: Added date serialization like the legacy path:
     ```typescript
     } else if (field.type === 'date') {
       values.push(typeof fieldValue === 'string' ? fieldValue : new Date(fieldValue).toISOString());
     }
     ```
   - Symptom: Messages persisted but `timestamp` field was null, breaking ORDER BY queries

### Still TODO

- Remove legacy ENTITY_REGISTRY path from all managers once confirmed stable
- Implement schema-based column migration (currently skipped for existing tables)
- Remove ENTITY_REGISTRY imports from adapter files (legacy code preserved for fallback)

---

## Anti-Patterns

### WRONG: Blob Storage
```typescript
// DON'T: Dump everything into a JSON blob
INSERT INTO entities (id, data) VALUES (?, ?)
// data = '{"uniqueId": "...", "displayName": "..."}'
```

### WRONG: Pass-Through Snake Case
```typescript
// DON'T: Return raw SQL results without conversion
return { unique_id: row.unique_id }  // Should be uniqueId
```

### WRONG: Hardcoded Table Knowledge
```typescript
// DON'T: Know about specific tables
if (collection === 'users') { /* special handling */ }
// Adapter should be generic
```

### RIGHT: Generic Schema-Driven
```typescript
// DO: Build tables from schema, convert at boundary
const columns = schema.fields.map(f => ({
  name: toSnakeCase(f.name),
  type: mapToSqlType(f.type)
}));
// CREATE TABLE with proper columns
// Return with camelCase conversion
```
