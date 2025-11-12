# SQL Storage Adapter Refactoring Plan

## Current State: Architectural Problem

**SqliteStorageAdapter.ts: 2,188 lines**

### The Core Issue

We have THREE SQL adapters planned (SQLite, PostgreSQL, MySQL) that would share 90% of their code:
- Schema generation (CREATE TABLE, indexes, migrations)
- Query building (SELECT, JOIN, WHERE clauses)
- Transaction management (BEGIN, COMMIT, ROLLBACK)
- Filter/operator conversion
- Batch operations

**Current architecture**: DataStorageAdapter (abstract base) → SqliteStorageAdapter (concrete)

**Problem**: When we add PostgreSQL, we'd copy-paste 1,800+ lines and change 10% (driver calls).

## Target Architecture: Three-Tier Hierarchy

```
DataStorageAdapter (abstract - 262 lines)
├── SqlStorageAdapterBase (abstract - ~800 lines, SQL-specific)
│   ├── SqliteStorageAdapter (concrete - ~350 lines, SQLite driver)
│   ├── PostgresStorageAdapter (concrete - ~350 lines, Postgres driver)
│   └── MySqlStorageAdapter (concrete - ~350 lines, MySQL driver)
├── JsonFileStorageAdapter (concrete - ~400 lines)
└── MemoryStorageAdapter (concrete - ~400 lines)
```

### Key Insight

When 3 adapters share 90% of code, you need an **intermediate base class** with SQL-specific logic.

---

## SqlStorageAdapterBase: What Goes In It

### 1. Schema Generation (~200 lines)

**Methods to extract:**
```typescript
protected abstract getSqlDialect(): 'sqlite' | 'postgres' | 'mysql';

protected mapFieldTypeToSql(fieldType: FieldType, options?: FieldMetadata['options']): string {
  const dialect = this.getSqlDialect();
  // Base implementation with dialect-specific overrides
}

protected generateCreateTableSql(collectionName: string, entityClass: EntityConstructor): string {
  // Uses mapFieldTypeToSql internally
}

protected generateCreateIndexSql(collectionName: string, entityClass: EntityConstructor): string[] {
  // Index generation logic
}

protected formatDefaultValue(value: unknown, sqlType: string): string
protected getDefaultForType(sqlType: string): string
```

**Why base class?**
- 95% identical across SQLite/Postgres/MySQL
- Only differences: `AUTOINCREMENT` vs `SERIAL`, JSON syntax, TEXT vs VARCHAR

### 2. Schema Management (~200 lines)

```typescript
protected async ensureEntityTable(collectionName: string): Promise<void> {
  // Create table if not exists, migrate schema if needed
}

protected async tableExists(tableName: string): Promise<boolean> {
  // Query information_schema or sqlite_master
}

protected async getTableColumns(tableName: string): Promise<Set<string>> {
  // Get current table structure
}

protected async migrateTableSchema(
  collectionName: string,
  existingColumns: Set<string>,
  entityClass: EntityConstructor
): Promise<void> {
  // Add missing columns, preserve data
}
```

**Why base class?**
- Schema introspection patterns identical
- Only differences: metadata table names (`sqlite_master` vs `information_schema`)

### 3. Query Building (~250 lines)

```typescript
protected buildEntitySelectQuery(
  query: StorageQuery,
  entityClass: EntityConstructor
): { sql: string; params: any[] } {
  // Build SELECT with JOINs, WHERE, ORDER BY, LIMIT
}

protected buildJsonQuery(query: StorageQuery): { sql: string; params: any[] } {
  // JSON field extraction
}

protected sanitizeJsonPath(fieldPath: string): string

protected buildWhereClause(filters: any): { clause: string; params: any[] }
protected buildOrderByClause(orderBy: any[]): string
protected buildLimitOffsetClause(limit?: number, offset?: number): string
```

**Why base class?**
- Query structure identical across SQL databases
- Only differences: JSON syntax (`json_extract` vs `->` vs `->>`)

### 4. Transaction Management (~50 lines)

```typescript
protected abstract async executeSQL(sql: string, params: any[]): Promise<any[]>
protected abstract async executeStatement(sql: string, params: any[]): Promise<{ lastID?: number; changes: number }>

protected async beginTransaction(): Promise<void> {
  await this.executeStatement('BEGIN TRANSACTION', []);
}

protected async commitTransaction(): Promise<void> {
  await this.executeStatement('COMMIT', []);
}

protected async rollbackTransaction(): Promise<void> {
  await this.executeStatement('ROLLBACK', []);
}
```

**Why base class?**
- Transaction SQL is identical
- Concrete adapters implement `executeSQL` / `executeStatement`

### 5. Collection Management (~100 lines)

