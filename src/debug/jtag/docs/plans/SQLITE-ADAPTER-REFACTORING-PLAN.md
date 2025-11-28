# SqliteStorageAdapter Refactoring Plan

**Goal**: Reduce SqliteStorageAdapter from 2277 lines to 800-1000 lines by eliminating duplication with base classes and extracting utilities.

**Current State**:
- SqliteStorageAdapter: 2277 lines
- SqlStorageAdapterBase: 192 lines
- Phase 1 completed: Extracted SqlNamingConverter, SqliteRawExecutor, SqliteTransactionManager

**Problem**: SqliteStorageAdapter reimplements everything instead of delegating to base classes, despite having 192 lines of base class code available.

---

## Architecture Analysis

### What SqlStorageAdapterBase Currently Provides (192 lines)
- `mapFieldTypeToSql()` - Field type → SQL type mapping (lines 54-92)
- `generateCreateTableSql()` - CREATE TABLE generation from metadata (lines 97-152)
- `generateCreateIndexSql()` - CREATE INDEX generation from metadata (lines 157-176)

### What SqliteStorageAdapter Reimplements (2277 lines)
Everything else, including logic that should be in base classes.

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

2. **SqliteStorageAdapter.ts** - DELETE lines 1115-1184, use base class:
```typescript
// BEFORE (144 lines of duplication)
private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
  // ... 144 lines of query building with filter duplication ...
}

// AFTER (use base class)
private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
  const tableName = SqlNamingConverter.toTableName(query.collection);
  let sql = `SELECT * FROM ${tableName}`;
  const allParams: any[] = [];

  // Use base class for filter building
  if (query.filter) {
    const { clauses, params } = this.buildFilterClause(query.filter, SqlNamingConverter.toSnakeCase);
    if (clauses.length > 0) {
      sql += ` WHERE ${clauses.join(' AND ')}`;
      allParams.push(...params);
    }
  }

  // ... rest of query building (ORDER BY, LIMIT, etc.) ...
  return { sql, params: allParams };
}
```

**Lines Saved**: ~150 lines

---

### 2A.2 - Result Mapping (~200 lines saved)

**Problem**: SqliteStorageAdapter has separate methods for mapping SQL rows to DataRecord<T>:
- `readFromEntityTable()` (lines 887-948)
- `readFromSimpleEntityTable()` (lines 953-981)
- Duplicate type conversion logic in `queryFromEntityTable()` (lines 1012-1071)

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

/**
 * Convert SQL value to JavaScript type based on field type
 */
private convertSqlValueToJs(value: unknown, fieldType: FieldType): unknown {
  switch (fieldType) {
    case 'boolean':
      return value === 1;
    case 'json':
      return JSON.parse(value as string);
    case 'date':
      return new Date(value as string);
    default:
      return value;
  }
}
```

2. **SqliteStorageAdapter.ts** - DELETE lines 887-948, 953-981, use base class:
```typescript
// BEFORE (62 lines)
private async readFromEntityTable<T extends RecordData>(
  collection: string,
  id: UUID,
  entityClass: EntityConstructor
): Promise<StorageResult<DataRecord<T>>> {
  const tableName = SqlNamingConverter.toTableName(collection);
  const sql = `SELECT * FROM ${tableName} WHERE id = ? LIMIT 1`;
  const rows = await this.executor.runSql(sql, [id]);

  if (rows.length === 0) {
    return { success: false, error: `Record not found: ${collection}/${id}` };
  }

  const row = rows[0];
  const entityData: any = {};
  // ... 40 lines of type conversion ...
  return { success: true, data: record };
}

