//! Durable proof that this owner created the declared physical schema.
//! Receipt-free tables are admitted only after proving the current declaration exactly.
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
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    #[sea_orm(table_name = "sqlite_schema")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub name: String,
        pub tbl_name: String,
        pub r#type: String,
        #[serde(deserialize_with = "serde::Deserialize::deserialize")]
        pub sql: Option<String>,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

async fn physical<C: ConnectionTrait>(db: &C, collection: &str) -> Result<Vec<catalog::Model>, String> {
    catalog::Entity::find()
        .filter(catalog::Column::TblName.eq(naming::to_table_name(collection)))
        .order_by_asc(catalog::Column::Name)
        .all(db)
        .await
        .map_err(|e| e.to_string())
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
        None if actual.is_empty() => Ok(true),
        None => {
            verify_current_declaration(schema, &actual).await?;
            record(db, schema).await?;
            Ok(false)
        }
    }
}

/// Let SQLite canonicalize the owner's current DDL in a disposable, empty catalog.
/// This visits schema metadata only: no resident rows are copied or transformed.
/// There is no alternate declaration, inferred type, or permissive comparison.
async fn verify_current_declaration(
    schema: &CollectionSchema,
    actual: &[catalog::Model],
) -> Result<(), String> {
    let declaration = super::relational_schema::compile(schema)?;
    let expected = sea_orm::Database::connect("sqlite::memory:")
        .await.map_err(|e| e.to_string())?;
    expected.execute(sea_orm::DbBackend::Sqlite.build(&declaration.create))
        .await.map_err(|e| e.to_string())?;
    for index in &declaration.indexes {
        expected.execute(sea_orm::DbBackend::Sqlite.build(index))
            .await.map_err(|e| e.to_string())?;
    }
    let declared = physical(&expected, &schema.collection).await?;
    expected.close().await.map_err(|e| e.to_string())?;
    if actual != declared {
        return Err(format!("{}: physical schema differs from its current declaration; admission refused", schema.collection));
    }
    Ok(())
}

pub(super) async fn record<C: ConnectionTrait>(
    db: &C,
    schema: &CollectionSchema,
) -> Result<(), String> {
    let physical = physical(db, &schema.collection).await?;
    ActiveModel {
        collection: Set(schema.collection.clone()),
        declaration: Set(serde_json::to_value(schema).map_err(|e| e.to_string())?),
        physical: Set(serde_json::to_value(physical).map_err(|e| e.to_string())?),
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
            let schema = decode(&receipt)?;
            super::relational_schema::compile(&schema)?;
            verify_physical(&receipt, &physical(db, &schema.collection).await?)?;
            after = Some(receipt.collection);
            schemas.insert(schema.collection.clone(), std::sync::Arc::new(schema));
        }
    }
    Ok(schemas)
}
fn decode(receipt: &Model) -> Result<CollectionSchema, String> {
    let schema: CollectionSchema =
        CollectionSchema::deserialize(&receipt.declaration).map_err(|e| e.to_string())?;
    if receipt.collection != schema.collection {
        return Err("schema receipt identity mismatch".into());
    }
    Ok(schema)
}
fn verify(receipt: &Model, schema: &CollectionSchema, actual: &[catalog::Model]) -> Result<(), String> {
    if !super::entity::schemas_equivalent(&decode(receipt)?, schema) {
        return Err(format!(
            "{}: declared or physical schema changed; explicit migration required",
            schema.collection
        ));
    }
    verify_physical(receipt, actual)
}
fn verify_physical(receipt: &Model, actual: &[catalog::Model]) -> Result<(), String> {
    let rows = receipt.physical.as_array()
        .ok_or_else(|| format!("{}: physical schema receipt is not an array", receipt.collection))?;
    if rows.len() != actual.len() {
        return Err(format!(
            "{}: declared or physical schema changed; explicit migration required",
            receipt.collection
        ));
    }
    for (row, actual) in rows.iter().zip(actual) {
        if !row.is_object() {
            return Err(format!("{}: physical schema receipt row is not an object", receipt.collection));
        }
        let declared = catalog::Model::deserialize(row)
            .map_err(|e| format!("{}: invalid physical schema receipt: {e}", receipt.collection))?;
        if &declared != actual {
            return Err(format!(
                "{}: declared or physical schema changed; explicit migration required",
                receipt.collection
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_receipt_preserves_null_and_refuses_missing_or_extra_fields()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let actual = [catalog::Model {
            name: "auto_index".into(), tbl_name: "records".into(),
            r#type: "index".into(), sql: None,
        }];
        let mut receipt = Model {
            collection: "records".into(), declaration: Json::Null,
            physical: serde_json::to_value(&actual)?,
        };
        verify_physical(&receipt, &actual)?;
        for invalid in [
            serde_json::json!([["auto_index", "records", "index", null]]),
            serde_json::json!([{"name":"auto_index","tbl_name":"records","type":"index"}]),
            serde_json::json!([{"name":"auto_index","tbl_name":"records","type":"index","sql":null,"extra":1}]),
            serde_json::json!([{"name":"auto_index","tbl_name":"records","type":"index","sql":""}]),
        ] {
            receipt.physical = invalid;
            assert!(verify_physical(&receipt, &actual).is_err());
        }
        Ok(())
    }
}
