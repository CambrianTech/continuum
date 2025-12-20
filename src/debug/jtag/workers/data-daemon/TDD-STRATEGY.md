# TDD Strategy for Rust Data Daemon Adapter

## The Problem

**Three days, no real progress.** Why?

- No incremental validation of components
- Breaking working code without knowing what broke
- No isolation testing - everything tested as one big system
- No performance benchmarks (15k records should be trivial with indexes)
- No comparison to TypeScript adapter output (the known-good reference)

## The Solution: Test-Driven Development

**Target**: Validate `StorageAdapter` trait compliance for **all adapters**
- SQLite is just the **first** adapter (Postgres, MySQL, Graph, Semantic DBs coming)
- Tests must validate **trait contract**, not SQLite specifics
- Same API (StorageAdapter trait = DataStorageAdapter abstract class)
- Same JSON output format (regardless of backend)
- Same query semantics (filter, sort, limit, offset)
- Same performance characteristics (<100ms for 15k indexed records)

**Key Insight**: Test the **interface contract**, then run same tests against SqliteAdapter, PostgresAdapter, MySQLAdapter, etc.

---

## Adapter-Agnostic Testing Strategy

### Trait Contract Tests (Reusable for All Adapters)

```rust
// tests/trait_contract.rs
// Generic tests that work for ANY StorageAdapter implementation

pub async fn test_adapter_contract<T: StorageAdapter>(adapter: &T, test_db_path: &str) {
    // These tests MUST pass for any adapter implementing StorageAdapter
    test_initialize(adapter, test_db_path).await;
    test_create(adapter).await;
    test_read(adapter).await;
    test_query_basic(adapter).await;
    test_query_filter(adapter).await;
    test_query_sort(adapter).await;
    test_query_pagination(adapter).await;
    test_list_collections(adapter).await;
    test_get_stats(adapter).await;
}
```

### Per-Adapter Test Suites

```rust
// tests/adapters/sqlite_adapter.rs
#[tokio::test]
async fn test_sqlite_adapter_contract() {
    let adapter = SqliteAdapter::new();
    test_adapter_contract(&adapter, "/tmp/test-sqlite.db").await;
}

// tests/adapters/postgres_adapter.rs (future)
#[tokio::test]
async fn test_postgres_adapter_contract() {
    let adapter = PostgresAdapter::new();
    test_adapter_contract(&adapter, "postgresql://test").await;
}

// tests/adapters/graph_adapter.rs (future)
#[tokio::test]
async fn test_graph_adapter_contract() {
    let adapter = GraphAdapter::new();
    test_adapter_contract(&adapter, "neo4j://test").await;
}
```

**Benefit**: Write tests once, validate every adapter. Ensures **uniform behavior** across all backends.

---

## Critical: Cursor/Pagination Architecture

**The Real Problem**: Suspected inefficient cursor causing timeouts on simple queries.

### What Should Happen (SQL Does the Work)

```sql
-- Count query (for totalCount metadata)
SELECT COUNT(*) FROM chat_messages WHERE room_id = ?;
-- Returns: 15000 (just number, instant)

-- Data query (for actual messages)
SELECT * FROM chat_messages
WHERE room_id = ?
ORDER BY created_at DESC
LIMIT 30 OFFSET 0;
-- Returns: 30 rows (instant with index on room_id)
```

### What's Probably Happening (Inefficient)

```rust
// BAD: Fetch ALL rows, then limit in Rust
let mut stmt = conn.prepare("SELECT * FROM chat_messages WHERE room_id = ?")?;
let rows = stmt.query_map([room_id], |row| { /* build JSON */ })?;

// WRONG: Collecting all 15k rows into memory
let all_records: Vec<_> = rows.collect()?;

// WRONG: Limiting AFTER fetching everything
let limited = all_records.into_iter().take(30).collect();
```

### What MUST Happen (Efficient)