// AFTER (10 lines)
private async readFromEntityTable<T extends RecordData>(
  collection: string,
  id: UUID,
  entityClass: EntityConstructor
): Promise<StorageResult<DataRecord<T>>> {
  const tableName = SqlNamingConverter.toTableName(collection);
  const sql = `SELECT * FROM ${tableName} WHERE id = ? LIMIT 1`;
  const rows = await this.executor.runSql(sql, [id]);

  if (rows.length === 0) {
    return { success: false, error: `Record not found: ${collection}/${id}` };
  }

  const record = this.mapSqlRowToRecord<T>(rows[0], collection, entityClass, SqlNamingConverter.toSnakeCase);
  return { success: true, data: record };
}
```

**Lines Saved**: ~200 lines

---

### 2A.3 - Schema Migration (~200 lines saved)

**Problem**: SqliteStorageAdapter has schema migration logic (lines 454-547) that's generic SQL, not SQLite-specific.

**Solution**: Move column addition logic to SqlStorageAdapterBase.

**Files to Change**:

1. **SqlStorageAdapterBase.ts** - Add method:
```typescript
/**
 * Migrate table schema by adding missing columns
 * SQLite/Postgres/MySQL all support ALTER TABLE ADD COLUMN
 */
protected async migrateTableSchema(
  tableName: string,
  entityClass: EntityConstructor,
  fieldNameMapper: (field: string) => string
): Promise<{ added: string[]; upToDate: boolean }> {
  // Get existing columns
  const existingColumns = await this.getTableColumns(tableName);

  // Get expected columns from entity metadata
  const fieldMetadata = getFieldMetadata(entityClass);
  const missingColumns: string[] = [];

  for (const [fieldName, metadata] of fieldMetadata.entries()) {
    const columnName = fieldNameMapper(fieldName);

    if (!existingColumns.has(columnName)) {
      missingColumns.push(columnName);

      // Generate ALTER TABLE statement
      const sqlType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);
      const nullable = metadata.options?.nullable !== false;
      const defaultValue = metadata.options?.default;

      let alterSql = `ALTER TABLE ${tableName} ADD COLUMN ${columnName} ${sqlType}`;

      if (!nullable) {
        // For NOT NULL columns on existing tables, must provide default
        if (defaultValue !== undefined) {
          alterSql += ` DEFAULT ${this.formatDefaultValue(defaultValue, sqlType)}`;
        } else {
          alterSql += ` DEFAULT ${this.getDefaultForType(sqlType)}`;
        }
        alterSql += ' NOT NULL';
      }

      console.log(`   🔄 Adding column: ${columnName} (${sqlType})`);
      await this.executeRawStatement(alterSql);
    }
  }

  return {
    added: missingColumns,
    upToDate: missingColumns.length === 0
  };
}

/**
 * Get existing columns for a table (dialect-specific, override in subclass)
 */
protected abstract getTableColumns(tableName: string): Promise<Set<string>>;

/**
 * Format default value for SQL
 */
private formatDefaultValue(value: unknown, sqlType: string): string {
  if (value === null) return 'NULL';
  if (typeof value === 'string') return `'${value.replace(/'/g, "''")}'`;
  if (typeof value === 'number') return String(value);
  if (typeof value === 'boolean') return value ? '1' : '0';
  if (sqlType === 'TEXT') return `'${JSON.stringify(value).replace(/'/g, "''")}'`;
  return 'NULL';
}

/**
 * Get sensible default value for SQL type
 */
private getDefaultForType(sqlType: string): string {
  switch (sqlType) {
    case 'INTEGER':
    case 'REAL':
      return '0';
    case 'TEXT':
      return "''";
    default:
      return 'NULL';
  }
}
```

2. **SqliteStorageAdapter.ts** - Keep SQLite-specific `getTableColumns()`, DELETE migration logic:
```typescript
// BEFORE (94 lines of migration logic)
private async migrateTableSchema(
  collectionName: string,
  tableName: string,
  entityClass: EntityConstructor
): Promise<void> {
  // ... 94 lines ...
}

