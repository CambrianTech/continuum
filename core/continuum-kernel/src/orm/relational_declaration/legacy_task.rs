//! Migrate the unit variant stored as unquoted text by the retired SQLite writer.
use super::{CollectionSchema, SchemaError};
use crate::orm::types::FieldType;
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ConnectionTrait, DbBackend};

pub(super) async fn migrate<C: ConnectionTrait>(
    db: &C,
    schema: &CollectionSchema,
) -> Result<(), SchemaError> {
    if schema.collection != "staged_credit" {
        return Ok(());
    }
    let Some(field) = schema.fields.iter().find(|field| field.name == "task"
        && field.field_type == FieldType::Json) else { return Ok(()); };
    let table = Alias::new(&schema.collection);
    let column = Alias::new(&field.name);
    let tag = "notATask";
    let matches = Expr::col(column.clone()).eq(tag);
    let probe = Query::select().expr(Expr::val(1)).from(table.clone())
        .and_where(matches.clone()).limit(1).to_owned();
    if db.query_one(DbBackend::Sqlite.build(&probe)).await?.is_none() {
        return Ok(());
    }
    let update = Query::update().table(table)
        .value(column, serde_json::to_string(tag)?)
        .and_where(matches).to_owned();
    let rows = db.execute(DbBackend::Sqlite.build(&update)).await?.rows_affected();
    tracing::info!(rows, "migrated staged_credit.task unit tags");
    Ok(())
}