```rust
// GOOD: SQL does the limiting
let sql = format!(
    "SELECT * FROM {} WHERE room_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
    collection
);

let mut stmt = conn.prepare(&sql)?;
let rows = stmt.query_map(params![room_id, limit, offset], |row| { /* build JSON */ })?;

// GOOD: Only iterates over 30 rows (what SQL returned)
let records: Vec<_> = rows.collect()?;  // Only 30 iterations
```

**Test This**: Verify LIMIT clause is in the SQL, not applied in Rust after fetching.

---

## Primary Use Case: chat_messages

**Entities:**
- **Room**: `{ id: UUID, name: 'general', ... }`
- **ChatMessage**: `{ id: UUID, roomId: UUID, content: string, createdAt: timestamp, ... }`

**Common Queries:**

1. **Get message count for room**:
   ```sql
   SELECT COUNT(*) FROM chat_messages WHERE room_id = ?;
   -- <10ms even with 15k messages
   ```

2. **Get latest 30 messages**:
   ```sql
   SELECT * FROM chat_messages
   WHERE room_id = ?
   ORDER BY created_at DESC
   LIMIT 30;
   -- <10ms with index on (room_id, created_at)
   ```

3. **Pagination**:
   ```sql
   SELECT * FROM chat_messages
   WHERE room_id = ?
   ORDER BY created_at DESC
   LIMIT 30 OFFSET 60;  -- Page 3
   -- <10ms with index
   ```

**Index Requirements**:
```sql
CREATE INDEX idx_chat_room_time ON chat_messages(room_id, created_at DESC);
```

**Performance Expectations**:
- Total count: <10ms (no matter how many messages)
- Page of 30: <10ms (no matter how many total messages)
- **No timeouts, ever**

This is the primary test case - if this doesn't work, nothing works.

---

## Core Components to Test in Isolation

### 1. Field Name Conversion (`to_snake_case`)

**Why Test First**: Foundation for all operations. If this is wrong, nothing works.

```rust
// tests/unit/field_conversion.rs
#[test]
fn test_camel_to_snake() {
    assert_eq!(to_snake_case("userId"), "user_id");
    assert_eq!(to_snake_case("createdAt"), "created_at");
    assert_eq!(to_snake_case("isActive"), "is_active");
    assert_eq!(to_snake_case("id"), "id");  // Already snake
}
```

**Pass Criteria**: All TypeScript field names convert correctly

---

### 2. Type Detection (`ValueRef` → JSON)

**Why Test First**: Query results depend on correct type mapping.

```rust
// tests/unit/type_detection.rs
#[test]
fn test_value_ref_to_json() {
    // Setup: Create test DB with known types
    let conn = Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE test (
        id TEXT,
        count INTEGER,
        price REAL,
        active INTEGER,
        data BLOB
    )", []).unwrap();

    conn.execute("INSERT INTO test VALUES (?, ?, ?, ?, ?)",
        params!["abc123", 42, 19.99, 1, b"binary"]).unwrap();

    // Test: Read row and validate JSON types
    let row = conn.query_row("SELECT * FROM test", [], |row| {
        // Extract and validate each type
        let id = row.get_ref(0)?;  // Should be String
        let count = row.get_ref(1)?;  // Should be i64
        let price = row.get_ref(2)?;  // Should be f64
        let active = row.get_ref(3)?;  // Should be i64 (not bool)
        let data = row.get_ref(4)?;  // Should be hex string

        // Assert correct JSON conversion
        assert!(matches!(id, ValueRef::Text(_)));
        assert!(matches!(count, ValueRef::Integer(_)));
        assert!(matches!(price, ValueRef::Real(_)));

        Ok(())
    }).unwrap();
}
```

**Pass Criteria**: All SQLite types map to correct JSON types

---

### 3. SqliteAdapter::initialize()

**Why Test First**: Must establish working connection before any operations.

