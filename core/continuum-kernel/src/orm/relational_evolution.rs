//! Declaration-driven evolution within the admitting owner's transaction.
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend};
use sea_orm::sea_query::{Alias, Asterisk, Func, Query, Table};
use super::{adapter::naming, schema_error::SchemaError, types::CollectionSchema};

pub(super) async fn evolve(
    db: &DatabaseTransaction, previous: &CollectionSchema, current: &CollectionSchema,
) -> Result<(), SchemaError> {
    let gap = |detail| SchemaError::gap(&current.collection, detail);
    // Plan completely before issuing mutations. Existing fields retain their exact
    // declared type, constraints and codec; no value is guessed from its name.
    let rebuild = previous.fields.iter().any(|field|
        current.fields.iter().find(|candidate| candidate.name == field.name) != Some(field))
        || previous.indexes.iter().any(|index| !current.indexes.contains(index))
        || current.fields.iter().any(|field| !previous.fields.iter().any(|old| old.name == field.name)
            && (field.unique || field.foreign_key.is_some()));
    for field in &current.fields {
        if let Some(old) = previous.fields.iter().find(|old| old.name == field.name) {
            if old.field_type != field.field_type {
                return Err(gap("type conversion requires a declared conversion contract"));
            }
        }
    }
    let added: Vec<_> = current.fields.iter().filter(|field|
        !previous.fields.iter().any(|old| old.name == field.name)).collect();
    for field in &added {
        if !field.nullable && field.default_value.is_none() {
            return Err(gap("added field requires a declared value or constraint rebuild"));
        }
    }
    let before = super::relational_schema::compile(previous)
        .map_err(|error| SchemaError::gap(&current.collection, error))?;
    let after = super::relational_schema::compile(current)
        .map_err(|error| SchemaError::gap(&current.collection, error))?;
    let table = naming::to_table_name(&current.collection);
    if rebuild {
        return rebuild_table(db, &table, &before, after).await;
    }
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

async fn rebuild_table(
    db: &DatabaseTransaction, table: &str,
    before: &super::relational_schema::DeclaredTable,
    mut after: super::relational_schema::DeclaredTable,
) -> Result<(), SchemaError> {
    let temporary = format!("orm_rebuild_{}", uuid::Uuid::new_v4().simple());
    after.create.table(Alias::new(&temporary));
    db.execute(DbBackend::Sqlite.build(&after.create)).await?;
    let columns: Vec<_> = after.create.get_columns().iter()
        .filter(|column| before.create.get_columns().iter().any(|old|
            old.get_column_name() == column.get_column_name()))
        .map(|column| Alias::new(column.get_column_name())).collect();
    let selection = Query::select().columns(columns.clone()).from(Alias::new(table)).to_owned();
    let mut copy = Query::insert();
    copy.into_table(Alias::new(&temporary)).columns(columns).select_from(selection)
        .map_err(|error| SchemaError::gap(table, error.to_string()))?;
    db.execute(DbBackend::Sqlite.build(&copy)).await?;
    db.execute(DbBackend::Sqlite.build(&Table::drop().table(Alias::new(table)).to_owned())).await?;
    db.execute(DbBackend::Sqlite.build(&Table::rename().table(Alias::new(&temporary), Alias::new(table)).to_owned())).await?;
    for index in after.indexes { db.execute(DbBackend::Sqlite.build(&index)).await?; }
    Ok(())
}

pub(super) async fn verify_references(db: &DatabaseTransaction) -> Result<(), SchemaError> {
    let query = Query::select().column(Asterisk)
        .from_function(Func::cust(Alias::new("pragma_foreign_key_check")), Alias::new("violations"))
        .limit(1).to_owned();
    if db.query_one(DbBackend::Sqlite.build(&query)).await?.is_some() {
        return Err(SchemaError::gap("schema", "foreign-key violation after evolution"));
    }
    Ok(())
}
