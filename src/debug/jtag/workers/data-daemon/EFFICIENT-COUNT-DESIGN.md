# Efficient COUNT(*) Design

## Problem

**File**: `daemons/data-daemon/shared/DataDaemon.ts:625-630`

```typescript
const countResult = await this.adapter.query(countQuery);
const totalCount = countResult.success ? (countResult.data?.length ?? 0) : 0;
```

**Impact**:
- Fetches ALL 15,284 rows from chat_messages just to count them
- TypeScript: 2.5 seconds for count
- Rust: Times out completely
- Result: Unacceptable performance for pagination

## Root Cause

`DataStorageAdapter` interface has NO `count()` method, so callers must:
1. Call `query()` with no LIMIT
2. Fetch ALL matching rows
3. Count array length

**This is fundamentally inefficient for SQL databases** - should use `SELECT COUNT(*)`.

## Solution: Add count() Method to Adapter Interface

### 1. Update TypeScript Interface

**File**: `daemons/data-daemon/shared/DataStorageAdapter.ts`

Add new abstract method after `query()`:

```typescript
/**
 * Count records matching query without fetching data
 *
 * Efficient alternative to query() when only totalCount is needed.
 * SQL adapters use COUNT(*), NoSQL adapters use count operations.
 */
abstract count(query: StorageQuery): Promise<StorageResult<number>>;
```

### 2. Implement in TypeScript SqliteStorageAdapter

**File**: `daemons/data-daemon/server/SqliteStorageAdapter.ts`

```typescript
async count(query: StorageQuery): Promise<StorageResult<number>> {
  const db = this.getDatabase();

  // Build WHERE clause from filter (reuse existing filter logic)
  const { whereClause, params } = this.buildWhereClause(query.filter);

  // COUNT(*) query - NO SELECT *, NO ORDER BY, NO LIMIT
  const sql = `SELECT COUNT(*) as count FROM ${query.collection}${whereClause}`;

  try {
    const result = db.prepare(sql).get(...params);
    return {
      success: true,
      data: result.count
    };
  } catch (error) {
    return {
      success: false,
      error: `Count failed: ${error.message}`
    };
  }
}
```

**Key points**:
- NO `SELECT *` - just COUNT(*)
- NO `ORDER BY` - irrelevant for counting
- NO `LIMIT` - want total count
- Uses same WHERE clause building logic as query()

### 3. Implement in Rust StorageAdapter Trait

**File**: `workers/data-daemon/src/storage/adapter.rs`

```rust
/// Count records matching query without fetching data
async fn count(&self, query: Value) -> Result<i64, Box<dyn Error>>;
```

**File**: `workers/data-daemon/src/storage/sqlite.rs`

```rust
async fn count(&self, query: Value) -> Result<i64, Box<dyn Error>> {
    let collection = query.get("collection")
        .and_then(|c| c.as_str())
        .ok_or("Missing collection in query")?
        .to_string();

    // Build SQL query - COUNT(*) ONLY
    let mut sql = format!("SELECT COUNT(*) FROM {}", collection);

    // Build params from filter (reuse existing filter extraction logic)
    let mut filter_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(filter) = query.get("filter").and_then(|f| f.as_object()) {
        let conditions: Vec<String> = filter.iter()
            .map(|(field, value)| {
                match value {
                    Value::String(s) => filter_params.push(Box::new(s.clone())),
                    Value::Number(n) if n.is_i64() => filter_params.push(Box::new(n.as_i64().unwrap())),
                    Value::Number(n) if n.is_f64() => filter_params.push(Box::new(n.as_f64().unwrap())),
                    Value::Bool(b) => filter_params.push(Box::new(*b as i64)),
                    Value::Null => filter_params.push(Box::new(rusqlite::types::Null)),
                    _ => {}
                }
                format!("{} = ?", to_snake_case(field))
            })
            .collect();
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
    }

    // Execute count query
    let conn_guard = self.connection.lock().unwrap();
    let conn = conn_guard.as_ref().ok_or("Not initialized")?;

    let param_refs: Vec<&dyn rusqlite::ToSql> = filter_params.iter()
        .map(|p| &**p as &dyn rusqlite::ToSql)
        .collect();

    let count: i64 = conn.query_row(&sql, &param_refs[..], |row| row.get(0))?;

    Ok(count)
}
```