```rust
// tests/integration/adapter_init.rs
#[tokio::test]
async fn test_adapter_initialize() {
    let mut adapter = SqliteAdapter::new();

    let config = serde_json::json!({
        "filename": "/tmp/test-init.sqlite"
    });

    // Test: Initialize succeeds
    let result = adapter.initialize(config).await;
    assert!(result.is_ok(), "Initialize failed: {:?}", result.err());

    // Test: Connection is usable
    let collections = adapter.list_collections().await;
    assert!(collections.is_ok(), "Connection not usable");

    // Cleanup
    adapter.close().await.unwrap();
    std::fs::remove_file("/tmp/test-init.sqlite").ok();
}

#[tokio::test]
async fn test_adapter_pragmas() {
    let mut adapter = SqliteAdapter::new();

    let config = serde_json::json!({
        "filename": "/tmp/test-pragmas.sqlite"
    });

    adapter.initialize(config).await.unwrap();

    // Test: Verify WAL mode set
    // TODO: Query PRAGMA journal_mode and verify = "wal"

    adapter.close().await.unwrap();
    std::fs::remove_file("/tmp/test-pragmas.sqlite").ok();
}
```

**Pass Criteria**:
- Connection established
- WAL mode enabled
- Pragmas set correctly

---

### 4. SqliteAdapter::create()

**Why Test First**: Must insert records before we can query them.

```rust
// tests/integration/adapter_create.rs
#[tokio::test]
async fn test_create_basic_record() {
    let mut adapter = setup_test_adapter().await;

    // Create test table (manually for now)
    create_users_table(&adapter).await;

    let record = serde_json::json!({
        "id": "user-001",
        "userId": "joel",
        "createdAt": "2025-01-01T00:00:00Z",
        "isActive": true
    });

    // Test: Create succeeds
    let result = adapter.create("users", record.clone()).await;
    assert!(result.is_ok(), "Create failed: {:?}", result.err());

    // Test: Record actually inserted (verify with raw SQL)
    let conn = get_connection(&adapter);
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE id = ?",
        ["user-001"],
        |row| row.get(0)
    ).unwrap();

    assert_eq!(count, 1, "Record not inserted");

    cleanup_test_adapter(adapter).await;
}

#[tokio::test]
async fn test_create_field_conversion() {
    let mut adapter = setup_test_adapter().await;
    create_users_table(&adapter).await;

    let record = serde_json::json!({
        "id": "user-002",
        "createdAt": "2025-01-01T00:00:00Z"  // camelCase
    });

    adapter.create("users", record).await.unwrap();

    // Test: Verify field stored as snake_case column
    let conn = get_connection(&adapter);
    let created_at: String = conn.query_row(
        "SELECT created_at FROM users WHERE id = ?",  // snake_case column
        ["user-002"],
        |row| row.get(0)
    ).unwrap();

    assert_eq!(created_at, "2025-01-01T00:00:00Z");

    cleanup_test_adapter(adapter).await;
}
```

**Pass Criteria**:
- Records inserted successfully
- camelCase fields → snake_case columns
- All JSON types handled (String, Number, Bool, Null)

---

### 5. SqliteAdapter::read()

**Why Test First**: Simplest query operation - read by ID.

```rust
// tests/integration/adapter_read.rs
#[tokio::test]
async fn test_read_existing_record() {
    let mut adapter = setup_test_adapter().await;
    create_and_seed_users(&adapter).await;  // Insert known test data

    // Test: Read existing record
    let result = adapter.read("users", "user-001").await;
    assert!(result.is_ok());

    let record = result.unwrap();
    assert!(record.is_some(), "Record not found");

    let data = record.unwrap();
    assert_eq!(data["id"], "user-001");
    assert_eq!(data["user_id"], "joel");  // snake_case in JSON

    cleanup_test_adapter(adapter).await;
}

#[tokio::test]
async fn test_read_nonexistent_record() {
    let mut adapter = setup_test_adapter().await;
    create_users_table(&adapter).await;

    // Test: Read non-existent record returns None
    let result = adapter.read("users", "nonexistent").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none(), "Should return None for missing record");

    cleanup_test_adapter(adapter).await;
}
```

**Pass Criteria**:
- Existing records returned correctly
- Missing records return None (not error)
- JSON format matches TypeScript adapter

---

### 6. SqliteAdapter::query() - The Critical Component

**Why Test First**: Most complex operation. Filter, sort, limit, offset must all work.

#### Test 6a: Basic Query (No Filters)

