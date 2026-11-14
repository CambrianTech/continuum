use super::*;

// A metadata lookup must not transport the historical payload, must still report
// the full match count, and must reject SQL expressions in the selected fields.
#[tokio::test]
async fn list_projects_in_storage_and_preserves_count_and_identity() -> Result<(), Box<dyn std::error::Error>> {
    let module = DataModule::new(no_substrate_entities);
    let (_tmp, path) = test_db_path("projection");
    let adapter = module.state.get_adapter(&path).await?;
    declare_fixture(adapter.as_ref(), "projection", &[("cardId", crate::orm::FieldType::String), ("state", crate::orm::FieldType::Number), ("payload", crate::orm::FieldType::String)]).await;
    for n in 0..3 {
        let created = module.state.create_record(&path, "projection".into(), Some(format!("row-{n}")),
            json!({"cardId": "card", "state": n, "payload": "x".repeat(1_000_000)})).await?;
        assert!(created.success, "{:?}", created.error);
    }
    let params = DataListParams {
        collection: "projection".into(), handle: Some(path.clone()),
        filter: Some(json!({"cardId":"card"})), select: Some(vec!["state".into()]),
        limit: std::num::NonZeroU32::new(1).expect("positive fixture page"), offset: None, sort: None,
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
        limit: std::num::NonZeroU32::new(1).expect("positive fixture page"), offset: None, sort: None,
    };
    assert!(module.state.list(invalid).await.is_err());
    Ok(())
}

#[test]
fn list_schema_requires_a_positive_page_size() {
    for params in [json!({"collection":"rows"}), json!({"collection":"rows","limit":0}), json!({"collection":"rows","limit":null})] {
        assert!(serde_json::from_value::<DataListParams>(params).is_err());
    }
    let params: DataListParams = serde_json::from_value(json!({"collection":"rows","limit":7})).expect("positive page");
    assert_eq!(params.limit.get(), 7);
}
