# Decorator-Driven Schema Architecture

**Status**: Design Decision - Implementation Required
**Version**: 1.0
**Date**: 2025-12-03

---

## Executive Summary

Field decorators are the **single source of truth** for all storage requirements. They define:
- Database schema (indexes, types, constraints)
- Query optimization (which fields to SELECT in lists)
- API behavior (what data gets returned by default)

This creates a clean intermediate representation between entity definitions and storage implementations (JSON files today, SQLite/Postgres tomorrow).

---

## The Problem

Without decorator-driven schema:
- **Verbose by default**: Every `data/list` call returns ALL fields, wasting bandwidth
- **High token usage**: AIs get huge entity payloads when they only need id + name
- **No query optimization**: Can't build covering indexes because we don't know which fields matter
- **Schema drift**: SQL schema defined separately from entity, leading to mismatches

---

## The Solution: `summary` Field Decorator

Add a `summary: boolean` option to field decorators that marks fields for inclusion in non-verbose list queries.

### Decorator Options

```typescript
export interface FieldMetadata {
  fieldName: string;
  fieldType: FieldType;
  options?: {
    // Storage
    index?: boolean;           // Create database index
    unique?: boolean;          // Unique constraint
    nullable?: boolean;        // Allow NULL
    maxLength?: number;        // Field size limit

    // Query behavior
    summary?: boolean;         // Include in list queries (default: false)
    description?: boolean;     // The "toString" field (only one per entity)

    // Relationships
    references?: string;       // Foreign key target
  };
}
```

---

## Example: WallDocumentEntity

```typescript
export class WallDocumentEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.WALL_DOCUMENTS;

  // ALWAYS INCLUDED (from BaseEntity): id, createdAt, updatedAt

  // Summary fields - included in list queries
  @TextField({ index: true, summary: true })
  roomId!: UUID;

  @TextField({
    index: true,
    summary: true,
    description: true,        // This is the entity's "toString"
    maxLength: 256
  })
  name!: string;               // e.g., "governance-framework.md"

  @TextField({ index: true, summary: true })
  createdBy!: UUID;

  @TextField({ index: true, summary: true })
  lastModifiedBy!: UUID;

  @DateField({ index: true, summary: true })
  lastModifiedAt!: Date;

  @TextField({ summary: true })
  lineCount!: number;          // Quick stats

  @TextField({ summary: true })
  byteCount!: number;

  // Detail fields - NOT in summary (requires explicit read)
  @TextField()
  filePath!: string;           // Only needed for actual file operations

  @TextField({ nullable: true })
  lastCommitHash?: string;     // Git metadata - rarely needed

  @TextField({ nullable: true })
  currentLeaseId?: UUID;       // Lease system - detail only

  @TextField({ nullable: true })
  protectionLevel?: string;    // Governance - detail only
}
```

---

## Query Behavior

### Default List Query (Summary Mode)

```typescript
// Request
await Commands.execute<DataListParams, DataListResult>('data/list', {
  collection: 'wall_documents',
  filter: { roomId: 'abc-123' }
});

// Response - ONLY summary fields + BaseEntity fields
{
  success: true,
  records: [
    {
      id: "doc-001",
      createdAt: "2025-12-03T10:00:00Z",
      updatedAt: "2025-12-03T14:30:00Z",
      data: {
        roomId: "abc-123",
        name: "governance-framework.md",  // description field
        createdBy: "user-001",
        lastModifiedBy: "user-003",
        lastModifiedAt: "2025-12-03T14:30:00Z",
        lineCount: 342,
        byteCount: 15678
        // filePath: NOT INCLUDED
        // lastCommitHash: NOT INCLUDED
        // currentLeaseId: NOT INCLUDED
      }
    }
  ]
}
```

**Token savings**: ~60% reduction for entities with many detail fields

### Explicit Full Read

```typescript
// Request
await Commands.execute<DataReadParams, DataReadResult>('data/read', {
  collection: 'wall_documents',
  id: 'doc-001'
});

// Response - ALL fields
{
  success: true,
  record: {
    id: "doc-001",
    data: {
      roomId: "abc-123",
      name: "governance-framework.md",
      createdBy: "user-001",
      lastModifiedBy: "user-003",
      lastModifiedAt: "2025-12-03T14:30:00Z",
      lineCount: 342,
      byteCount: 15678,
      filePath: "/path/to/file.md",           // NOW INCLUDED
      lastCommitHash: "a3f9c21",              // NOW INCLUDED
      currentLeaseId: "lease-042",            // NOW INCLUDED
      protectionLevel: "peer_review"          // NOW INCLUDED
    }
  }
}
```

---

## SQL Database Optimization

When SQLite/Postgres adapters are implemented, they use decorator metadata to generate optimal schemas:

### Schema Generation