```rust
#[tokio::test]
async fn test_query_all_records() {
    let mut adapter = setup_test_adapter().await;
    seed_users(&adapter, 100).await;  // Insert 100 test users

    let query = serde_json::json!({
        "collection": "users"
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());

    let records = result.unwrap();
    assert_eq!(records.len(), 100, "Should return all 100 records");

    cleanup_test_adapter(adapter).await;
}
```

#### Test 6b: Filtering

```rust
#[tokio::test]
async fn test_query_with_filter() {
    let mut adapter = setup_test_adapter().await;
    seed_users(&adapter, 100).await;

    let query = serde_json::json!({
        "collection": "users",
        "filter": {
            "isActive": true  // camelCase in filter
        }
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());

    let records = result.unwrap();
    // Verify all returned records have isActive=true
    for record in records {
        assert_eq!(record["data"]["is_active"], 1);  // SQLite stores bool as int
    }

    cleanup_test_adapter(adapter).await;
}
```

#### Test 6c: Sorting

```rust
#[tokio::test]
async fn test_query_with_sort() {
    let mut adapter = setup_test_adapter().await;
    seed_users(&adapter, 100).await;

    let query = serde_json::json!({
        "collection": "users",
        "sort": [{
            "field": "createdAt",  // camelCase
            "direction": "desc"
        }]
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());

    let records = result.unwrap();
    // Verify records sorted descending by createdAt
    for i in 0..records.len()-1 {
        let current = &records[i]["data"]["created_at"];
        let next = &records[i+1]["data"]["created_at"];
        assert!(current >= next, "Records not sorted descending");
    }

    cleanup_test_adapter(adapter).await;
}
```

#### Test 6d: Pagination (CRITICAL - This is where timeouts happen)

```rust
#[tokio::test]
async fn test_query_with_limit_offset() {
    let mut adapter = setup_test_adapter().await;
    seed_users(&adapter, 15_000).await;  // Real-world scale: 15k records

    // Test: First page
    let query = serde_json::json!({
        "collection": "users",
        "limit": 50,
        "offset": 0
    });

    let start = std::time::Instant::now();
    let result = adapter.query(query).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 50);
    assert!(elapsed.as_millis() < 100, "Query too slow: {}ms", elapsed.as_millis());

    // Test: Middle page
    let query = serde_json::json!({
        "collection": "users",
        "limit": 50,
        "offset": 7500
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 50);

    cleanup_test_adapter(adapter).await;
}

#[tokio::test]
async fn test_query_negative_limit() {
    let mut adapter = setup_test_adapter().await;
    seed_users(&adapter, 100).await;

    // Test: LIMIT -1 means "no limit" (this was the panic bug)
    let query = serde_json::json!({
        "collection": "users",
        "limit": -1
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 100, "Should return all records");

    cleanup_test_adapter(adapter).await;
}
```

**Pass Criteria**:
- Filter works correctly
- Sort works correctly (ASC/DESC)
- Limit/offset pagination works
- LIMIT -1 handled (no panic)
- 15k records query completes <100ms
- No timeouts

---

### 7. Performance Benchmarks

**Why Test**: 15k records is NOTHING for an indexed DB. Must be fast.

```rust
// tests/benchmarks/query_performance.rs
#[tokio::test]
async fn benchmark_query_15k_records() {
    let mut adapter = setup_test_adapter().await;

    // Create indexed table
    create_indexed_users_table(&adapter).await;
    seed_users(&adapter, 15_000).await;

    // Benchmark: Full table scan
    let query = serde_json::json!({
        "collection": "users"
    });

    let start = std::time::Instant::now();
    let result = adapter.query(query).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 15_000);

    println!("Query 15k records: {}ms", elapsed.as_millis());
    assert!(elapsed.as_millis() < 500, "Query too slow: {}ms", elapsed.as_millis());

    // Benchmark: Indexed query (filter on indexed column)
    let query = serde_json::json!({
        "collection": "users",
        "filter": { "userId": "joel" }
    });

    let start = std::time::Instant::now();
    let result = adapter.query(query).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    println!("Indexed query: {}ms", elapsed.as_millis());
    assert!(elapsed.as_millis() < 10, "Indexed query too slow: {}ms", elapsed.as_millis());

    cleanup_test_adapter(adapter).await;
}
```

