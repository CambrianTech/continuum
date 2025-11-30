# SqliteStorageAdapter Refactoring Plan (REVISED)

**CRITICAL**: File is too large to read without crashing (2277 lines). Must break into multiple files FIRST.

**Primary Goal**: Break SqliteStorageAdapter into **multiple separate files/classes** for better organization and readability.

**Secondary Goal**: After decomposition, reduce remaining coordinator class to 300-500 lines by eliminating duplication.

**Current State**:
- SqliteStorageAdapter: 2277 lines ❌ **TOO LARGE**
- SqlStorageAdapterBase: 192 lines
- Phase 1 completed: Extracted SqlNamingConverter, SqliteRawExecutor, SqliteTransactionManager

---

## PHASE 0: FILE DECOMPOSITION (NEW - MUST DO FIRST)

### Problem
The 2277-line SqliteStorageAdapter file is unreadable and crashes tooling. Need to break into logical modules.

### Strategy: Extract Specialized Managers (Composition over Consolidation)

**Goal**: SqliteStorageAdapter becomes a ~300-500 line **coordinator** that delegates to specialized manager classes.

---

### Module 1: SqliteSchemaManager (~350 lines)

**Responsibility**: Table creation, schema migration, integrity verification

**Location**: `daemons/data-daemon/server/managers/SqliteSchemaManager.ts`

**Extracted Methods** (from SqliteStorageAdapter lines 117-628):
- `initialize()` - Database initialization
- `verifyIntegrity()` - PRAGMA integrity_check
- `configureSqlite()` - PRAGMA settings
- `ensureSchema()` - Ensure collection schema exists
- `tableExists()` - Check if table exists
- `getTableColumns()` - Get column list
- `migrateTableSchema()` - Add missing columns
- `migrateSimpleEntityTable()` - Migrate simple entity tables
- `formatDefaultValue()` - Format default value for SQL
- `getDefaultForType()` - Get default value for SQL type
- `logEntitySchemas()` - Log registered entity schemas
- `createCoreSchema()` - Create _collections table

**Interface**:
```typescript
export class SqliteSchemaManager {
  constructor(
    private executor: SqliteRawExecutor,
    private transactionManager: SqliteTransactionManager
  ) {}

  async initialize(config: StorageAdapterConfig): Promise<void>
  async ensureSchema(collectionName: string, schema?: unknown): Promise<StorageResult<boolean>>
  async verifyIntegrity(): Promise<void>
  private async migrateTableSchema(collectionName: string, tableName: string, entityClass: EntityConstructor): Promise<void>
  // ... etc
}
```

**Lines**: ~350 (extracted from 512 lines in SqliteStorageAdapter)

---

### Module 2: SqliteQueryExecutor (~400 lines)

**Responsibility**: Query building and execution for read operations

**Location**: `daemons/data-daemon/server/managers/SqliteQueryExecutor.ts`

**Extracted Methods** (from SqliteStorageAdapter lines 863-1360):
- `read()` - Read single record
- `readFromEntityTable()` - Read from entity-specific table
- `readFromSimpleEntityTable()` - Read from simple entity table
- `query()` - Query multiple records
- `queryFromEntityTable()` - Query entity-specific table
- `queryFromSimpleEntityTable()` - Query simple entity table
- `buildEntitySelectQuery()` - Build SELECT for entity table
- `buildSimpleEntitySelectQuery()` - Build SELECT for simple table
- `buildSelectQuery()` - Build legacy SELECT
- `sanitizeJsonPath()` - Sanitize JSON path
- `buildJsonQuery()` - Build JSON query
- `explainQuery()` - Explain query plan
- `getSqliteQueryPlan()` - Get SQLite query plan
- `estimateRowCount()` - Estimate row count

**Interface**:
```typescript
export class SqliteQueryExecutor {
  constructor(
    private executor: SqliteRawExecutor,
    private queryBuilder: typeof SqliteQueryBuilder
  ) {}

  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>>
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>>
  async explainQuery(query: StorageQuery): Promise<QueryExplanation>

  private async readFromEntityTable<T extends RecordData>(collection: string, id: UUID, entityClass: EntityConstructor): Promise<StorageResult<DataRecord<T>>>
  private async queryFromEntityTable<T extends RecordData>(query: StorageQuery, entityClass: EntityConstructor): Promise<StorageResult<DataRecord<T>[]>>
  // ... etc
}
```

**Lines**: ~400

---

### Module 3: SqliteWriteManager (~350 lines)

