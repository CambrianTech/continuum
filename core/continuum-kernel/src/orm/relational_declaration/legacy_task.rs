//! Migrate the unit variant stored as unquoted text by the retired SQLite writer.
use super::{CollectionSchema, Column, Entity, SchemaError};
use crate::orm::types::FieldType;
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter};

pub(super) fn applies(schema: &CollectionSchema) -> bool {
    schema.collection == "staged_credit" && schema.fields.iter().any(|field|
        field.name == "task" && field.field_type == FieldType::Json)
}

pub(super) async fn finish<C: ConnectionTrait>(
    db: &C, schema: &CollectionSchema, migrated: Option<bool>,
) -> Result<(), SchemaError> {
    if migrated != Some(true) && applies(schema) {
        migrate(db, schema).await?;
        Entity::update_many().col_expr(Column::TaskUnitMigrated, Expr::value(true))
            .filter(Column::Collection.eq(schema.collection.as_str())).exec(db).await?;
    }
    Ok(())
}

pub(super) async fn migrate<C: ConnectionTrait>(db: &C, schema: &CollectionSchema) -> Result<(), SchemaError> {
    if !applies(schema) {
        return Ok(());
    }
    let tag = "notATask";
    let update = Query::update().table(Alias::new(&schema.collection))
        .value(Alias::new("task"), serde_json::to_string(tag)?) // disk: staged_credit.task stores JSON text in SQLite.
        .and_where(Expr::col(Alias::new("task")).eq(tag)).to_owned();
    let rows = db.execute(DbBackend::Sqlite.build(&update)).await?.rows_affected();
    tracing::info!(rows, "migrated staged_credit.task unit tags");
    Ok(())
}
