use super::*;

#[tokio::test]
async fn restart_catalog_crosses_pages_and_rejects_late_physical_drift(
) -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::sea_query::{Alias, ColumnDef, Table};
    use sea_orm::{ConnectionTrait, DbBackend};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("catalog.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let config = || AdapterConfig {
        connection_string: path.into(),
        max_connections: 1,
        ..Default::default()
    };
    let initial = RelationalStore::open(config()).await?;
    // More than two transfer pages; empty tables still require declaration verification.
    for index in 0..131 {
        let mut schema = Parent::collection_schema();
        schema.collection = format!("catalog_{index:03}");
        initial.ensure(schema).await?;
    }
    initial.db.clone().close().await?;
    let started = std::time::Instant::now();
    let restored = RelationalStore::open(config()).await?;
    println!(
        "CATALOG_REPLAY declarations=131 connections=1 elapsed_us={}",
        started.elapsed().as_micros()
    );
    assert_eq!(restored.collections().len(), 131);
    for index in 0..131 {
        let name = format!("catalog_{index:03}");
        assert_eq!(restored.schema(&name)?.collection, name);
    }
    let altered = Table::alter()
        .table(Alias::new("catalog_130"))
        .add_column(ColumnDef::new(Alias::new("unexpected")).text())
        .to_owned();
    restored
        .db
        .execute(DbBackend::Sqlite.build(&altered))
        .await?;
    restored.db.clone().close().await?;
    let failure = RelationalStore::open(config())
        .await
        .err()
        .ok_or("late drift was admitted")?;
    assert!(failure.contains("catalog_130"), "{failure}");
    assert!(failure.contains("physical schema changed"), "{failure}");
    Ok(())
}

#[tokio::test]
async fn restart_refuses_unknown_nested_receipt_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::orm::relational_declaration as declaration;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("unknown-metadata.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let owner = store(path).await?;
    let mut receipt = declaration::Entity::find_by_id(Parent::COLLECTION)
        .one(&owner.db)
        .await?
        .ok_or("fixture declaration absent")?;
    receipt.declaration["fields"][0]["unsupportedCollation"] = json!("NOCASE");
    let changed = receipt.declaration.clone();
    let mut active = receipt.into_active_model();
    active.declaration = Set(changed);
    active.update(&owner.db).await?;
    owner.db.clone().close().await?;
    let failure = store(path)
        .await
        .err()
        .ok_or("unknown metadata was silently discarded")?;
    assert!(
        failure.contains("declared or physical schema changed"),
        "{failure}"
    );
    Ok(())
}