**Responsibility**: Create, update, delete operations

**Location**: `daemons/data-daemon/server/managers/SqliteWriteManager.ts`

**Extracted Methods** (from SqliteStorageAdapter lines 685-1862):
- `create()` - Create record
- `createSimpleEntityTable()` - Create simple entity table
- `createInSimpleEntityTable()` - Insert into simple table
- `createLegacyBlob()` - Insert legacy blob
- `update()` - Update record
- `updateInEntityTable()` - Update in entity table
- `updateInSimpleEntityTable()` - Update in simple table
- `delete()` - Delete record
- `batch()` - Batch operations
- `truncate()` - Truncate collection
- `clear()` - Clear all data

**Interface**:
```typescript
export class SqliteWriteManager {
  constructor(
    private executor: SqliteRawExecutor,
    private transactionManager: SqliteTransactionManager,
    private schemaManager: SqliteSchemaManager
  ) {}

  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>>
  async update<T extends RecordData>(collection: string, id: UUID, updates: Partial<T>): Promise<StorageResult<DataRecord<T>>>
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>>
  async batch<T extends RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>>
  async truncate(collection: string): Promise<StorageResult<boolean>>
  async clear(): Promise<StorageResult<boolean>>
}
```

**Lines**: ~350

---

### Module 4: SqliteVectorSearchManager (~280 lines)

**Responsibility**: Vector search operations

**Location**: `daemons/data-daemon/server/managers/SqliteVectorSearchManager.ts`

**Extracted Methods** (from SqliteStorageAdapter lines 2094-2277):
- `vectorSearch()` - Search by vector
- `generateEmbedding()` - Generate embedding
- `indexVector()` - Index vector
- `backfillVectors()` - Backfill vectors
- `getVectorIndexStats()` - Get index stats
- `getVectorSearchCapabilities()` - Get capabilities
- `ensureVectorTable()` - Ensure vector table exists
- `storeVectorInSQLite()` - Store vector
- `getVectorsFromSQLite()` - Get vectors
- `countVectorsInSQLite()` - Count vectors

**Interface**:
```typescript
export class SqliteVectorSearchManager implements VectorSearchAdapter {
  constructor(
    private executor: SqliteRawExecutor,
    private transactionManager: SqliteTransactionManager
  ) {}

  async vectorSearch<T extends RecordData>(request: VectorSearchRequest): Promise<StorageResult<VectorSearchResult<T>>>
  async generateEmbedding(request: GenerateEmbeddingRequest): Promise<StorageResult<EmbeddingResult>>
  async indexVector(request: IndexVectorRequest): Promise<StorageResult<boolean>>
  async backfillVectors(request: BackfillVectorsRequest): Promise<StorageResult<BackfillVectorsResult>>
  async getVectorIndexStats(collection: string): Promise<StorageResult<VectorIndexStats>>
  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities>
}
```

**Lines**: ~280

---

### Module 5: SqliteCollectionManager (~150 lines)

**Responsibility**: Collection-level operations (list, stats, cleanup)

**Location**: `daemons/data-daemon/server/managers/SqliteCollectionManager.ts`

**Extracted Methods** (from SqliteStorageAdapter lines 1593-1962):
- `listCollections()` - List all collections
- `getCollectionStats()` - Get collection statistics
- `cleanup()` - Cleanup resources
- `close()` - Close database
- `ensureCollection()` - Ensure collection exists
- `updateCollectionStats()` - Update collection statistics
- `clearAll()` - Clear all collections

**Interface**:
```typescript
export class SqliteCollectionManager {
  constructor(
    private db: sqlite3.Database,
    private executor: SqliteRawExecutor
  ) {}

  async listCollections(): Promise<StorageResult<string[]>>
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>>
  async ensureCollection(collection: string): Promise<void>
  async updateCollectionStats(collection: string): Promise<void>
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>>
  async cleanup(): Promise<void>
  async close(): Promise<void>
}
```

**Lines**: ~150

---

### Module 6: SqliteStorageAdapter (COORDINATOR - ~400 lines)

**What Remains**: Thin coordinator that delegates to managers

**Location**: `daemons/data-daemon/server/SqliteStorageAdapter.ts` (existing file, gutted)

**Responsibilities**:
- Implements StorageAdapter interface
- Holds references to all managers
- Delegates all operations to appropriate manager
- Minimal coordination logic