**Pass Criteria**:
- 15k full scan: <500ms
- Indexed query: <10ms
- No timeouts, no panics

---

## Comparison Testing: Rust vs TypeScript

**The Gold Standard**: TypeScript adapter is known-good. Rust must match **exactly**.

```rust
// tests/integration/typescript_comparison.rs
#[tokio::test]
async fn test_output_matches_typescript() {
    // 1. Query same data with TypeScript adapter (via Commands.execute)
    let ts_result = execute_typescript_query().await;

    // 2. Query same data with Rust adapter
    let rust_result = execute_rust_query().await;

    // 3. Compare JSON output (must be identical)
    assert_eq!(
        serde_json::to_string(&ts_result).unwrap(),
        serde_json::to_string(&rust_result).unwrap(),
        "Rust adapter output does not match TypeScript adapter"
    );
}
```

**How to Test**:
1. Start system with `DATA_DAEMON_TYPE=sqlite` (TypeScript)
2. Execute query via `./jtag data/list`, capture JSON
3. Switch to `DATA_DAEMON_TYPE=rust`, restart
4. Execute same query, capture JSON
5. Diff the outputs - must be identical

---

## Test Execution Strategy

### Phase 1: Unit Tests (Isolation)

```bash
cd workers/data-daemon
cargo test --lib unit::
```

**Tests**:
- Field conversion
- Type detection
- Helper functions

**Pass Criteria**: All unit tests green

---

### Phase 2: Integration Tests (Component)

```bash
cargo test --test adapter_init
cargo test --test adapter_create
cargo test --test adapter_read
cargo test --test adapter_query
```

**Tests**:
- Each adapter method in isolation
- Uses test database (not production)
- No TypeScript system dependency

**Pass Criteria**: All integration tests green

---

### Phase 3: Performance Tests

```bash
cargo test --test query_performance -- --nocapture
```

**Tests**:
- 15k record queries
- Pagination performance
- Indexed vs unindexed

**Pass Criteria**: <100ms for paginated queries, <500ms for full scan

---

### Phase 4: System Integration

```bash
# 1. Start Rust workers
./workers/start-workers.sh

# 2. Set DATA_DAEMON_TYPE=rust
echo "DATA_DAEMON_TYPE=rust" > ~/.continuum/config.env

# 3. Start system
npm start

# 4. Run data operations
./jtag data/list --collection=users --limit=50
```

**Tests**:
- Rust worker connects successfully
- RustAdapter initializes
- Queries execute without timeout
- Results match TypeScript adapter

**Pass Criteria**: No errors, no timeouts, identical results

---

## Isolation Trade-offs

**Benefits**:
- Fast feedback (seconds, not minutes)
- Pinpoint failures (know exactly what broke)
- No dependencies (test DB, not production)
- Repeatable (no side effects)

**Risks**:
- Missing integration issues (socket communication, serialization)
- Missing TypeScript→Rust boundary bugs
- Missing production data edge cases

**Mitigation**:
- Integration tests after unit tests pass
- System tests after integration tests pass
- Compare to TypeScript adapter output

---

## Current Status: What's Missing?

1. ❌ **No unit tests** for field conversion, type detection
2. ❌ **No integration tests** for adapter methods
3. ❌ **No performance benchmarks**
4. ❌ **No TypeScript comparison tests**
5. ❌ **No test database setup** (using production DB in tests)

**This is why we've made no progress in 3 days.**

---

## Next Steps (Methodical)

1. **Write unit tests first** (field conversion, type detection)
2. **Implement missing pieces** to make tests pass
3. **Write integration tests** (adapter methods)
4. **Run performance benchmarks** (15k records)
5. **Compare to TypeScript** (gold standard)
6. **System integration** (only after all above pass)

**Do not touch production code until tests are written and passing.**

This is TDD. This is how you build reliable systems.