```typescript
protected async ensureCollection(collection: string): Promise<void>
protected async updateCollectionStats(collection: string): Promise<void>
protected async createCoreSchema(): Promise<void>
```

**Why base class?**
- Collection tracking logic identical
- Uses abstract `executeSQL` internally

---

## Concrete SQL Adapters: What Stays Driver-Specific

### SqliteStorageAdapter (~350 lines)

```typescript
export class SqliteStorageAdapter extends SqlStorageAdapterBase {
  private db: sqlite3.Database | null = null;

  protected getSqlDialect(): 'sqlite' { return 'sqlite'; }

  async initialize(config: StorageAdapterConfig): Promise<void> {
    // 1. Create sqlite3.Database connection
    // 2. Configure PRAGMA settings (WAL, foreign keys, etc.)
    // 3. Call super.createCoreSchema()
  }

  protected async executeSQL(sql: string, params: any[]): Promise<any[]> {
    return new Promise((resolve, reject) => {
      this.db!.all(sql, params, (err, rows) => {
        if (err) reject(err);
        else resolve(rows);
      });
    });
  }

  protected async executeStatement(sql: string, params: any[]): Promise<{ lastID?: number; changes: number }> {
    return new Promise((resolve, reject) => {
      this.db!.run(sql, params, function(err) {
        if (err) reject(err);
        else resolve({ lastID: this.lastID, changes: this.changes });
      });
    });
  }

  async close(): Promise<void> {
    // Close sqlite3 connection
  }

  // SQLite-specific overrides (if needed)
  protected mapFieldTypeToSql(fieldType: FieldType, options?: any): string {
    const base = super.mapFieldTypeToSql(fieldType, options);
    // Override for SQLite quirks (e.g., AUTOINCREMENT)
    return base;
  }
}
```

**Key Properties:**
- Only 350 lines (down from 2,188)
- Focuses ONLY on SQLite driver specifics
- Inherits all SQL logic from base class

### PostgresStorageAdapter (~350 lines) - NEW

```typescript
import { Pool, PoolClient } from 'pg';

export class PostgresStorageAdapter extends SqlStorageAdapterBase {
  private pool: Pool | null = null;

  protected getSqlDialect(): 'postgres' { return 'postgres'; }

  async initialize(config: StorageAdapterConfig): Promise<void> {
    // 1. Create pg.Pool connection
    this.pool = new Pool({
      host: config.host || 'localhost',
      port: config.port || 5432,
      database: config.database || 'continuum',
      user: config.user || 'postgres',
      password: config.password,
      max: 20, // connection pool size
    });

    // 2. Test connection
    const client = await this.pool.connect();
    client.release();

    // 3. Create core schema
    await this.createCoreSchema();
  }

  protected async executeSQL(sql: string, params: any[]): Promise<any[]> {
    const client = await this.pool!.connect();
    try {
      const result = await client.query(sql, params);
      return result.rows;
    } finally {
      client.release();
    }
  }

  protected async executeStatement(sql: string, params: any[]): Promise<{ lastID?: number; changes: number }> {
    const client = await this.pool!.connect();
    try {
      const result = await client.query(sql, params);
      return {
        lastID: result.rows[0]?.id, // Postgres uses RETURNING clause
        changes: result.rowCount || 0
      };
    } finally {
      client.release();
    }
  }

  async close(): Promise<void> {
    await this.pool?.end();
  }

  // Postgres-specific overrides
  protected mapFieldTypeToSql(fieldType: FieldType, options?: any): string {
    // SERIAL instead of AUTOINCREMENT
    // JSONB instead of JSON
    // VARCHAR(n) limits
    const dialect = this.getSqlDialect();

    switch (fieldType) {
      case 'uuid':
        return 'UUID';
      case 'text':
        return 'TEXT';
      case 'integer':
        return options?.autoIncrement ? 'SERIAL' : 'INTEGER';
      case 'json':
        return 'JSONB'; // Postgres uses binary JSON
      case 'timestamp':
        return 'TIMESTAMP WITH TIME ZONE';
      default:
        return super.mapFieldTypeToSql(fieldType, options);
    }
  }

  // Postgres-specific JSON query syntax
  protected buildJsonQuery(query: StorageQuery): { sql: string; params: any[] } {
    // Use Postgres -> and ->> operators
    // Example: data->>'field' instead of json_extract(data, '$.field')
  }
}
```

**Key Properties:**
- Only 350 lines
- Adds PostgreSQL to the system with minimal code
- Reuses 90% of SqliteStorageAdapter logic

---

## Migration Strategy: Test-Driven Extraction

### Phase 1: Create SqlStorageAdapterBase (2 hours)

1. **Create new file**: `daemons/data-daemon/server/SqlStorageAdapterBase.ts`