**Structure**:
```typescript
export class SqliteStorageAdapter extends SqlStorageAdapterBase implements VectorSearchAdapter {
  private db!: sqlite3.Database;
  private executor!: SqliteRawExecutor;
  private transactionManager!: SqliteTransactionManager;

  // NEW: Manager instances
  private schemaManager!: SqliteSchemaManager;
  private queryExecutor!: SqliteQueryExecutor;
  private writeManager!: SqliteWriteManager;
  private vectorSearchManager!: SqliteVectorSearchManager;
  private collectionManager!: SqliteCollectionManager;

  // Implements base class abstract methods
  protected getSqlDialect(): SqlDialect {
    return 'sqlite';
  }

  protected async executeRawSql(sql: string, params?: SqlValue[]): Promise<Record<string, unknown>[]> {
    return this.executor.runSql(sql, params);
  }

  protected async executeRawStatement(sql: string, params?: SqlValue[]): Promise<{ lastID?: number; changes: number }> {
    return this.executor.runStatement(sql, params);
  }

  // Initialize - creates all managers
  async initialize(config: StorageAdapterConfig): Promise<void> {
    // 1. Open database connection
    // 2. Create executor and transaction manager
    // 3. Create all managers
    // 4. Delegate to schemaManager.initialize()
  }

  // CRUD operations - delegate to managers
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    return this.writeManager.create(record);
  }

  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    return this.queryExecutor.read(collection, id);
  }

  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    return this.queryExecutor.query(query);
  }

  async update<T extends RecordData>(collection: string, id: UUID, updates: Partial<T>): Promise<StorageResult<DataRecord<T>>> {
    return this.writeManager.update(collection, id, updates);
  }

  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    return this.writeManager.delete(collection, id);
  }

  async batch<T extends RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    return this.writeManager.batch(operations);
  }

  // Schema operations - delegate to schemaManager
  async ensureSchema(collectionName: string, schema?: unknown): Promise<StorageResult<boolean>> {
    return this.schemaManager.ensureSchema(collectionName, schema);
  }

  // Collection operations - delegate to collectionManager
  async listCollections(): Promise<StorageResult<string[]>> {
    return this.collectionManager.listCollections();
  }

  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    return this.collectionManager.getCollectionStats(collection);
  }

  async truncate(collection: string): Promise<StorageResult<boolean>> {
    return this.writeManager.truncate(collection);
  }

  async clear(): Promise<StorageResult<boolean>> {
    return this.writeManager.clear();
  }

  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    return this.collectionManager.clearAll();
  }

  // Vector search operations - delegate to vectorSearchManager
  async vectorSearch<T extends RecordData>(request: VectorSearchRequest): Promise<StorageResult<VectorSearchResult<T>>> {
    return this.vectorSearchManager.vectorSearch(request);
  }

  async generateEmbedding(request: GenerateEmbeddingRequest): Promise<StorageResult<EmbeddingResult>> {
    return this.vectorSearchManager.generateEmbedding(request);
  }

  async indexVector(request: IndexVectorRequest): Promise<StorageResult<boolean>> {
    return this.vectorSearchManager.indexVector(request);
  }

  async backfillVectors(request: BackfillVectorsRequest): Promise<StorageResult<BackfillVectorsResult>> {
    return this.vectorSearchManager.backfillVectors(request);
  }

  async getVectorIndexStats(collection: string): Promise<StorageResult<VectorIndexStats>> {
    return this.vectorSearchManager.getVectorIndexStats(collection);
  }

  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities> {
    return this.vectorSearchManager.getVectorSearchCapabilities();
  }

  // Query explanation - delegate to queryExecutor
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    return this.queryExecutor.explainQuery(query);
  }

  // Lifecycle operations - delegate to collectionManager
  async cleanup(): Promise<void> {
    await this.collectionManager.cleanup();
  }

  async close(): Promise<void> {
    await this.collectionManager.close();
  }
}
```

**Lines**: ~400 (down from 2277)

---

## Phase 0 Summary

### Before (monolithic):
```
SqliteStorageAdapter.ts: 2277 lines ❌
```

### After (decomposed):
```
SqliteStorageAdapter.ts: ~400 lines (coordinator)
managers/
  SqliteSchemaManager.ts: ~350 lines
  SqliteQueryExecutor.ts: ~400 lines
  SqliteWriteManager.ts: ~350 lines
  SqliteVectorSearchManager.ts: ~280 lines
  SqliteCollectionManager.ts: ~150 lines

Total: ~1930 lines (same logic, better organized)
```

