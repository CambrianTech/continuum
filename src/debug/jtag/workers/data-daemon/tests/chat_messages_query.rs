/// Integration test for chat_messages queries
/// Tests the exact production use case: filter by roomId, sort by createdAt, limit 30
///
/// This test reproduces the bug: WHERE clause parameters not passed to query_map()

use data_daemon_worker::storage::{StorageAdapter, SqliteAdapter};
use serde_json::json;
use std::time::Instant;

/// Setup test database with chat_messages table (matches production schema)
async fn setup_test_db() -> SqliteAdapter {
    let mut adapter = SqliteAdapter::new();

    let config = json!({
        "filename": "/tmp/test-chat-messages.sqlite"
    });

    adapter.initialize(config).await.expect("Failed to initialize adapter");

    // Create chat_messages table (production schema)
    // Using raw SQL for now since we don't have migrations in test
    let conn = get_connection(&adapter);
    conn.execute("DROP TABLE IF EXISTS chat_messages", []).unwrap();
    conn.execute(
        "CREATE TABLE chat_messages (
            id TEXT PRIMARY KEY,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            version INTEGER DEFAULT 1
        )",
        [],
    ).unwrap();

    // Create index on room_id (critical for performance)
    conn.execute(
        "CREATE INDEX idx_chat_room_time ON chat_messages(room_id, created_at DESC)",
        [],
    ).unwrap();

    adapter
}

/// Seed test data: 3 rooms with messages
/// - general: 50 messages
/// - random: 30 messages
/// - academy: 20 messages
/// Total: 100 messages (small dataset for fast testing)
async fn seed_chat_messages(adapter: &SqliteAdapter) {
    let conn = get_connection(adapter);

    let rooms = vec![
        ("room-general-uuid", "general", 50),
        ("room-random-uuid", "random", 30),
        ("room-academy-uuid", "academy", 20),
    ];

    let mut message_count = 1;
    for (room_id, room_name, count) in rooms {
        for i in 0..count {
            let id = format!("msg-{:04}", message_count);
            let timestamp = format!("2025-01-01T{:02}:{:02}:00Z", i / 60, i % 60);

            conn.execute(
                "INSERT INTO chat_messages (id, room_id, user_id, content, created_at, updated_at, version)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    id,
                    room_id,
                    "user-joel-uuid",
                    format!("Message {} in {}", i + 1, room_name),
                    &timestamp,
                    &timestamp,
                    1
                ],
            ).unwrap();

            message_count += 1;
        }
    }
}

/// Helper to get underlying connection (for setup only)
fn get_connection(adapter: &SqliteAdapter) -> std::sync::MutexGuard<'_, Option<rusqlite::Connection>> {
    // HACK: This is just for testing - accessing private field
    // In real code, adapter methods handle this
    unsafe {
        let ptr = adapter as *const SqliteAdapter as *mut SqliteAdapter;
        let adapter_mut = &mut *ptr;
        // Access connection field (this is a test hack)
        std::mem::transmute::<_, &mut std::sync::Arc<std::sync::Mutex<Option<rusqlite::Connection>>>>(
            &mut *(adapter_mut as *mut _ as *mut u8).offset(0)
        ).lock().unwrap()
    }
}

/// Test: Query messages for one room only
#[tokio::test]
async fn test_filter_by_room_id() {
    let adapter = setup_test_db().await;
    seed_chat_messages(&adapter).await;

    // Query: Get messages from "general" room only
    let query = json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "room-general-uuid"  // camelCase (will convert to room_id)
        },
        "sort": [{
            "field": "createdAt",
            "direction": "desc"
        }],
        "limit": 30
    });

    let result = adapter.query(query).await;

    // Assert: Query succeeded
    assert!(result.is_ok(), "Query failed: {:?}", result.err());

    let records = result.unwrap();

    // BUG DETECTION: If WHERE clause params not passed, this will return ALL 100 messages
    // Expected: 30 messages (limited from 50 in "general" room)
    // Actual (with bug): 30 messages but from ALL rooms (wrong!)

    println!("Returned {} records", records.len());
    assert_eq!(records.len(), 30, "Should return exactly 30 records");

    // CRITICAL TEST: Verify ALL records are from "general" room
    for (i, record) in records.iter().enumerate() {
        let room_id = record["data"]["room_id"]
            .as_str()
            .expect(&format!("Record {} missing room_id", i));

        assert_eq!(
            room_id, "room-general-uuid",
            "Record {} has wrong room_id: {} (expected room-general-uuid)",
            i, room_id
        );
    }

    // Cleanup
    std::fs::remove_file("/tmp/test-chat-messages.sqlite").ok();
}

