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
    let params = DataListParams {
        collection: "projection".into(), handle: Some(path.clone()),
        filter: Some(json!({"cardId":"card"})), select: Some(vec!["state".into()]),
        limit: Some(1), offset: None, sort: None,
    };
    let result = module.state.list(params).await?;
    assert_eq!(result.total, 3);
    assert_eq!(result.items.len(), 1);
    let record = &result.items[0]; // The length assertion above proves this fixture returned one row.
    assert!(record.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with("row-")));
    let data = &record["data"];
    assert!(data.get("state").is_some());
    assert!(data.get("payload").is_none());
    assert!(data.get("cardId").is_none());
    let invalid = DataListParams {
        collection: "projection".into(), handle: Some(path),
        filter: None, select: Some(vec!["(select payload from projection)".into()]),
        limit: Some(1), offset: None, sort: None,
    };
    assert!(module.state.list(invalid).await.is_err());
    Ok(())
}
