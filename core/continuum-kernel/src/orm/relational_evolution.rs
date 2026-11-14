//! Declaration-driven evolution within the admitting owner's transaction.
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend};
use sea_orm::sea_query::{Alias, Table};
use super::{adapter::naming, schema_error::SchemaError, types::CollectionSchema};

pub(super) async fn evolve(
    db: &DatabaseTransaction, previous: &CollectionSchema, current: &CollectionSchema,
) -> Result<(), SchemaError> {
    let gap = |detail| SchemaError::gap(&current.collection, detail);
    // Plan completely before issuing mutations. Existing fields retain their exact
    // declared type, constraints and codec; no value is guessed from its name.
    for field in &previous.fields {
        if current.fields.iter().find(|candidate| candidate.name == field.name) != Some(field) {
            return Err(gap("field removal or alteration requires transactional rebuild"));
        }
    }
    for index in &previous.indexes {
        if !current.indexes.contains(index) {
            return Err(gap("index replacement requires transactional rebuild"));
        }
    }
    let added: Vec<_> = current.fields.iter().filter(|field|
        !previous.fields.iter().any(|old| old.name == field.name)).collect();
    for field in &added {
        if (!field.nullable && field.default_value.is_none()) || field.unique || field.foreign_key.is_some() {
            return Err(gap("added field requires a declared value or constraint rebuild"));
        }
    }
    let before = super::relational_schema::compile(previous)
        .map_err(|error| SchemaError::gap(&current.collection, error))?;
    let after = super::relational_schema::compile(current)
        .map_err(|error| SchemaError::gap(&current.collection, error))?;
    let table = naming::to_table_name(&current.collection);
    for column in after.create.get_columns() {
        if !before.create.get_columns().iter().any(|old| old.get_column_name() == column.get_column_name()) {
            let alter = Table::alter().table(Alias::new(&table)).add_column(column.clone()).to_owned();
            db.execute(DbBackend::Sqlite.build(&alter)).await?;
        }
    }
    // Statements come from the same compiler used for fresh creation. Only new
    // declarations are issued; SQLite still arbitrates global index-name collisions.
    for (index, statement) in current.indexes.iter().zip(after.indexes.iter().skip(
        current.fields.iter().filter(|field| field.indexed && naming::to_snake_case(&field.name) != "id").count()
    )) {
        if !previous.indexes.contains(index) {
            db.execute(DbBackend::Sqlite.build(statement)).await?;
        }
    }
    for (field, statement) in current.fields.iter().filter(|field|
        field.indexed && naming::to_snake_case(&field.name) != "id").zip(&after.indexes) {
        if added.contains(&field) {
            db.execute(DbBackend::Sqlite.build(statement)).await?;
        }
    }
    Ok(())
}
