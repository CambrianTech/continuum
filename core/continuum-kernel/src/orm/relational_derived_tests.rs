use super::{relational_evolution as evolution, relational_schema::DeclaredTable};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, TransactionTrait};

mod before {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "counter_fixture")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub name: String,
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
