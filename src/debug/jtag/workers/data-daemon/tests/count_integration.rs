/// Integration test for count() method with production database
use data_daemon_worker::storage::{StorageAdapter, SqliteAdapter};
use serde_json::json;

#[tokio::test]
async fn test_count_production_database() {
    // Use production database path
    let db_path = std::env::var("HOME").unwrap() + "/.continuum/data/continuum.db";

    let mut adapter = SqliteAdapter::new();

    let config = json!({
        "filename": db_path
    });

    adapter.initialize(config).await.expect("Failed to initialize");

    // Test 1: Count all chat_messages
    let count_all = adapter.count(json!({
        "collection": "chat_messages"
    })).await.expect("Count all failed");

    println!("Total chat_messages: {}", count_all);
    assert!(count_all > 0, "Should have at least some messages");

    // Test 2: Count with filter (specific room)
    let count_filtered = adapter.count(json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "5e71a0c8-0303-4eb8-a478-3a121248"
        }
    })).await.expect("Count filtered failed");

    println!("Filtered chat_messages: {}", count_filtered);
    assert!(count_filtered > 0, "Should have messages in that room");
    assert!(count_filtered < count_all, "Filtered count should be less than total");

    // Test 3: Count with non-existent filter (should be 0)
    let count_empty = adapter.count(json!({
        "collection": "chat_messages",
        "filter": {
            "roomId": "non-existent-room-id"
        }
    })).await.expect("Count empty failed");

    println!("Non-existent room messages: {}", count_empty);
    assert_eq!(count_empty, 0, "Non-existent room should have 0 messages");

    adapter.close().await.ok();
}
