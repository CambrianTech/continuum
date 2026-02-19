# Multi-Database Handle System - Implementation Status

**Status**: Phase 1 In Progress
**Last Updated**: 2025-11-08

---

## ✅ Completed

### 1. Core Architecture (DONE)
- ✅ `DatabaseHandleRegistry.ts` - Registry for managing multiple storage adapters
- ✅ Documentation: `MULTI-DATABASE-HANDLES.md`
- ✅ Documentation: `MULTI-DATABASE-SECURITY.md` (Phase 2+ design)

### 2. Data Commands - Partial (1/3 Done)
- ✅ `commands/data/open/shared/DataOpenTypes.ts` - Type definitions for data/open command

---

## 🚧 In Progress

### 3. Data Commands - Remaining Files
**Next Steps**:

#### `commands/data/open/server/DataOpenServerCommand.ts`
```typescript
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataOpenParams, DataOpenResult } from '../shared/DataOpenTypes';
import { createDataOpenResultFromParams } from '../shared/DataOpenTypes';

export class DataOpenServerCommand {
  private registry = DatabaseHandleRegistry.getInstance();

  async execute(params: DataOpenParams): Promise<DataOpenResult> {
    try {
      const handle = await this.registry.open(params.adapter, params.config);

      return createDataOpenResultFromParams(params, {
        success: true,
        dbHandle: handle,
        adapter: params.adapter
      });
    } catch (error) {
      return createDataOpenResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
```

#### `commands/data/open/browser/DataOpenBrowserCommand.ts`
```typescript
// Browser commands forward to server via CommandDaemon
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataOpenParams, DataOpenResult } from '../shared/DataOpenTypes';

export class DataOpenBrowserCommand {
  async execute(params: DataOpenParams): Promise<DataOpenResult> {
    return await Commands.execute<DataOpenResult>('data/open', params);
  }
}
```

---

## 📋 TODO

### 4. Data Close Command (Priority: High)
Create complete command structure:
- `commands/data/close/shared/DataCloseTypes.ts`
- `commands/data/close/server/DataCloseServerCommand.ts`
- `commands/data/close/browser/DataCloseBrowserCommand.ts`

**Types**:
```typescript
export interface DataCloseParams extends JTAGPayload {
  readonly dbHandle: DbHandle;
}

export interface DataCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly dbHandle: DbHandle;
  readonly timestamp: string;
  readonly error?: string;
}
```

### 5. Data List-Handles Command (Priority: High)
Create complete command structure:
- `commands/data/list-handles/shared/DataListHandlesTypes.ts`
- `commands/data/list-handles/server/DataListHandlesServerCommand.ts`
- `commands/data/list-handles/browser/DataListHandlesBrowserCommand.ts`

**Types**:
```typescript
export interface DataListHandlesParams extends JTAGPayload {
  // No additional parameters needed
}

export interface DataListHandlesResult extends JTAGPayload {
  readonly success: boolean;
  readonly handles: readonly {
    handle: DbHandle;
    adapter: AdapterType;
    config: AdapterConfig;
    isDefault: boolean;
    openedAt: number;
    lastUsedAt: number;
  }[];
  readonly timestamp: string;
  readonly error?: string;
}
```