**Key points**:
- Returns `i64` directly (not Vec of records)
- Reuses filter param extraction from query() method
- NO result set iteration - single query_row() call

### 4. Update DataDaemon to Use count()

**File**: `daemons/data-daemon/shared/DataDaemon.ts:625-630`

**Before** (slow):
```typescript
const countQuery: StorageQuery = {
  collection: params.collection,
  filter: params.filter
};
const countResult = await this.adapter.query(countQuery);
const totalCount = countResult.success ? (countResult.data?.length ?? 0) : 0;
```

**After** (fast):
```typescript
const countQuery: StorageQuery = {
  collection: params.collection,
  filter: params.filter
};
const countResult = await this.adapter.count(countQuery);
const totalCount = countResult.success ? (countResult.data ?? 0) : 0;
```

## Performance Expectations

### Before (using query())
- TypeScript: 2.5 seconds (parses 15k JSON rows)
- Rust: Timeout (>10 seconds trying to parse 15k rows)

### After (using count())
- TypeScript: <50ms (COUNT(*) on indexed column)
- Rust: <10ms (COUNT(*) on indexed column)

**Improvement**: ~50-500x faster for large collections

## Testing Strategy

### Unit Test (Rust)

**File**: `workers/data-daemon/tests/count_test.rs`

```rust
#[tokio::test]
async fn test_count_with_filter() {
    let mut adapter = SqliteAdapter::new();
    adapter.initialize(json!({"filename": ":memory:"})).await.unwrap();

    // Seed 100 messages: 50 in room A, 50 in room B
    for i in 0..50 {
        adapter.create("chat_messages", json!({
            "id": format!("msg-a-{}", i),
            "roomId": "room-a",
            "content": format!("Message {}", i)
        })).await.unwrap();
    }
    for i in 0..50 {
        adapter.create("chat_messages", json!({
            "id": format!("msg-b-{}", i),
            "roomId": "room-b",
            "content": format!("Message {}", i)
        })).await.unwrap();
    }

    // Count room A messages
    let count = adapter.count(json!({
        "collection": "chat_messages",
        "filter": {"roomId": "room-a"}
    })).await.unwrap();

    assert_eq!(count, 50);
}
```

### Integration Test (Production Data)

```bash
# With TypeScript adapter
DATA_DAEMON_TYPE=sqlite npm start

# Count chat_messages (should be fast)
time ./jtag data/count --collection=chat_messages
# Expected: <50ms, returns 15284

# Count with filter
time ./jtag data/count --collection=chat_messages \
  --filter='{"roomId":"5e71a0c8-0303-4eb8-a478-3a121248"}'
# Expected: <50ms, returns actual count for that room
```

### Performance Test (Cursor Open)

```bash
# With Rust adapter (after implementing count())
DATA_DAEMON_TYPE=rust npm start

# Open paginated query (uses count internally)
time ./jtag data/query-open --collection=chat_messages \
  --filter='{"roomId":"5e71a0c8-0303-4eb8-a478-3a121248"}' \
  --pageSize=30

# Expected:
# - totalCount: returned instantly (<50ms)
# - queryHandle: UUID for pagination
# - First page: 30 messages, <100ms total
```

## Implementation Order

1. ✅ **Document the problem** (this file)
2. ✅ Add `count()` to DataStorageAdapter interface (TypeScript)
3. ✅ Implement `count()` in SqliteStorageAdapter (TypeScript)
4. ✅ Test TypeScript implementation with production data
5. ✅ Add `count()` to StorageAdapter trait (Rust)
6. ✅ Implement `count()` in SqliteAdapter (Rust)
7. ⏳ Test Rust implementation with production data (ready for testing)
8. ✅ Update DataDaemon.openPaginatedQuery() to use count()
9. ⏳ Verify cursor commands work fast with both backends
10. ⏳ Add data/count CLI command for manual testing

## Conclusion

**Problem**: No count() method forces inefficient "fetch all + count array" pattern

**Solution**: Add count() method that executes optimized COUNT(*) queries

**Impact**: 50-500x performance improvement for pagination and totalCount queries

**Architecture**: Clean adapter pattern - each backend implements count() efficiently for its query language (SQL COUNT, MongoDB count(), etc.)
