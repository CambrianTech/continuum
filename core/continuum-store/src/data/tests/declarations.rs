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
    adapter.ensure_schema(declared).await.into_result()?;
    assert!(crate::data::adapters::resolve_collection_schema(
        &registry,
        "unknown_fixture_collection"
    )
    .is_err());
    Ok(())
}