### Benefits:
- ✅ Each file is readable (< 500 lines)
- ✅ Clear separation of concerns
- ✅ Easy to navigate and understand
- ✅ Manager classes can be tested independently
- ✅ No more crashing when trying to read the file

---

## Phase 0 Implementation Steps

### Step 1: Create directory structure
```bash
mkdir -p daemons/data-daemon/server/managers
```

### Step 2: Extract SqliteSchemaManager (~350 lines)
- Create `managers/SqliteSchemaManager.ts`
- Move initialization and schema management methods
- Update SqliteStorageAdapter to use schemaManager
- Test: `npm run build:ts && npm start`

### Step 3: Extract SqliteQueryExecutor (~400 lines)
- Create `managers/SqliteQueryExecutor.ts`
- Move read/query methods
- Update SqliteStorageAdapter to use queryExecutor
- Test: `./jtag data/list --collection=users`

### Step 4: Extract SqliteWriteManager (~350 lines)
- Create `managers/SqliteWriteManager.ts`
- Move create/update/delete/batch methods
- Update SqliteStorageAdapter to use writeManager
- Test: `./jtag data/create`, `./jtag data/update`, `./jtag data/delete`

### Step 5: Extract SqliteVectorSearchManager (~280 lines)
- Create `managers/SqliteVectorSearchManager.ts`
- Move vector search methods
- Update SqliteStorageAdapter to use vectorSearchManager
- Test: Vector search operations

### Step 6: Extract SqliteCollectionManager (~150 lines)
- Create `managers/SqliteCollectionManager.ts`
- Move collection-level operations
- Update SqliteStorageAdapter to use collectionManager
- Test: `./jtag data/list --collection=*`

### Step 7: Verify SqliteStorageAdapter is now ~400 lines
- Check line count: `wc -l SqliteStorageAdapter.ts`
- Verify all tests pass: `npm run test:crud`
- Take screenshot to celebrate

---

## AFTER Phase 0: Continue with Original Phases

Once file decomposition is complete and SqliteStorageAdapter is ~400 lines, THEN proceed with the original optimization plan:

- **Phase 2A**: Move generic SQL logic UP to SqlStorageAdapterBase (~550 lines saved)
- **Phase 2B**: Use SqliteQueryBuilder properly (~150 lines saved)
- **Phase 2C**: Additional cleanup (~470 lines saved)

**Final Result**:
- SqliteStorageAdapter: ~300 lines (coordinator only)
- Managers: 5 files, ~1500 lines total
- Base class: SqlStorageAdapterBase with ~550 lines of reusable SQL logic

---

## Original Plan (Phases 2A, 2B, 2C)

The following sections document the ORIGINAL refactoring plan. These phases should be executed AFTER Phase 0 (file decomposition) is complete.

---

## Phase 2A: Move Generic SQL Logic UP to SqlStorageAdapterBase

### 2A.1 - Filter Translation (~150 lines saved)

**Problem**: SqliteStorageAdapter.buildEntitySelectQuery() (lines 1115-1184) duplicates SqliteQueryBuilder.buildOperatorClause() (lines 111-230).

**Solution**: Move operator mapping to SqlStorageAdapterBase.

**Files to Change**:

1. **SqlStorageAdapterBase.ts** - Add method:
```typescript
/**
 * Build WHERE clause from universal filter operators
 * Returns SQL clause, parameters, and human-readable description
 */
protected buildFilterClause(
  filters: Record<string, unknown>,
  fieldNameMapper: (field: string) => string
): { clauses: string[]; params: unknown[]; descriptions: string[] } {
  const clauses: string[] = [];
  const params: unknown[] = [];
  const descriptions: string[] = [];

  for (const [field, filter] of Object.entries(filters)) {
    const columnName = fieldNameMapper(field);

    if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
      // Handle operators: $eq, $ne, $gt, $gte, $lt, $lte, $in, $nin, $exists, $regex, $contains
      for (const [operator, value] of Object.entries(filter)) {
        const result = this.buildOperatorClause(columnName, operator, value, field);
        clauses.push(result.clause);
        params.push(...result.params);
        descriptions.push(result.description);
      }
    } else {
      // Direct value implies $eq
      clauses.push(`${columnName} = ?`);
      params.push(filter);
      descriptions.push(`field "${field}" equals ${JSON.stringify(filter)}`);
    }
  }

  return { clauses, params, descriptions };
}

/**
 * Build single operator clause ($eq, $gt, $in, etc.)
 */
private buildOperatorClause(
  columnName: string,
  operator: string,
  value: unknown,
  field: string
): { clause: string; params: unknown[]; description: string } {
  // Copy implementation from SqliteQueryBuilder.buildOperatorClause()
  // Lines 111-230 from SqliteQueryBuilder.ts
}
```