```typescript
// Reads decorator metadata
const metadata = getFieldMetadata(WallDocumentEntity);

// Generates SQL schema
CREATE TABLE wall_documents (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  -- Summary fields (indexed for fast queries)
  room_id TEXT NOT NULL,
  name VARCHAR(256) NOT NULL,
  created_by TEXT NOT NULL,
  last_modified_by TEXT NOT NULL,
  last_modified_at TEXT NOT NULL,
  line_count INTEGER NOT NULL,
  byte_count INTEGER NOT NULL,

  -- Detail fields (no indexes)
  file_path TEXT NOT NULL,
  last_commit_hash TEXT,
  current_lease_id TEXT,
  protection_level TEXT
);

-- Indexes for summary fields
CREATE INDEX idx_wall_documents_room_id ON wall_documents(room_id);
CREATE INDEX idx_wall_documents_name ON wall_documents(name);
CREATE INDEX idx_wall_documents_created_by ON wall_documents(created_by);
CREATE INDEX idx_wall_documents_last_modified_by ON wall_documents(last_modified_by);
CREATE INDEX idx_wall_documents_last_modified_at ON wall_documents(last_modified_at);

-- Covering index for list queries (only summary fields)
CREATE INDEX idx_wall_documents_summary
  ON wall_documents(room_id, last_modified_at, name, created_by, last_modified_by, line_count, byte_count);
```

### Optimized List Query

```sql
-- data/list automatically uses covering index
SELECT
  id, created_at, updated_at,
  room_id, name, created_by, last_modified_by, last_modified_at, line_count, byte_count
FROM wall_documents
WHERE room_id = 'abc-123'
ORDER BY last_modified_at DESC
LIMIT 50;
```

**Performance**: Index-only scan, no table access needed

---

## Implementation Strategy

### Phase 1: Add `summary` Decorator Option

1. **Update FieldDecorators.ts**
   ```typescript
   export interface FieldMetadata {
     options?: {
       summary?: boolean;  // NEW
       // ... existing options
     };
   }
   ```

2. **Add Helper Function**
   ```typescript
   /**
    * Get summary field names for an entity
    */
   export function getSummaryFields(entityClass: EntityConstructor): string[] {
     const metadata = getFieldMetadata(entityClass);
     const summaryFields: string[] = [];

     // Always include BaseEntity fields
     summaryFields.push('id', 'createdAt', 'updatedAt');

     // Add fields marked with summary: true
     for (const [fieldName, fieldMetadata] of metadata) {
       if (fieldMetadata.options?.summary === true) {
         summaryFields.push(fieldName);
       }
     }

     return summaryFields;
   }
   ```

### Phase 2: Update Entities

Add `summary: true` to all existing entities for fields that should appear in lists:

```typescript
// Example: ChatMessageEntity
@TextField({ index: true, summary: true })
roomId!: UUID;

@TextField({ index: true, summary: true })
senderId!: UUID;

@TextField({ summary: true, description: true, maxLength: TEXT_LENGTH.SHORT })
contentPreview!: string;  // First 30 chars for lists

@JsonField()  // NOT summary - full content only on explicit read
content!: MessageContent;
```

### Phase 3: Implement Field Projection in DataDaemon

```typescript
// In DataDaemon list handler
async list(params: DataListParams): Promise<DataListResult> {
  const entityClass = this.getEntityClass(params.collection);
  const summaryFields = getSummaryFields(entityClass);

  // Query storage (JSON files for now)
  const records = await this.storage.list(params);

  // Project only summary fields
  const projectedRecords = records.map(record => ({
    ...record,
    data: this.projectFields(record.data, summaryFields)
  }));

  return {
    success: true,
    records: projectedRecords,
    totalCount: records.length,
    hasMore: false
  };
}

private projectFields(data: any, fields: string[]): any {
  const projected: any = {};
  for (const field of fields) {
    if (field in data) {
      projected[field] = data[field];
    }
  }
  return projected;
}
```

### Phase 4: Add Explicit Full Projection (Optional)

```typescript
export interface DataListParams extends JTAGPayload {
  readonly collection: string;
  readonly filter?: Partial<BaseEntity>;
  readonly fields?: string[] | '*';  // NEW: explicit field selection
  readonly limit?: number;
  readonly offset?: number;
}

// Usage
await Commands.execute('data/list', {
  collection: 'wall_documents',
  filter: { roomId: 'abc-123' },
  fields: '*'  // Explicitly request all fields
});

// Or specific fields
await Commands.execute('data/list', {
  collection: 'wall_documents',
  filter: { roomId: 'abc-123' },
  fields: ['id', 'name', 'filePath']  // Custom projection
});
```

---

## Benefits

### 1. Bandwidth & Token Efficiency
- **60-80% reduction** in payload size for list queries
- Lower API costs for LLM token usage
- Faster network transmission

### 2. Query Performance
- Covering indexes can serve list queries entirely from index
- No need to access table data
- Scales to millions of records

### 3. Single Source of Truth
- Decorators define everything: types, indexes, query behavior
- No schema drift between entity and database
- Storage adapters read metadata, don't invent schema

### 4. Future-Proof
- Works with JSON file storage today
- Optimizes SQLite/Postgres tomorrow
- Same decorator metadata drives both

### 5. Developer Experience
- Entity author decides what's important for lists
- No need to remember to pass `fields` parameter
- Sensible defaults (summary only)

---

## Migration Path

### Backward Compatibility