/// Test: Count query (no SELECT *, just COUNT(*))
#[tokio::test]
async fn test_count_messages_in_room() {
    let adapter = setup_test_db().await;
    seed_chat_messages(&adapter).await;

    // This is what we SHOULD do for totalCount
    // For now, we'll use query() and check length
    // TODO: Add count() method to StorageAdapter trait

    let query = json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "room-general-uuid"
        }
        // No limit - get all for counting
    });

    let result = adapter.query(query).await;
    assert!(result.is_ok());

    let records = result.unwrap();

    // Should return all 50 messages from general room
    assert_eq!(records.len(), 50, "Should return all 50 messages from general room");

    // Verify all from correct room
    for record in &records {
        let room_id = record["data"]["room_id"].as_str().unwrap();
        assert_eq!(room_id, "room-general-uuid");
    }

    // Cleanup
    std::fs::remove_file("/tmp/test-chat-messages.sqlite").ok();
}

/// Test: Performance - should be instant even with thousands of messages
#[tokio::test]
async fn test_query_performance() {
    let adapter = setup_test_db().await;

    // Seed 5000 messages across 3 rooms
    let conn = get_connection(&adapter);
    for i in 0..5000 {
        let room_id = match i % 3 {
            0 => "room-general-uuid",
            1 => "room-random-uuid",
            _ => "room-academy-uuid",
        };

        let id = format!("msg-{:05}", i);
        let timestamp = format!("2025-01-01T{:02}:{:02}:00Z", i / 3600, (i / 60) % 60);

        conn.execute(
            "INSERT INTO chat_messages (id, room_id, user_id, content, created_at, updated_at, version)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                id,
                room_id,
                "user-joel-uuid",
                format!("Message {}", i + 1),
                &timestamp,
                &timestamp,
                1
            ],
        ).unwrap();
    }
    drop(conn); // Release lock

    // Query with filter (should use index)
    let query = json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "room-general-uuid"
        },
        "sort": [{
            "field": "createdAt",
            "direction": "desc"
        }],
        "limit": 30
    });

    let start = Instant::now();
    let result = adapter.query(query).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Query failed");

    let records = result.unwrap();
    assert_eq!(records.len(), 30);

    // Performance assertion: Should be <100ms with index
    println!("Query took: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 100,
        "Query too slow: {}ms (expected <100ms)",
        elapsed.as_millis()
    );

    // Cleanup
    std::fs::remove_file("/tmp/test-chat-messages.sqlite").ok();
}

/// Test: Pagination (OFFSET)
#[tokio::test]
async fn test_pagination() {
    let adapter = setup_test_db().await;
    seed_chat_messages(&adapter).await;

    // Get page 1 (first 10 messages)
    let page1_query = json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "room-general-uuid"
        },
        "sort": [{
            "field": "createdAt",
            "direction": "asc"  // Ascending for predictable pagination
        }],
        "limit": 10,
        "offset": 0
    });

    let page1 = adapter.query(page1_query).await.unwrap();
    assert_eq!(page1.len(), 10);

    // Get page 2 (next 10 messages)
    let page2_query = json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "room-general-uuid"
        },
        "sort": [{
            "field": "createdAt",
            "direction": "asc"
        }],
        "limit": 10,
        "offset": 10
    });

    let page2 = adapter.query(page2_query).await.unwrap();
    assert_eq!(page2.len(), 10);

    // Verify no overlap (different message IDs)
    let page1_id = page1[0]["data"]["id"].as_str().unwrap();
    let page2_id = page2[0]["data"]["id"].as_str().unwrap();
    assert_ne!(page1_id, page2_id, "Pages should have different messages");

    // Cleanup
    std::fs::remove_file("/tmp/test-chat-messages.sqlite").ok();
}