2. **SqliteQueryExecutor.ts** - Use base class for filter building (instead of inline duplication)

**Lines Saved**: ~150 lines

---

### 2A.2 - Result Mapping (~200 lines saved)

**Problem**: SqliteQueryExecutor has duplicate type conversion logic in multiple methods.

**Solution**: Move SQL row → DataRecord<T> conversion to SqlStorageAdapterBase.

**Files to Change**:

1. **SqlStorageAdapterBase.ts** - Add method:
```typescript
/**
 * Map SQL row to DataRecord<T>
 * Handles type conversion (boolean, json, date) based on field metadata
 */
protected mapSqlRowToRecord<T extends RecordData>(
  row: Record<string, unknown>,
  collection: string,
  entityClass: EntityConstructor | undefined,
  fieldNameMapper: (field: string) => string
): DataRecord<T> {
  if (!entityClass || !hasFieldMetadata(entityClass)) {
    // Simple entity table (JSON data column)
    return {
      id: row.id as UUID,
      collection,
      data: JSON.parse(row.data as string),
      metadata: {
        createdAt: row.created_at as string,
        updatedAt: row.updated_at as string,
        version: row.version as number
      }
    };
  }

  // Entity-specific table with field metadata
  const entityData: any = {};
  const fieldMetadata = getFieldMetadata(entityClass);

  for (const [fieldName, metadata] of fieldMetadata.entries()) {
    const columnName = fieldNameMapper(fieldName);
    let value = row[columnName];

    if (value !== undefined && value !== null) {
      // Convert SQL value back to JavaScript type
      value = this.convertSqlValueToJs(value, metadata.fieldType);

      // Put BaseEntity fields in metadata, others in data
      if (!['id', 'createdAt', 'updatedAt', 'version'].includes(fieldName)) {
        entityData[fieldName] = value;
      }
    }
  }

  return {
    id: row.id as UUID,
    collection,
    data: entityData as T,
    metadata: {
      createdAt: row.created_at as string,
      updatedAt: row.updated_at as string,
      version: row.version as number
    }
  };
}
```

2. **SqliteQueryExecutor.ts** - Use base class method

**Lines Saved**: ~200 lines

---

### 2A.3 - Schema Migration (~200 lines saved)

**Problem**: SqliteSchemaManager has schema migration logic that's generic SQL, not SQLite-specific.

**Solution**: Move column addition logic to SqlStorageAdapterBase.

**Files to Change**:

1. **SqlStorageAdapterBase.ts** - Add method for ALTER TABLE logic
2. **SqliteSchemaManager.ts** - Use base class method

**Lines Saved**: ~200 lines

---

## Phase 2B: Use SqliteQueryBuilder Properly (Eliminate Duplication)

### 2B.1 - Replace buildEntitySelectQuery with SqliteQueryBuilder

**Problem**: SqliteQueryExecutor.buildEntitySelectQuery() duplicates SqliteQueryBuilder.buildSelect().

**Solution**: DELETE method, use SqliteQueryBuilder directly.

**Files to Change**:

1. **SqliteQueryExecutor.ts** - Replace inline query building with SqliteQueryBuilder calls

**Lines Saved**: ~150 lines

---

## Phase 2C: Additional Cleanup

### 2C.3 - Fix Batch Operations to Use SqliteTransactionManager

**Problem**: SqliteWriteManager.batch() has manual transaction management.

**Solution**: Refactor to use existing SqliteTransactionManager.

**Files to Change**:

1. **SqliteWriteManager.ts** - Use transactionManager.withTransaction()

**Lines Saved**: ~70 lines

---

## Summary of Line Count Reductions

| Phase | Description | Lines Saved | New Location |
|-------|-------------|-------------|--------------|
| **Phase 0** | **File Decomposition** | **N/A** | **5 manager classes** |
| **2A.1** | Filter Translation | ~150 | SqlStorageAdapterBase |
| **2A.2** | Result Mapping | ~200 | SqlStorageAdapterBase |
| **2A.3** | Schema Migration | ~200 | SqlStorageAdapterBase |
| **2B.1** | Use SqliteQueryBuilder | ~150 | Already exists |
| **2C.3** | Fix Batch Operations | ~70 | Use existing TransactionManager |
| **Total (Phases 2A-2C)** | | **~770 lines** | |