2. **Extract abstract base class**:
   ```typescript
   export abstract class SqlStorageAdapterBase extends DataStorageAdapter {
     // Abstract methods concrete adapters must implement
     protected abstract getSqlDialect(): 'sqlite' | 'postgres' | 'mysql';
     protected abstract executeSQL(sql: string, params: any[]): Promise<any[]>;
     protected abstract executeStatement(sql: string, params: any[]): Promise<{ lastID?: number; changes: number }>;

     // Shared SQL logic (800 lines)
     protected mapFieldTypeToSql(...) { /* extracted */ }
     protected generateCreateTableSql(...) { /* extracted */ }
     protected buildEntitySelectQuery(...) { /* extracted */ }
     protected beginTransaction() { /* extracted */ }
     // ... etc
   }
   ```

3. **Test**: Compile, ensure no type errors

**Commit**: "feat: add SqlStorageAdapterBase with SQL-generic logic"

### Phase 2: Refactor SqliteStorageAdapter (1 hour)

1. **Change inheritance**:
   ```typescript
   export class SqliteStorageAdapter extends SqlStorageAdapterBase {
   ```

2. **Delete extracted methods** (1,800+ lines → 350 lines)

3. **Implement abstract methods**:
   ```typescript
   protected getSqlDialect(): 'sqlite' { return 'sqlite'; }
   protected async executeSQL(...) { /* sqlite3 driver */ }
   protected async executeStatement(...) { /* sqlite3 driver */ }
   ```

4. **Keep SQLite-specific**:
   - `initialize()` - connection setup
   - `configureSqlite()` - PRAGMA statements
   - `close()` - cleanup
   - Optional overrides for SQLite quirks

**Test Strategy**:
```bash
npm start  # Full deployment
npm test -- --grep="database"  # Database integration tests
./jtag data/list --collection=users  # Smoke test
```

**Expected Result**: All tests pass, zero behavior change

**Commit**: "refactor: SqliteStorageAdapter extends SqlStorageAdapterBase (2188→350 lines)"

### Phase 3: Add PostgresStorageAdapter (2 hours)

1. **Install pg driver**:
   ```bash
   npm install pg @types/pg
   ```

2. **Create new file**: `daemons/data-daemon/server/PostgresStorageAdapter.ts`

3. **Implement PostgresStorageAdapter** (~350 lines)
   - Copy SqliteStorageAdapter structure
   - Replace sqlite3 with pg driver
   - Override dialect-specific methods

4. **Update StorageAdapterFactory**:
   ```typescript
   case 'postgres':
     return new PostgresStorageAdapter();
   ```

5. **Add Postgres to DatabaseConfig**:
   ```typescript
   export const POSTGRES_CONFIG = {
     host: process.env.POSTGRES_HOST || 'localhost',
     port: parseInt(process.env.POSTGRES_PORT || '5432'),
     database: process.env.POSTGRES_DB || 'continuum',
     user: process.env.POSTGRES_USER || 'postgres',
     password: process.env.POSTGRES_PASSWORD
   };
   ```

**Test Strategy**:
```bash
# Start local Postgres
docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=test postgres:15

# Test with Postgres
STORAGE_BACKEND=postgres POSTGRES_PASSWORD=test npm start
./jtag data/list --collection=users
```

**Expected Result**: System works identically with PostgreSQL backend

**Commit**: "feat: add PostgresStorageAdapter using SqlStorageAdapterBase (350 lines)"

---

## Testing Checklist

### Unit Tests (Isolated)

```typescript
// tests/unit/sql-storage-adapter-base.test.ts
describe('SqlStorageAdapterBase', () => {
  describe('mapFieldTypeToSql', () => {
    it('SQLite: uses AUTOINCREMENT');
    it('Postgres: uses SERIAL');
    it('MySQL: uses AUTO_INCREMENT');
  });

  describe('generateCreateTableSql', () => {
    it('generates valid SQLite schema');
    it('generates valid Postgres schema');
  });

  describe('buildEntitySelectQuery', () => {
    it('handles simple filters');
    it('handles JOINs');
    it('handles JSON queries');
  });
});
```

### Integration Tests (Real Database)

```bash
npm test -- --grep="SqliteStorageAdapter"  # Existing tests
npm test -- --grep="PostgresStorageAdapter"  # New tests
npm test -- tests/integration/database/  # Full suite
```

### Smoke Tests (Manual)

```bash
# SQLite (existing)
npm start
./jtag data/list --collection=users

# Postgres (new)
STORAGE_BACKEND=postgres npm start
./jtag data/list --collection=users

# Verify identical behavior
diff <(./jtag data/read --collection=users --id=joel-id) \
     <(STORAGE_BACKEND=postgres ./jtag data/read --collection=users --id=joel-id)
```