// AFTER (2 lines - delegate to base)
private async migrateTableSchemaWrapper(
  collectionName: string,
  tableName: string,
  entityClass: EntityConstructor
): Promise<void> {
  const result = await this.migrateTableSchema(tableName, entityClass, SqlNamingConverter.toSnakeCase);
  if (result.added.length > 0) {
    console.log(`✅ Migrated ${tableName}: added ${result.added.length} new columns`);
  } else {
    console.log(`✅ Schema up-to-date: ${tableName}`);
  }
}
```

**Lines Saved**: ~200 lines

---

## Phase 2B: Use SqliteQueryBuilder Properly (Eliminate Duplication)

### 2B.1 - Replace buildEntitySelectQuery with SqliteQueryBuilder

**Problem**: SqliteStorageAdapter.buildEntitySelectQuery() (lines 1106-1250) duplicates SqliteQueryBuilder.buildSelect().

**Solution**: DELETE method, use SqliteQueryBuilder directly.

**Files to Change**:

1. **SqliteStorageAdapter.ts** - DELETE lines 1106-1250:
```typescript
// BEFORE (144 lines)
private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
  const params: any[] = [];
  const tableName = SqlNamingConverter.toTableName(query.collection);
  let sql = `SELECT * FROM ${tableName}`;
  // ... 140 lines of query building ...
  return { sql, params };
}

// AFTER (2 lines)
private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
  const tableName = SqlNamingConverter.toTableName(query.collection);
  const { sql, params } = SqliteQueryBuilder.buildSelect(query, tableName);
  return { sql, params };
}
```

2. **SqliteStorageAdapter.ts** - Update queryFromEntityTable() to use new method:
```typescript
// Line 1013 - No changes needed, already uses buildEntitySelectQuery()
private async queryFromEntityTable<T extends RecordData>(
  query: StorageQuery,
  entityClass: EntityConstructor
): Promise<StorageResult<DataRecord<T>[]>> {
  const { sql, params } = this.buildEntitySelectQuery(query, entityClass);
  // ... rest stays the same ...
}
```

**Lines Saved**: ~150 lines

---

## Phase 2C: Extract Composition Utilities

### 2C.1 - EntityTableManager (~300 lines saved)

**Problem**: SqliteStorageAdapter has scattered entity table operations:
- `createSimpleEntityTable()` (lines 775-798)
- `createInSimpleEntityTable()` (lines 803-824)
- `createLegacyBlob()` (lines 829-856)
- `migrateSimpleEntityTable()` (lines 507-547)

**Solution**: Extract to dedicated utility class.

**New File**: `daemons/data-daemon/server/EntityTableManager.ts`
```typescript
/**
 * EntityTableManager - Handles simple entity table operations
 *
 * Used for collections without registered entity metadata.
 * Creates tables with basic structure + JSON data column.
 */

import { SqliteRawExecutor } from './SqliteRawExecutor';
import { SqlNamingConverter } from '../shared/SqlNamingConverter';
import type { DataRecord, RecordData } from '../shared/DataStorageAdapter';

export class EntityTableManager {
  constructor(private executor: SqliteRawExecutor) {}

  /**
   * Create simple entity table (id, data, created_at, updated_at, version)
   */
  async createSimpleTable(collectionName: string): Promise<void> {
    const tableName = SqlNamingConverter.toTableName(collectionName);

    console.log(`🏗️ Creating simple entity table: ${collectionName} -> ${tableName}`);

    const sql = `
      CREATE TABLE IF NOT EXISTS ${tableName} (
        id TEXT PRIMARY KEY,
        data TEXT NOT NULL,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
        version INTEGER DEFAULT 1
      )
    `;

    await this.executor.runSql(sql);

    // Create basic indexes
    await this.executor.runSql(`CREATE INDEX IF NOT EXISTS idx_${tableName}_created_at ON ${tableName}(created_at)`);
    await this.executor.runSql(`CREATE INDEX IF NOT EXISTS idx_${tableName}_updated_at ON ${tableName}(updated_at)`);

    console.log(`✅ Simple entity table created: ${tableName}`);
  }

  /**
   * Insert record into simple entity table
   */
  async insertIntoSimpleTable<T extends RecordData>(record: DataRecord<T>): Promise<DataRecord<T>> {
    const tableName = SqlNamingConverter.toTableName(record.collection);

    const sql = `
      INSERT OR REPLACE INTO ${tableName} (
        id, data, created_at, updated_at, version
      ) VALUES (?, ?, ?, ?, ?)
    `;

    const params = [
      record.id,
      JSON.stringify(record.data),
      record.metadata.createdAt,
      record.metadata.updatedAt,
      record.metadata.version
    ];

    await this.executor.runStatement(sql, params);
    console.log(`✅ Inserted into simple entity table ${tableName}`);

    return record;
  }