**Result After Phase 0**: SqliteStorageAdapter: 2277 → ~400 lines (coordinator)
**Result After All Phases**: SqliteStorageAdapter: ~300 lines + 5 manager classes (~1500 lines total)

---

## Implementation Order (REVISED)

### Priority 1: Phase 0 (File Decomposition) - DO THIS FIRST
Start with breaking the monolithic file into multiple classes.
- **Step 0.1**: Extract SqliteSchemaManager (~350 lines)
- **Step 0.2**: Extract SqliteQueryExecutor (~400 lines)
- **Step 0.3**: Extract SqliteWriteManager (~350 lines)
- **Step 0.4**: Extract SqliteVectorSearchManager (~280 lines)
- **Step 0.5**: Extract SqliteCollectionManager (~150 lines)
- **Step 0.6**: Reduce SqliteStorageAdapter to ~400 line coordinator

### Priority 2: Phase 2B (Quick Win)
Use SqliteQueryBuilder in SqliteQueryExecutor - easiest optimization.

### Priority 3: Phase 2C.3 (Fix Batch)
Fix batch operations in SqliteWriteManager to use SqliteTransactionManager.

### Priority 4: Phases 2A.1, 2A.2, 2A.3
Move generic SQL logic to base class (lower priority, more invasive).

---

## Testing Strategy

### After Each Phase 0 Step:
1. **TypeScript Compilation**: `npm run build:ts`
2. **CRUD Tests**: `npm run test:crud`
3. **Manual Testing**: `./jtag data/list --collection=users`

### Phase-Specific Tests:

**Phase 0** (File Decomposition):
- Test all CRUD operations after each manager extraction
- Verify no behavior changes
- Check line counts: `wc -l SqliteStorageAdapter.ts managers/*.ts`

**Phase 2B** (SqliteQueryBuilder):
- Test query with filters: `data/list --filter='{"active":true}'`
- Test query with operators: `data/list --filter='{"age":{"$gt":18}}'`
- Test query with sorting: `data/list --orderBy='[{"field":"createdAt","direction":"desc"}]'`

**Phase 2C.3** (Batch Operations):
- Test batch create/update/delete
- Verify transaction rollback on error

---

## Post-Refactoring Benefits

### For SqliteStorageAdapter (Phase 0):
- Reduced from 2277 → ~400 lines (thin coordinator)
- Clear separation of concerns across 5 manager classes
- Each manager < 500 lines (readable and maintainable)
- No more "file too large to read" errors

### For Managers:
- SqliteSchemaManager: Schema and migrations
- SqliteQueryExecutor: Read operations
- SqliteWriteManager: Write operations
- SqliteVectorSearchManager: Vector search
- SqliteCollectionManager: Collection lifecycle

### For Future Work (Phases 2A-2C):
- Further reduce SqliteStorageAdapter to ~300 lines
- Push generic SQL logic to SqlStorageAdapterBase
- Ready for PostgreSQL/MySQL adapters

---

## Architecture After Phase 0

```
SqliteStorageAdapter (~400 lines - coordinator)
├── delegates to managers:
    ├── SqliteSchemaManager (~350 lines)
    ├── SqliteQueryExecutor (~400 lines)
    ├── SqliteWriteManager (~350 lines)
    ├── SqliteVectorSearchManager (~280 lines)
    └── SqliteCollectionManager (~150 lines)
```

---

## Success Criteria

✅ **Phase 0 Complete**:
- SqliteStorageAdapter reduced to ~400 lines (coordinator only)
- 5 manager classes created in `managers/` directory
- All tests pass (CRUD + integration)
- No behavior changes (pure refactoring)
- TypeScript compilation succeeds
- Each file < 500 lines (readable)

✅ **All Phases Complete**:
- SqliteStorageAdapter further reduced to ~300 lines
- All tests pass
- Code coverage maintained
- Ready for PostgreSQL adapter implementation

---

## Notes

- **Backwards Compatibility**: All refactoring maintains existing behavior
- **No Breaking Changes**: Public API unchanged
- **Incremental**: Each step can be committed separately
- **Testable**: Full test coverage at each step
- **Reversible**: Each step can be reverted if needed

---

**Status**: Ready for Phase 0 implementation (file decomposition)
**Start With**: Extract SqliteSchemaManager
**Estimated Total Time**: 2-3 days for Phase 0, then 1-2 days for Phases 2A-2C
