use super::{relational_evolution as evolution, relational_schema::DeclaredTable};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, TransactionTrait};

/// Read-only catalog inspection through the same typed projections as admission.
#[tokio::test]
#[ignore = "requires ORM_INSPECT_URL and ORM_INSPECT_TABLE"]
async fn inspect_declared_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("ORM_INSPECT_URL")?;
    let table = std::env::var("ORM_INSPECT_TABLE")?;
    let db = sea_orm::Database::connect(url).await?;
    println!("{:#?}", super::relational_structure::read(&db, &table).await?);
    db.close().await?;
    Ok(())
}

/// Replay only on a private copy of a quiescent database snapshot, never live data.
#[tokio::test]
#[ignore = "requires ORM_REPLAY_DATABASE snapshot path"]
async fn replay_snapshot_open() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::path::PathBuf::from(std::env::var("ORM_REPLAY_DATABASE")?);
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("replay.db");
    std::fs::copy(&source, &target)?;
    for suffix in ["-wal", "-shm"] {
        let sibling = std::path::PathBuf::from(format!("{}{suffix}", source.display()));
        if sibling.exists() { std::fs::copy(sibling, format!("{}{suffix}", target.display()))?; }
    }
    for _ in 0..2 {
        let start = std::time::Instant::now();
        let store = super::relational::RelationalStore::open(super::adapter::AdapterConfig {
            connection_string: target.to_str().ok_or("path encoding")?.into(),
            ..Default::default()
        }).await?;
        println!("SNAPSHOT_OPEN_US {}", start.elapsed().as_micros());
        store.db.close().await?;
    }
    Ok(())
}

mod before {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "counter_fixture")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        #[sea_orm(column_type = "Text")]
        pub name: String,
        #[sea_orm(column_type = "Integer")]
        pub high: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}
mod after {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "counter_fixture")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false, default_value = "work")]
        pub scope: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub name: String,
        pub high: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

#[tokio::test]
async fn derived_key_evolution_preserves_high_water_and_restarts() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("derived.db");
    let url = format!("sqlite://{}?mode=rwc", path.to_str().ok_or("path encoding")?.replace('\\', "/"));
    let db = evolution::schema_connection(&url).await?;
    let transaction = db.begin().await?;
    evolution::admit_table(&transaction, "counter_fixture", DeclaredTable::from_entity(before::Entity)).await?;
    before::ActiveModel { name: Set("kept".into()), high: Set(9_007_199_254_740_993) }.insert(&transaction).await?;
    transaction.commit().await?;
    let transaction = db.begin().await?;
    evolution::admit_table(&transaction, "counter_fixture", DeclaredTable::from_entity(after::Entity)).await?;
    evolution::verify_references(&transaction).await?;
    transaction.commit().await?;
    db.close().await?;
    let reopened = evolution::schema_connection(&url).await?;
    let transaction = reopened.begin().await?;
    evolution::admit_table(&transaction, "counter_fixture", DeclaredTable::from_entity(after::Entity)).await?;
    let row = after::Entity::find_by_id((String::from("work"), String::from("kept"))).one(&transaction).await?.ok_or("row lost")?;
    assert_eq!(row.high, 9_007_199_254_740_993);
    after::ActiveModel { scope: Set("other".into()), name: Set("kept".into()), high: Set(1) }.insert(&transaction).await?;
    transaction.commit().await?;
    reopened.close().await?;
    Ok(())
}
