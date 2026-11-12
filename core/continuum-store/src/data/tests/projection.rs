use super::*;

// A metadata lookup must not transport the historical payload, must still report
// the full match count, and must reject SQL expressions in the selected fields.
#[tokio::test]
async fn list_projects_in_storage_and_preserves_count_and_identity() -> Result<(), Box<dyn std::error::Error>> {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, path) = test_db_path("projection");
    for n in 0..3 {
        let created = module.state.create_record(&path, "projection".into(), Some(format!("row-{n}")),
            json!({"cardId": "card", "state": n, "payload": "x".repeat(1_000_000)})).await?;
        assert!(created.success, "{:?}", created.error);
    }
    let params = serde_json::from_value(json!({"collection":"projection", "dbPath":path,
        "filter":{"cardId":"card"}, "select":["state"], "limit":1}))?;
    let result = module.state.list(params).await?;
    assert_eq!(result.total, 3);
    assert_eq!(result.items.len(), 1);
    let record: DataRecord = serde_json::from_value(result.items.into_iter().next().ok_or("missing projected row")?)?;
    assert!(record.id.starts_with("row-"));
    assert!(record.data.get("state").is_some());
    assert!(record.data.get("payload").is_none());
    assert!(record.data.get("cardId").is_none());
    let invalid = serde_json::from_value(json!({"collection":"projection", "dbPath":path,
        "select":["(select payload from projection)"]}))?;
    assert!(module.state.list(invalid).await.is_err());
    Ok(())
}