  /**
   * Migrate simple entity table by ensuring required columns exist
   */
  async migrateSimpleTable(tableName: string): Promise<string[]> {
    // Get existing columns
    const result = await this.executor.runSql(`PRAGMA table_info(${tableName})`);
    const existingColumns = new Set(result.map((row: { name: string }) => row.name));

    // Required columns for simple entity table
    const requiredColumns = [
      { name: 'id', type: 'TEXT', nullable: false },
      { name: 'data', type: 'TEXT', nullable: false },
      { name: 'created_at', type: 'TEXT', nullable: true },
      { name: 'updated_at', type: 'TEXT', nullable: true },
      { name: 'version', type: 'INTEGER', nullable: true }
    ];

    const missingColumns: string[] = [];

    for (const column of requiredColumns) {
      if (!existingColumns.has(column.name)) {
        missingColumns.push(column.name);

        let alterSql = `ALTER TABLE ${tableName} ADD COLUMN ${column.name} ${column.type}`;

        if (!column.nullable) {
          alterSql += ` DEFAULT ${this.getDefaultForType(column.type)} NOT NULL`;
        } else {
          alterSql += ' DEFAULT NULL';
        }

        console.log(`   🔄 Adding missing column: ${column.name} (${column.type})`);
        await this.executor.runSql(alterSql);
      }
    }

    return missingColumns;
  }

  private getDefaultForType(sqlType: string): string {
    switch (sqlType) {
      case 'INTEGER':
        return '0';
      case 'TEXT':
        return "''";
      default:
        return 'NULL';
    }
  }
}
```

**SqliteStorageAdapter.ts Changes**:
```typescript
// Add to constructor
private entityTableManager!: EntityTableManager;

// In initialize()
this.entityTableManager = new EntityTableManager(this.executor);

// BEFORE (24 lines)
private async createSimpleEntityTable(collectionName: string): Promise<void> {
  // ... 24 lines ...
}

// AFTER (1 line)
private async createSimpleEntityTable(collectionName: string): Promise<void> {
  await this.entityTableManager.createSimpleTable(collectionName);
}

// Similar for other methods...
```

**Lines Saved**: ~300 lines

---

### 2C.2 - CollectionStatsManager (~100 lines saved)

**Problem**: Collection statistics logic scattered across SqliteStorageAdapter.

**Solution**: Extract to dedicated utility class (LOW PRIORITY - stats are currently no-op).

**New File**: `daemons/data-daemon/server/CollectionStatsManager.ts`
```typescript
/**
 * CollectionStatsManager - Handles collection statistics tracking
 *
 * Currently a no-op for entity tables (stats queried on-demand).
 * Kept for backwards compatibility with legacy _collections table.
 */

import { SqliteRawExecutor } from './SqliteRawExecutor';
import type { CollectionStats } from '../shared/DataStorageAdapter';

export class CollectionStatsManager {
  constructor(private executor: SqliteRawExecutor) {}

  /**
   * Get collection statistics from entity table
   */
  async getStats(collection: string, tableName: string): Promise<CollectionStats | undefined> {
    // Count records directly from entity table
    const countSql = `SELECT COUNT(*) as count FROM ${tableName}`;
    const countRows = await this.executor.runSql(countSql);
    const recordCount = countRows[0]?.count || 0;

    // Get table info
    const infoSql = `SELECT sql FROM sqlite_master WHERE type='table' AND name = ?`;
    const infoRows = await this.executor.runSql(infoSql, [tableName]);

    if (infoRows.length === 0) {
      return undefined;
    }

    return {
      name: collection,
      recordCount: recordCount,
      totalSize: 0,
      lastModified: new Date().toISOString(),
      schema: 'v1'
    };
  }

  /**
   * Update collection statistics (no-op for entity tables)
   */
  async updateStats(collection: string): Promise<void> {
    // No-op: Entity tables don't need statistics tracking in separate table
    // Stats can be queried directly from entity tables when needed
  }

