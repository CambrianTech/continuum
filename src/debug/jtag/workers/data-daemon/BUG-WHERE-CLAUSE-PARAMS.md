# Bug: WHERE Clause Parameters Not Passed to query_map()

## The Bug

**File**: `src/storage/sqlite.rs`, line 197

```rust
let rows: Result<Vec<_>, _> = stmt.query_map([], |row| {
                                            ^^
                                         EMPTY!
```

**Problem**:
1. SQL built correctly with WHERE clause placeholders: `WHERE room_id = ?`
2. But `query_map()` receives **empty params array** `[]`
3. The `?` placeholders have no values
4. SQLite ignores WHERE clause or errors
5. **Returns ALL rows instead of filtered subset**

## Impact

**Query**: Get 30 messages from room "general"
- Expected: 30 messages from that room
- Actual: ALL messages from database (could be 15,000+)
- **Result**: Timeout parsing 15k JSON objects when only needed 30

## The Fix

**Extract filter values and pass as params**:

```rust
// Build params from filter
let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

if let Some(filter) = query.get("filter").and_then(|f| f.as_object()) {
    for (field, value) in filter {
        match value {
            Value::String(s) => params.push(Box::new(s.clone())),
            Value::Number(n) if n.is_i64() => params.push(Box::new(n.as_i64().unwrap())),
            Value::Number(n) if n.is_f64() => params.push(Box::new(n.as_f64().unwrap())),
            Value::Bool(b) => params.push(Box::new(*b as i64)),
            _ => {}
        }
    }
}

let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter()
    .map(|p| &**p as &dyn rusqlite::ToSql)
    .collect();

// Pass params to query_map
let rows: Result<Vec<_>, _> = stmt.query_map(&param_refs[..], |row| {
    // ... build JSON
})?;
```

## Test Plan

### Before Fix (Bug Exists)

```bash
# Start system
npm start

# Query chat messages for a room
./jtag data/list --collection=chat_messages \
  --filter='{"roomId":"7ceb8b93-55d2-4af3-aeba-07a179b3"}' \
  --limit=30

# Expected: Timeout or returns messages from ALL rooms
# Actual: Bug confirmed
```

### After Fix (Bug Resolved)

```bash
# Rebuild Rust worker
cd workers/data-daemon
cargo build --release

# Restart workers
./workers/start-workers.sh

# Set to use Rust backend
echo "DATA_DAEMON_TYPE=rust" > ~/.continuum/config.env

# Restart system
npm start

# Query again
./jtag data/list --collection=chat_messages \
  --filter='{"roomId":"7ceb8b93-55d2-4af3-aeba-07a179b3"}' \
  --limit=30

# Expected: Returns exactly 30 messages from that room, instantly
# Performance: <100ms
```

## Root Cause Analysis

**Why did this happen?**
1. Code builds WHERE clause SQL correctly (line 150-158)
2. Code adds LIMIT/OFFSET correctly (line 176-185)
3. But forgot to extract filter VALUES and pass them as parameters
4. The placeholder `?` was left unbound

**Classic SQL injection vulnerability pattern** - except here it just breaks functionality instead of security.

## Prevention

**Add integration test that verifies filter works**:

```rust
#[tokio::test]
async fn test_filter_actually_filters() {
    // Seed 100 messages: 50 in room A, 50 in room B
    // Query for room A with limit 30
    // Assert: Returns 30 messages, ALL from room A
    // Assert: Query takes <100ms
}
```

This test would have caught the bug immediately.
