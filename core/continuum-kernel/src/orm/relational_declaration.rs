//! Durable proof that this owner created the declared physical schema.
//! Existing tables without this receipt require an explicit offline migration.
use super::{adapter::naming, types::CollectionSchema};
use sea_orm::entity::prelude::*;
use sea_orm::{QueryOrder, QuerySelect, Set};
use serde::Deserialize;

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
        .all(db)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(rows).map_err(|e| e.to_string())
}

/// True means a new table may be created. False means its durable receipt verified.
pub(super) async fn admit<C: ConnectionTrait>(
    db: &C,
    schema: &CollectionSchema,
) -> Result<bool, String> {
    let actual = physical(db, &schema.collection).await?;
    match Entity::find_by_id(schema.collection.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(receipt) => {
            verify(&receipt, schema, &actual)?;
            Ok(false)
        }
        None if actual.as_array().is_some_and(Vec::is_empty) => Ok(true),
        None => Err(format!(
            "{}: existing table has no declared-schema receipt; explicit migration required",
            schema.collection
        )),
    }
}

pub(super) async fn record<C: ConnectionTrait>(
    db: &C,
    schema: &CollectionSchema,
) -> Result<(), String> {
    let physical = physical(db, &schema.collection).await?;
    ActiveModel {
        collection: Set(schema.collection.clone()),
        declaration: Set(serde_json::to_value(schema).map_err(|e| e.to_string())?),
        physical: Set(physical),
    }
    .insert(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

// Transfer batch size, never a cap on the number of declarations admitted.
const CATALOG_PAGE_SIZE: u64 = 64;

/// Restore schema receipts in primary-key pages once per connection owner.
/// Receipt and physical verification still precede admission; this is not inference.
pub(super) async fn restore<C: ConnectionTrait>(
    db: &C,
) -> Result<std::collections::HashMap<String, std::sync::Arc<CollectionSchema>>, String> {
    let mut schemas = std::collections::HashMap::new();
    let mut after: Option<String> = None;
    loop {
        let mut query = Entity::find()
            .order_by_asc(Column::Collection)
            .limit(CATALOG_PAGE_SIZE);
        if let Some(cursor) = &after {
            query = query.filter(Column::Collection.gt(cursor.as_str()));
        }
        let receipts = query.all(db).await.map_err(|e| e.to_string())?;
        if receipts.is_empty() {
            break;
        }
        for receipt in receipts {
            let schema: CollectionSchema =
                CollectionSchema::deserialize(&receipt.declaration).map_err(|e| e.to_string())?;
            if receipt.collection != schema.collection {
                return Err("schema receipt identity mismatch".into());
            }
            super::relational_schema::compile(&schema)?;
            verify(&receipt, &schema, &physical(db, &schema.collection).await?)?;
            after = Some(receipt.collection);
            schemas.insert(schema.collection.clone(), std::sync::Arc::new(schema));
        }
    }
    Ok(schemas)
}
fn verify(receipt: &Model, schema: &CollectionSchema, actual: &Json) -> Result<(), String> {
    let expected = serde_json::to_value(schema).map_err(|e| e.to_string())?;
    if receipt.collection != schema.collection
        || receipt.declaration != expected
        || &receipt.physical != actual
    {
        return Err(format!(
            "{}: declared or physical schema changed; explicit migration required",
            schema.collection
        ));
    }
    Ok(())
}