  /**
   * Ensure collection exists in legacy registry (backwards compatibility)
   */
  async ensureCollection(collection: string): Promise<void> {
    const sql = `
      INSERT OR IGNORE INTO _collections (name, created_at, updated_at)
      VALUES (?, ?, ?)
    `;

    const now = new Date().toISOString();
    await this.executor.runStatement(sql, [collection, now, now]);
  }
}
```

**Lines Saved**: ~100 lines

---

### 2C.3 - Fix Batch Operations to Use SqliteTransactionManager

**Problem**: `batch()` method (lines 1669-1768) has manual transaction management with FIXME comments.

**Solution**: Refactor to use existing SqliteTransactionManager.

**SqliteStorageAdapter.ts Changes**:
```typescript
// BEFORE (100 lines with manual BEGIN/COMMIT/ROLLBACK)
async batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
  if (!this.db) {
    return { success: false, error: 'Database not initialized' };
  }

  return new Promise((resolve) => {
    this.db!.serialize(() => {
      // FIXME(Phase2): Manual transaction management
      if (!this.transactionManager.isInTransaction()) {
        this.db!.run('BEGIN TRANSACTION');
      }
      // ... 80 lines of manual transaction handling ...
    });
  });
}

// AFTER (30 lines using withTransaction)
async batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
  if (!this.db) {
    return { success: false, error: 'Database not initialized' };
  }

  try {
    const results = await this.transactionManager.withTransaction(async () => {
      const opResults: any[] = [];

      for (const op of operations) {
        switch (op.type) {
          case 'create':
            if (op.data && op.collection) {
              const record: DataRecord<T> = {
                id: op.id || `batch_${Date.now()}_${Math.random()}`,
                collection: op.collection,
                data: op.data as T,
                metadata: {
                  createdAt: new Date().toISOString(),
                  updatedAt: new Date().toISOString(),
                  version: 1
                }
              };
              opResults.push(await this.create(record));
            }
            break;

          case 'read':
            if (op.collection && op.id) {
              opResults.push(await this.read(op.collection, op.id));
            }
            break;

          case 'update':
            if (op.collection && op.id && op.data) {
              opResults.push(await this.update(op.collection, op.id, op.data as Partial<T>));
            }
            break;

          case 'delete':
            if (op.collection && op.id) {
              opResults.push(await this.delete(op.collection, op.id));
            }
            break;
        }
      }

      return opResults;
    });

    return { success: true, data: results };
  } catch (error: any) {
    return { success: false, error: error.message };
  }
}
```

**Lines Saved**: ~70 lines

---

## Summary of Line Count Reductions

| Phase | Description | Lines Saved | New Location |
|-------|-------------|-------------|--------------|
| **2A.1** | Filter Translation | ~150 | SqlStorageAdapterBase |
| **2A.2** | Result Mapping | ~200 | SqlStorageAdapterBase |
| **2A.3** | Schema Migration | ~200 | SqlStorageAdapterBase |
| **2B.1** | Use SqliteQueryBuilder | ~150 | Already exists |
| **2C.1** | EntityTableManager | ~300 | New utility class |
| **2C.2** | CollectionStatsManager | ~100 | New utility class |
| **2C.3** | Fix Batch Operations | ~70 | Use existing TransactionManager |
| **Total** | | **~1170 lines** | |

**Result**: SqliteStorageAdapter: 2277 → ~1107 lines (51% reduction)

---

## Implementation Order

### Step 1: Phase 2B (Quick Win)
Start with SqliteQueryBuilder integration - easiest and most impactful.
- Delete buildEntitySelectQuery duplication
- ~150 lines saved immediately
- Low risk (SqliteQueryBuilder already tested)

### Step 2: Phase 2C.3 (Fix Batch)
Fix batch operations to use SqliteTransactionManager.
- Remove FIXME comments
- ~70 lines saved
- Medium risk (transaction logic)

### Step 3: Phase 2C.1 (EntityTableManager)
Extract entity table operations to utility class.
- ~300 lines saved
- Low risk (pure extraction)

### Step 4: Phase 2A.2 (Result Mapping)
Move result mapping to base class.
- ~200 lines saved
- Medium risk (affects all read operations)

### Step 5: Phase 2A.1 (Filter Translation)
Move filter translation to base class.
- ~150 lines saved
- Medium risk (affects all query operations)

### Step 6: Phase 2A.3 (Schema Migration)
Move schema migration to base class.
- ~200 lines saved
- Low risk (only runs on startup)

### Step 7: Phase 2C.2 (CollectionStatsManager)
Extract collection statistics (optional - currently no-op).
- ~100 lines saved
- Low risk (low priority)

---

## Testing Strategy

### After Each Phase:
1. **TypeScript Compilation**: `npm run build:ts`
2. **CRUD Tests**: `npm run test:crud`
3. **Integration Tests**: Run full test suite
4. **Manual Testing**: `./jtag data/list --collection=users`

### Phase-Specific Tests:

**Phase 2B** (SqliteQueryBuilder):
- Test query with filters: `data/list --filter='{"active":true}'`
- Test query with operators: `data/list --filter='{"age":{"$gt":18}}'`
- Test query with sorting: `data/list --orderBy='[{"field":"createdAt","direction":"desc"}]'`

**Phase 2C.3** (Batch Operations):
- Test batch create/update/delete
- Verify transaction rollback on error

**Phase 2C.1** (EntityTableManager):
- Test creating unregistered collection
- Test migrating simple entity table

**Phase 2A.2** (Result Mapping):
- Test reading entities with various field types
- Test reading simple entities (JSON data column)

**Phase 2A.1** (Filter Translation):
- Test all operators: $eq, $ne, $gt, $gte, $lt, $lte, $in, $nin, $exists, $regex, $contains
- Test combined filters

**Phase 2A.3** (Schema Migration):
- Add new field to entity class
- Restart server, verify column added automatically

---

## Post-Refactoring Benefits

### For SqliteStorageAdapter:
- Reduced from 2277 → ~1107 lines (51% smaller)
- Focuses on SQLite-specific logic only
- Easier to understand and maintain
- Less duplication

### For SqlStorageAdapterBase:
- Now contains ~550 lines of shared SQL logic
- Ready for PostgreSQL/MySQL adapters to extend
- Filter translation, result mapping, schema migration shared

### For Future SQL Adapters:
PostgreSQL/MySQL adapters will be much simpler:
```typescript
export class PostgresStorageAdapter extends SqlStorageAdapterBase {
  // Only need ~500 lines of Postgres-specific code
  // All generic SQL logic inherited from base
}
```

---

## Architecture After Refactoring

```
SqlStorageAdapterBase (550 lines)
├── Type mapping (existing)
├── Schema generation (existing)
├── Filter translation (new)
├── Result mapping (new)
├── Schema migration (new)
└── Query building helpers (new)
    ↑ extends