During transition, default to returning ALL fields (current behavior):

```typescript
// In DataDaemon
const summaryFields = getSummaryFields(entityClass);

if (summaryFields.length === 0) {
  // Entity hasn't been updated with summary fields yet
  // Return all fields (backward compatible)
  return records;
} else {
  // Entity has summary fields defined
  // Return only summary fields
  return projectRecords(records, summaryFields);
}
```

### Incremental Rollout

1. **Week 1**: Add `summary` option to decorators, no behavior change
2. **Week 2**: Update high-traffic entities (ChatMessage, User, Room)
3. **Week 3**: Enable projection in DataDaemon
4. **Week 4**: Update remaining entities
5. **Week 5**: Monitor token usage and query performance

---

## Design Decisions

### Why `summary: true` Instead of `summary: false`?

**Decision**: Require explicit opt-in for summary fields

**Rationale**:
- Most fields are detail-only (default: not in summary)
- Forces entity author to think about what's important
- Avoids accidental inclusion of sensitive/large fields
- Better for token usage (minimalist default)

### Why Not Rely on `index: true`?

**Decision**: Separate `index` and `summary` concerns

**Rationale**:
- You might index a field for queries but not show it in lists
- You might show a non-indexed field in lists (e.g., lineCount)
- Mixing storage optimization with API behavior is confusing

### Why Keep `description: boolean` Separate?

**Decision**: `description` is special - only ONE per entity

**Rationale**:
- Every entity needs a "toString" representation
- Used for references, logs, debugging
- Should be brief (maxLength: TEXT_LENGTH.SHORT)
- Automatically included in summary (implied)

---

## Examples from Existing Entities

### UserEntity

```typescript
@TextField({ index: true, summary: true, description: true, maxLength: 30 })
displayName!: string;  // Brief name for lists

@TextField({ index: true, summary: true })
role!: string;  // Important for access control lists

@TextField({ summary: true })
status!: 'online' | 'offline' | 'away';  // Current state

@TextField()  // Detail only
email?: string;

@JsonField()  // Detail only
preferences?: UIPreferences;
```

### ChatMessageEntity

```typescript
@TextField({ index: true, summary: true })
roomId!: UUID;

@TextField({ index: true, summary: true })
senderId!: UUID;

@DateField({ index: true, summary: true })
timestamp!: Date;

@TextField({ summary: true, description: true, maxLength: TEXT_LENGTH.SHORT })
preview!: string;  // First 30 chars

@JsonField()  // Detail only - full content requires explicit read
content!: MessageContent;
```

### PinnedItemEntity

```typescript
@TextField({ index: true, summary: true })
roomId!: UUID;

@TextField({ summary: true, description: true })
title!: string;

@TextField({ summary: true })
pinType!: PinType;

@TextField({ summary: true, nullable: true })
category?: PinCategory;

@TextField({ nullable: true })  // Detail only
longDescription?: string;

@JsonField({ nullable: true })  // Detail only
bullets?: string[];
```

---

## Testing Strategy

### Unit Tests

```typescript
describe('Decorator-Driven Schema', () => {
  test('getSummaryFields returns correct fields', () => {
    const fields = getSummaryFields(WallDocumentEntity);

    expect(fields).toContain('id');
    expect(fields).toContain('roomId');
    expect(fields).toContain('name');
    expect(fields).not.toContain('filePath');
    expect(fields).not.toContain('lastCommitHash');
  });

  test('projectFields filters correctly', () => {
    const full = {
      id: '001',
      name: 'test.md',
      filePath: '/path/to/file',
      lineCount: 100
    };

    const projected = projectFields(full, ['id', 'name', 'lineCount']);

    expect(projected).toEqual({
      id: '001',
      name: 'test.md',
      lineCount: 100
    });
    expect(projected.filePath).toBeUndefined();
  });
});
```

### Integration Tests

```typescript
describe('data/list with field projection', () => {
  test('returns only summary fields by default', async () => {
    const result = await Commands.execute('data/list', {
      collection: 'wall_documents'
    });

    expect(result.success).toBe(true);
    expect(result.records[0].data.name).toBeDefined();
    expect(result.records[0].data.filePath).toBeUndefined();
  });

  test('returns all fields with explicit projection', async () => {
    const result = await Commands.execute('data/list', {
      collection: 'wall_documents',
      fields: '*'
    });

    expect(result.success).toBe(true);
    expect(result.records[0].data.name).toBeDefined();
    expect(result.records[0].data.filePath).toBeDefined();
  });
});
```

### Performance Tests

```bash
# Measure token usage before/after
./jtag data/list --collection=chat_messages --limit=100 | wc -c

# Before: ~250KB
# After:  ~85KB (66% reduction)
```

---

## References

- [FieldDecorators.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/system/data/decorators/FieldDecorators.ts) - Decorator implementation
- [ARCHITECTURE-RULES.md](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/docs/ARCHITECTURE-RULES.md) - Entity system rules
- [DataTypes.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/daemons/data-daemon/shared/DataTypes.ts) - Data command types

---

**End of Decorator-Driven Schema Architecture**