### 6. Update Existing Data Commands (Priority: Critical)
Add optional `dbHandle?: DbHandle` parameter to ALL data/* command types:

**Files to modify**:
- `commands/data/create/shared/DataCreateTypes.ts` - Add `dbHandle?` to `DataCreateParams`
- `commands/data/read/shared/DataReadTypes.ts` - Add `dbHandle?` to `DataReadParams`
- `commands/data/list/shared/DataListTypes.ts` - Add `dbHandle?` to `DataListParams`
- `commands/data/update/shared/DataUpdateTypes.ts` - Add `dbHandle?` to `DataUpdateParams`
- `commands/data/delete/shared/DataDeleteTypes.ts` - Add `dbHandle?` to `DataDeleteParams`
- `commands/data/query-open/shared/DataQueryOpenTypes.ts` - Add `dbHandle?` to params
- `commands/data/query-next/shared/DataQueryNextTypes.ts` - Add `dbHandle?` to params
- `commands/data/query-close/shared/DataQueryCloseTypes.ts` - Add `dbHandle?` to params
- `commands/data/schema/shared/DataSchemaTypes.ts` - Add `dbHandle?` to params
- `commands/data/truncate/shared/DataTruncateTypes.ts` - Add `dbHandle?` to params
- `commands/data/clear/shared/DataClearTypes.ts` - Add `dbHandle?` to params

**Pattern**:
```typescript
// Add this to every DataXxxParams interface:
readonly dbHandle?: DbHandle;  // Optional database handle (defaults to 'default')
```

### 7. Update DataDaemon (Priority: Critical)
**File**: `daemons/data-daemon/server/DataDaemon.ts`

**Changes needed**:
1. Import `DatabaseHandleRegistry` and `DbHandle` type
2. Get singleton instance in constructor
3. Update every method to accept optional `dbHandle` parameter
4. Route calls through `registry.getAdapter(dbHandle)` instead of hardcoded adapter

**Example**:
```typescript
import { DatabaseHandleRegistry, type DbHandle } from '../server/DatabaseHandleRegistry';

export class DataDaemon {
  private handleRegistry: DatabaseHandleRegistry;

  constructor() {
    this.handleRegistry = DatabaseHandleRegistry.getInstance();
  }

  async create<T extends BaseEntity>(params: {
    dbHandle?: DbHandle;
    collection: string;
    data: T;
    id?: UUID;
  }): Promise<DataCreateResult<T>> {
    const adapter = this.handleRegistry.getAdapter(params.dbHandle);
    return await adapter.create({
      collection: params.collection,
      data: params.data,
      id: params.id
    });
  }

  // ... repeat for all methods: read, list, update, delete, etc.
}
```

### 8. Testing (Priority: High)
Create integration tests:

**File**: `tests/integration/multi-database-handles.test.ts`

**Test scenarios**:
```typescript
describe('Multi-Database Handles', () => {
  it('should open SQLite database and return handle', async () => {
    const result = await Commands.execute('data/open', {
      adapter: 'sqlite',
      config: { path: '/tmp/test-training.sqlite', mode: 'create' }
    });

    expect(result.success).toBe(true);
    expect(result.dbHandle).toBeDefined();
  });

  it('should use default handle when omitted', async () => {
    const users = await Commands.execute('data/list', {
      collection: 'users'
    });
    expect(users.success).toBe(true);
  });

  it('should create records in training database', async () => {
    const { dbHandle } = await Commands.execute('data/open', {
      adapter: 'sqlite',
      config: { path: '/tmp/test-training.sqlite', mode: 'create' }
    });

    const result = await Commands.execute('data/create', {
      dbHandle,
      collection: 'training_examples',
      data: { text: 'example', label: 'positive' },
      id: 'test-001'
    });

    expect(result.success).toBe(true);
  });

  it('should list open handles', async () => {
    const result = await Commands.execute('data/list-handles', {});
    expect(result.handles.length).toBeGreaterThan(0);
    expect(result.handles[0].isDefault).toBe(true);
  });

  it('should close handle', async () => {
    const { dbHandle } = await Commands.execute('data/open', {
      adapter: 'sqlite',
      config: { path: '/tmp/test.sqlite', mode: 'create' }
    });

    const result = await Commands.execute('data/close', { dbHandle });
    expect(result.success).toBe(true);
  });
});
```

### 9. JSONL to SQLite Import (Priority: Medium)
**Goal**: Convert continuum-git JSONL dataset to SQLite for fine-tuning

**Script**: `scripts/import-jsonl-to-sqlite.ts`

**Steps**:
1. Open training database via `data/open`
2. Read JSONL file line by line
3. Create records via `data/create` with training dbHandle
4. Report progress (1590 records)

**Usage**:
```bash
npx tsx scripts/import-jsonl-to-sqlite.ts \
  --input=/datasets/parsed/continuum-git-2025-11-08T00-31-01.jsonl \
  --output=/datasets/prepared/continuum-git.sqlite
```

---

## 📖 Architecture Reference

**Key Principles**:
1. **Backward Compatible**: No `dbHandle` = uses 'default' handle
2. **Storage-Adapter-Agnostic**: Handles work with ANY storage backend
3. **Single Source of Truth**: `DATABASE_PATHS.SQLITE` remains default
4. **Type-Safe**: Full TypeScript inference across all commands

**Files**:
- Architecture: `docs/MULTI-DATABASE-HANDLES.md`
- Security (Phase 2): `docs/MULTI-DATABASE-SECURITY.md`
- Registry: `daemons/data-daemon/server/DatabaseHandleRegistry.ts`

---

## 🎯 Next Session Priorities

1. **Complete data/open command** (server + browser files)
2. **Create data/close command** (complete)
3. **Create data/list-handles command** (complete)
4. **Update all data/* command types** (add `dbHandle?` parameter)
5. **Update DataDaemon** (route through handle registry)
6. **Run `npm start`** and test with JTAG commands
7. **Create integration tests**
8. **Import JSONL to SQLite** for fine-tuning pipeline

---

## 💡 Testing Commands

Once implemented, test with:

```bash
# Open training database
./jtag data/open --adapter=sqlite --config='{"path":"/tmp/training.sqlite","mode":"create"}'

# List open handles
./jtag data/list-handles

# Create record in training DB (using returned handle)
./jtag data/create --dbHandle="<UUID>" --collection="training_examples" \
  --data='{"text":"example","label":"positive"}' --id="test-001"

# List from default database (backward compatible)
./jtag data/list --collection="users"

# Close handle
./jtag data/close --dbHandle="<UUID>"
```

---

## ⚠️ Critical Notes

1. **Don't break existing code**: All existing `data/*` calls must work without modification
2. **DEFAULT_HANDLE is sacred**: Always initialized, cannot be closed
3. **Type safety**: Import actual types, never use `any`
4. **Follow ARCHITECTURE-RULES.md**: Entity-agnostic in data/event layers
5. **Run `npm start` after changes**: 90+ seconds, mandatory

---

**Ready for continued implementation in next session.**
