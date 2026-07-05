# DataDaemon Architecture Issues & Fixes Needed

**Date**: 2025-11-22
**Status**: DOCUMENTED (not yet implemented)

---

## Current Problems

### 1. **ENTITY_REGISTRY is Global (Should be Per-Handle)**

```typescript
// daemons/data-daemon/server/SqliteStorageAdapter.ts:51
const ENTITY_REGISTRY = new Map<string, EntityConstructor>();  // ← GLOBAL!
```

**Problem**: Multiple databases share one entity registry
- Can't have different schemas per database
- Multi-tenant scenarios break
- Persona-specific collections pollute global namespace

**Should be**: Registry per adapter instance (per DbHandle)

### 2. **Adapters Create Tables Independently (Should be Orchestrated)**

**Current flow**:
```
Hippocampus → Commands.execute('data/create')
  → DataDaemon.validateSchema() [checks entity exists]
  → SqliteStorageAdapter.create() [creates table if needed]
```

**Problem**: DataDaemon and Adapter are out of sync
- Adapter can create tables without entities
- DataDaemon requires entities for validation
- No single source of truth for "does this collection exist?"

**Should be**: DataDaemon orchestrates, adapter implements

---

## Proper Architecture

### Design Pattern: **Template Method**

DataDaemon defines the flow, adapters implement the details.

```typescript
// DataDaemon orchestrates (public API)
async create<T>(collection: string, data: T): Promise<Result> {
  // 1. Validate data (if entity registered)
  const validation = this.validateSchema(collection, data);
  if (!validation.success) throw validation.error;

  // 2. Ensure collection exists (orchestrated by daemon)
  await this.ensureCollection(collection);

  // 3. Delegate to adapter
  return this.adapter.create(collection, data);
}

// DataDaemon orchestrates table creation
private async ensureCollection(collection: string): Promise<void> {
  // Check if already ensured (cache per-handle)
  if (this.ensuredCollections.has(collection)) return;

  // Get schema (from entity or null for custom collections)
  const entityClass = this.adapter.getRegisteredEntity(collection);
  const schema = entityClass ? this.extractSchema(entityClass) : null;

  // Call adapter's protected method (generic - works for SQL tables, Mongo collections, etc.)
  await this.adapter.ensureSchema(collection, schema);

  // Mark as ensured
  this.ensuredCollections.add(collection);
}
```

```typescript
// SqliteStorageAdapter implements (protected method)
export class SqliteStorageAdapter {
  // Per-instance entity registry (not global!)
  private entityRegistry = new Map<string, EntityConstructor>();

  // Protected method - called by DataDaemon (schema = SQL table in this adapter)
  protected async ensureSchema(
    collection: string,
    schema?: TableSchema
  ): Promise<void> {
    if (schema) {
      // Entity-based table (use decorators)
      await this.createEntityTable(collection, schema);
    } else {
      // Custom collection (simple table)
      await this.createSimpleTable(collection);
    }
  }

  // Public method - register entities for THIS adapter instance
  registerEntity(collectionName: string, entityClass: EntityConstructor): void {
    this.entityRegistry.set(collectionName, entityClass);
  }

  // Public method - get entity for THIS adapter instance
  getRegisteredEntity(collectionName: string): EntityConstructor | undefined {
    return this.entityRegistry.get(collectionName);
  }
}
```

---

## Benefits of Proper Architecture

### 1. **Single Orchestrator**
- DataDaemon manages: "ensure collection exists"
- Adapter implements: "how to create table"
- One place to cache what's been ensured

### 2. **Per-Handle Isolation**
```typescript
const mainDb = await Commands.execute('data/open', { path: 'main.db' });
const personaDb = await Commands.execute('data/open', { path: 'persona.db' });

// Each has own entity registry!
mainDb.registerEntity('users', UserEntity);
personaDb.registerEntity('memories', MemoryEntity);  // ← Doesn't pollute mainDb
```

### 3. **Adapter Flexibility**
```typescript
class MongoAdapter extends DataStorageAdapter {
  // MongoDB doesn't need table creation
  protected async ensureTable(collection: string): Promise<void> {
    // Do nothing - Mongo creates collections automatically
  }
}

class PostgresAdapter extends SqlStorageAdapterBase {
  // Postgres needs table creation (95% reuses SQLite logic)
  protected async ensureTable(collection: string, schema?: TableSchema): Promise<void> {
    // Use inherited SQL logic with Postgres-specific tweaks
    await super.ensureTable(collection, schema);
  }
}
```

---

## Migration Path

### Phase 1: Immediate Fix (DONE)
✅ Skip validation for unregistered collections
- Allows custom collections like "memories" to work
- Maintains backward compatibility
- Doesn't break existing code

### Phase 2: Move Registry to Per-Instance (TODO)
- [ ] Add `private entityRegistry` to SqliteStorageAdapter class
- [ ] Remove global `ENTITY_REGISTRY`
- [ ] Make `registerEntity()` instance method
- [ ] Update all entity registration code

### Phase 3: Orchestrate from DataDaemon (TODO)
- [ ] Add `ensureCollection()` to DataDaemon
- [ ] Add `protected ensureTable()` to adapter interface
- [ ] Move cache to DataDaemon level
- [ ] Remove adapter-level `ensureEntityTable()` calls

### Phase 4: Test Multi-Database (TODO)
```typescript
// Verify isolated registries work
const db1 = await openDatabase('test1.db');
const db2 = await openDatabase('test2.db');

db1.registerEntity('custom', CustomEntity1);
db2.registerEntity('custom', CustomEntity2);

// Should NOT conflict!
```

---

## Current Workaround (Until Refactor)

The immediate fix (skipping validation for unregistered collections) makes things work but doesn't fix the architecture:

```typescript
// DataDaemon.validateSchema() - CURRENT WORKAROUND
if (!EntityClass) {
  // No entity registered - skip validation (custom collection)
  console.log(`⚠️ DataDaemon: No entity registered for "${collection}" - skipping validation`);
  return { success: true };  // ← Allows memories to work
}
```

**This is fine for now**, but the proper fix is moving to per-handle registries with DataDaemon orchestration.

---

## Why This Matters

**Scenario**: Multi-tenant SaaS with persona-specific collections

```typescript
// Tenant A's persona database
const personaA = await openDatabase('.continuum/personas/tenant-a-persona/memory.db');
personaA.registerEntity('memories', MemoryEntity);
personaA.registerEntity('skills', SkillEntity);

// Tenant B's persona database
const personaB = await openDatabase('.continuum/personas/tenant-b-persona/memory.db');
personaB.registerEntity('memories', MemoryEntity);
personaB.registerEntity('custom_data', CustomEntity);

// Main database
const main = await openDatabase('main.db');
main.registerEntity('users', UserEntity);
main.registerEntity('rooms', RoomEntity);
```

**With global registry**: All three share entities → conflicts, pollution
**With per-handle registry**: Fully isolated → clean, scalable

---

## References

- **Template Method Pattern**: Gang of Four Design Patterns
- **Multi-tenancy**: Isolated schemas per tenant
- **Dependency Inversion**: High-level (DataDaemon) orchestrates, low-level (Adapter) implements