---

## Benefits After Refactor

### Code Reduction

- **Before**: SqliteStorageAdapter = 2,188 lines
- **After**:
  - SqlStorageAdapterBase = 800 lines (shared)
  - SqliteStorageAdapter = 350 lines (driver)
  - PostgresStorageAdapter = 350 lines (driver)
  - **Total**: 1,500 lines (31% reduction)

### Extensibility

Adding MySQL adapter becomes **trivial**:
1. Copy PostgresStorageAdapter.ts
2. Replace `pg` with `mysql2`
3. Override 3-4 dialect methods
4. **Done in 1 hour**

### Maintainability

- Bug fixes in SQL logic: **Change once in base class**
- Schema generation improvements: **Benefits all SQL databases**
- Query optimization: **Shared across adapters**

### Type Safety

```typescript
// Before: Any SQL adapter, no type checking
const adapter = new SqliteStorageAdapter();

// After: Base class enforces contract
abstract class SqlStorageAdapterBase {
  protected abstract executeSQL(sql: string, params: any[]): Promise<any[]>;
  // Concrete adapters MUST implement this
}
```

---

## Edge Cases & Considerations

### 1. SQL Dialect Differences

**Problem**: JSON syntax varies across databases
- SQLite: `json_extract(data, '$.field')`
- Postgres: `data->>'field'`
- MySQL: `JSON_EXTRACT(data, '$.field')`

**Solution**: Override `buildJsonQuery()` in concrete adapters

### 2. Transaction Isolation

**Problem**: Postgres defaults to READ COMMITTED, SQLite to SERIALIZABLE

**Solution**: Document transaction behavior, allow configuration:
```typescript
protected async beginTransaction(isolationLevel?: string): Promise<void> {
  await this.executeStatement(
    `BEGIN TRANSACTION ${isolationLevel || ''}`,
    []
  );
}
```

### 3. Connection Pooling

**Problem**: SQLite uses single connection, Postgres uses pool

**Solution**: Abstract into base class:
```typescript
protected abstract getConnection(): Promise<Connection>;
protected abstract releaseConnection(conn: Connection): void;
```

### 4. Schema Migration Safety

**Problem**: Production migrations must be non-destructive

**Solution**:
- Never DROP columns (only ADD)
- Preserve existing data
- Log all schema changes
- Dry-run mode for migrations

---

## Follow-Up Refactors

### 1. Extract SqlQueryBuilder

Currently buried in SqliteStorageAdapter. Should be separate:

```
daemons/data-daemon/server/
├── SqlStorageAdapterBase.ts
├── SqlQueryBuilder.ts (new - query building logic)
├── SqliteStorageAdapter.ts
└── PostgresStorageAdapter.ts
```

### 2. Extract Schema Management

```
daemons/data-daemon/server/
├── SqlStorageAdapterBase.ts
├── SqlSchemaManager.ts (new - schema generation/migration)
├── SqliteStorageAdapter.ts
└── PostgresStorageAdapter.ts
```

### 3. Add MySQL Support

Once PostgreSQL is proven, MySQL becomes a copy-paste job.

---

## Timeline Estimate

| Phase | Task | Estimated Time |
|-------|------|----------------|
| 1 | Create SqlStorageAdapterBase | 2 hours |
| 2 | Refactor SqliteStorageAdapter | 1 hour |
| 3 | Test SQLite refactor | 30 minutes |
| 4 | Add PostgresStorageAdapter | 2 hours |
| 5 | Test Postgres integration | 1 hour |
| 6 | Documentation updates | 30 minutes |
| **Total** | | **7 hours** |

---

## Success Criteria

✅ All existing tests pass with SqliteStorageAdapter
✅ SqliteStorageAdapter reduces from 2,188 → 350 lines
✅ PostgresStorageAdapter works identically to SQLite
✅ System deploys successfully with both backends
✅ No performance regression
✅ Future MySQL adapter estimated at <2 hours

---

## Risk Mitigation

### Risk: Breaking Existing Functionality

**Mitigation**:
- Extract base class FIRST (no behavior change)
- Refactor SqliteStorageAdapter SECOND (verify tests pass)
- Add Postgres THIRD (isolated, doesn't affect SQLite)

### Risk: Subtle SQL Dialect Bugs

**Mitigation**:
- Comprehensive integration tests
- Side-by-side comparison (SQLite vs Postgres)
- Gradual rollout (dev → staging → production)

### Risk: Transaction Behavior Differences

**Mitigation**:
- Document transaction semantics per database
- Add transaction tests to integration suite
- Make isolation level configurable

---

**Bottom Line**: This refactor eliminates 700 lines of duplication, makes PostgreSQL trivial to add, and sets foundation for MySQL support. All in 7 hours of focused work.
