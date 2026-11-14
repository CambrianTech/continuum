#![cfg(test)]

use super::persist_memory_on;
use continuum_client::{mock::MockTransport, Connection};
use serde_json::json;

#[tokio::test]
async fn failed_and_inconsistent_write_receipts_never_report_persistence(
) -> Result<(), Box<dyn std::error::Error>> {
    let memory = serde_json::from_value(json!({
        "record": {
            "id": "fixture", "persona_id": "fixture", "memory_type": "observation",
            "content": "must be durable before cache append", "context": {},
            "timestamp": "2026-11-14T00:00:00Z", "importance": 0.9,
            "access_count": 0, "tags": [], "related_to": [], "source": "fixture"
        },
        "embedding": null
    }))?;
    for receipt in [
        json!({"success":false,"data":null,"error":"injected write refusal"}),
        json!({"success":true,"data":null,"error":null}),
        json!({"success":false,"data":null,"error":null}),
    ] {
        let transport = MockTransport::new();
        transport.respond_with("data/create", receipt);
        let result = persist_memory_on(&Connection::new(transport), &"fixture".into(), &memory).await;
        assert!(result.is_err(), "uncommitted memory was reported durable");
    }
    let transport = MockTransport::new();
    transport.respond_to("data/create", |params| {
        assert_eq!(params["collection"], "memories");
        assert_eq!(params["dbPath"], "@persona:fixture");
        assert_eq!(params["id"], "fixture");
        Ok(json!({
            "success":true, "error":null,
            "data": {
                "id":"fixture", "collection":"memories", "data":params["data"],
                "metadata":{"createdAt":"2026-11-14T00:00:00Z","updatedAt":"2026-11-14T00:00:00Z","version":1}
            }
        }))
    });
    assert!(persist_memory_on(&Connection::new(transport), &"fixture".into(), &memory).await.is_ok());
    Ok(())
}