SqliteStorageAdapter (1107 lines)
├── SQLite connection management
├── SQLite-specific SQL syntax
├── Vector search implementation
├── Core CRUD orchestration
└── Delegates to:
    ├── SqliteRawExecutor (68 lines)
    ├── SqliteTransactionManager (70 lines)
    ├── SqliteQueryBuilder (238 lines)
    ├── SqlNamingConverter (36 lines)
    ├── EntityTableManager (150 lines)
    └── CollectionStatsManager (100 lines)
```

**Total System Lines**: 2319 lines (vs 2277 in monolith)
**But**: Properly organized, reusable, and extensible

---

## Success Criteria

✅ SqliteStorageAdapter reduced to ~1100 lines (from 2277)
✅ All tests pass (CRUD + integration)
✅ No behavior changes (pure refactoring)
✅ TypeScript compilation succeeds
✅ Code coverage maintained
✅ Ready for PostgreSQL adapter implementation

---

## Notes

- **Backwards Compatibility**: All refactoring maintains existing behavior
- **No Breaking Changes**: Public API unchanged
- **Incremental**: Each phase can be committed separately
- **Testable**: Full test coverage at each step
- **Reversible**: Each phase can be reverted if needed

---

**Status**: Ready for implementation
**Start With**: Phase 2B (SqliteQueryBuilder integration)
**Estimated Total Time**: 1-2 days for all phases
