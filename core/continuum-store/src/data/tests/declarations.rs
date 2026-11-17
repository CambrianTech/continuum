#![cfg(test)]

use crate::orm::{BaseEntity, OrmEntity, OrmEntityRegistry, StorageAdapter};

#[derive(serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[entity(collection = "memories")]
#[serde(rename_all = "camelCase")]
struct RustMemory {
    #[serde(flatten)]
    base: BaseEntity,
    owned_field: String,
}

#[derive(serde::Serialize, serde::Deserialize, crate::orm::Entity)]
#[entity(collection = "rust_only_credit_decisions")]
#[serde(rename_all = "camelCase")]
struct RustOnlyDecision {
    #[serde(flatten)]
    base: BaseEntity,
    accepted: bool,
    rejected_at_ms: Option<u64>,
}

#[tokio::test]
async fn startup_evolves_rust_only_integer_history_before_publication()
    -> Result<(), Box<dyn std::error::Error>>
{
    use crate::orm::{DataRecord, FieldType, StorageQuery};
    use serde_json::json;
    let home = tempfile::tempdir()?;
    let path = home.path().join("historical-credit.sqlite");
    let config = crate::orm::adapter::AdapterConfig {
        connection_string: path.to_string_lossy().into_owned(), max_connections: 1,
        ..Default::default()
    };
    let mut initial = crate::orm::SeaOrmAdapter::connect(config.clone()).await?;
    let mut previous = RustOnlyDecision::collection_schema();
    previous.fields.iter_mut().find(|field| field.name == "rejectedAtMs")
        .ok_or("missing declared rejection timestamp")?.field_type = FieldType::String;
    initial.ensure_schema(&previous).await?;
    for (id, timestamp) in [("rejected", json!("9007199254740993")), ("unreviewed", json!(null))] {
        initial.create(DataRecord {
            id: id.into(), collection: RustOnlyDecision::COLLECTION.into(),
            data: json!({"accepted":false, "rejectedAtMs":timestamp}),
            metadata: Default::default(),
        }).await?;
    }
    initial.close().await?;
    let registry = OrmEntityRegistry::new();
    registry.register::<RustOnlyDecision>()?;
    for _ in 0..2 {
        let mut reopened = crate::orm::SeaOrmAdapter::connect(config.clone()).await?;
        crate::data::adapters::ensure_declared_schemas(&reopened, &registry).await?;
        let result = reopened.query(&StorageQuery {
            collection: RustOnlyDecision::COLLECTION.into(), limit: Some(2),
            ..Default::default()
        }).await?;
        assert_eq!(result.len(), 2);
        for row in result {
            assert_eq!(row.data["accepted"], false);
            assert_eq!(row.data["rejectedAtMs"], if row.id == "rejected" {
                json!(9007199254740993u64)
            } else { json!(null) });
        }
        reopened.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn rust_only_entity_is_readable_before_any_write_and_after_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = OrmEntityRegistry::new();
    registry.register::<RustOnlyDecision>()?;
    let home = tempfile::tempdir()?;
    let path = home.path().join("decisions.sqlite");
    for _ in 0..2 {
        let mut adapter = crate::orm::SeaOrmAdapter::connect(crate::orm::adapter::AdapterConfig {
            connection_string: path.to_string_lossy().into_owned(),
            max_connections: 1,
            ..Default::default()
        }).await?;
        crate::data::adapters::ensure_declared_schemas(&adapter, &registry).await?;
        let count = adapter.count(&crate::orm::StorageQuery {
            collection: RustOnlyDecision::COLLECTION.into(),
            ..Default::default()
        }).await?;
        assert_eq!(count, 0);
        adapter.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn startup_and_later_admission_use_the_same_registered_declaration(
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = OrmEntityRegistry::new();
    registry.register::<RustMemory>()?;
    let adapter = crate::orm::SeaOrmAdapter::connect(crate::orm::adapter::AdapterConfig {
        connection_string: ":memory:".into(),
        max_connections: 1,
        ..Default::default()
    })
    .await?;
    crate::data::adapters::ensure_declared_schemas(&adapter, &registry).await?;
    let declared =
        crate::data::adapters::resolve_collection_schema(&registry, RustMemory::COLLECTION)?;
    assert!(declared
        .fields
        .iter()
        .any(|field| field.name == "ownedField"));
    assert!(!declared
        .fields
        .iter()
        .any(|field| field.name == "sessionId"));
    adapter.ensure_schema(&declared).await?;
    assert!(crate::data::adapters::resolve_collection_schema(
        &registry,
        "unknown_fixture_collection"
    )
    .is_err());
    Ok(())
}
