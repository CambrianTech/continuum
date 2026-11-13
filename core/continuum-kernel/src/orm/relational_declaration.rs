//! Durable proof that this owner created the declared physical schema.
//! Existing tables without this receipt require an explicit offline migration.
use sea_orm::entity::prelude::*;
use sea_orm::{QueryOrder, Set};
use super::{adapter::naming, types::CollectionSchema};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "orm_declarations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub collection: String,
    pub declaration: Json,
    pub physical: Json,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

mod catalog {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
    #[sea_orm(table_name = "sqlite_schema")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub name: String,
        pub tbl_name: String,
        pub r#type: String,
        pub sql: Option<String>,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

async fn physical<C: ConnectionTrait>(db: &C, collection: &str) -> Result<Json, String> {
    let rows = catalog::Entity::find()
        .filter(catalog::Column::TblName.eq(naming::to_table_name(collection)))
        .order_by_asc(catalog::Column::Name)
        .all(db).await.map_err(|e| e.to_string())?;
    serde_json::to_value(rows).map_err(|e| e.to_string())
}

/// True means a new table may be created. False means its durable receipt verified.
pub(super) async fn admit<C: ConnectionTrait>(db: &C, schema: &CollectionSchema) -> Result<bool, String> {
    let actual = physical(db, &schema.collection).await?;
    let expected = serde_json::to_value(schema).map_err(|e| e.to_string())?;
    match Entity::find_by_id(schema.collection.clone()).one(db).await.map_err(|e| e.to_string())? {
        Some(receipt) if receipt.declaration == expected && receipt.physical == actual => Ok(false),
        Some(_) => Err(format!("{}: declared or physical schema changed; explicit migration required", schema.collection)),
        None if actual.as_array().is_some_and(Vec::is_empty) => Ok(true),
        None => Err(format!("{}: existing table has no declared-schema receipt; explicit migration required", schema.collection)),
    }
}

pub(super) async fn record<C: ConnectionTrait>(db: &C, schema: &CollectionSchema) -> Result<(), String> {
    let physical = physical(db, &schema.collection).await?;
    ActiveModel {
        collection: Set(schema.collection.clone()),
        declaration: Set(serde_json::to_value(schema).map_err(|e| e.to_string())?),
        physical: Set(physical),
    }.insert(db).await.map_err(|e| e.to_string())?;
    Ok(())
}
