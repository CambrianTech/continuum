//! Typed SQLite catalog projections for declaration comparison.
//! Catalog spelling and column position are not storage contracts.
use sea_orm::{ConnectionTrait, FromQueryResult};
use sea_orm::sea_query::{Alias, Asterisk, Func, Query};
use super::schema_error::SchemaError;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, FromQueryResult)]
pub(super) struct Column {
    pub name: String,
    pub r#type: String,
    pub notnull: i64,
    pub dflt_value: Option<String>,
    pub pk: i64,
    pub hidden: i64,
}

#[derive(Debug, FromQueryResult)]
struct Index {
    name: String,
    unique: i64,
    origin: String,
    partial: i64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, FromQueryResult)]
struct IndexColumn {
    seqno: i64,
    name: Option<String>,
    desc: i64,
    coll: String,
    key: i64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct IndexShape {
    // Explicit names belong to the declaration; SQLite's auto-index names do not.
    name: Option<String>,
    unique: i64,
    origin: String,
    partial: i64,
    columns: Vec<IndexColumn>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, FromQueryResult)]
struct ForeignKey {
    id: i64,
    seq: i64,
    table: String,
    from: String,
    to: Option<String>,
    on_update: String,
    on_delete: String,
    r#match: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Structure {
    columns: Vec<Column>,
    indexes: Vec<IndexShape>,
    foreign_keys: Vec<ForeignKey>,
}

async fn rows<C: ConnectionTrait, T: FromQueryResult>(
    db: &C, function: &str, table: &str,
) -> Result<Vec<T>, SchemaError> {
    let query = Query::select().column(Asterisk)
        .from_function(Func::cust(Alias::new(function)).arg(table), Alias::new("catalog"))
        .to_owned();
    Ok(T::find_by_statement(db.get_database_backend().build(&query)).all(db).await?)
}

pub(super) async fn read<C: ConnectionTrait>(db: &C, table: &str) -> Result<Structure, SchemaError> {
    let mut columns: Vec<Column> = rows(db, "pragma_table_xinfo", table).await?;
    if columns.iter().any(|column| column.hidden != 0) {
        return Err(SchemaError::gap(table, "generated columns require expression proof"));
    }
    for column in &mut columns { column.r#type.make_ascii_uppercase(); }
    columns.sort();
    let indexes: Vec<Index> = rows(db, "pragma_index_list", table).await?;
    let mut shapes = Vec::with_capacity(indexes.len());
    for index in indexes {
        let mut columns: Vec<IndexColumn> = rows(db, "pragma_index_xinfo", &index.name).await?;
        // Auxiliary rowid/storage columns have no bearing on the declared index keys.
        columns.retain(|column| column.key != 0);
        if index.partial != 0 || columns.iter().any(|column| column.name.is_none()) {
            return Err(SchemaError::gap(table, "expression indexes require expression proof"));
        }
        columns.sort();
        shapes.push(IndexShape {
            name: (index.origin == "c").then_some(index.name), unique: index.unique,
            origin: index.origin, partial: index.partial, columns,
        });
    }
    shapes.sort();
    let mut foreign_keys: Vec<ForeignKey> = rows(db, "pragma_foreign_key_list", table).await?;
    foreign_keys.sort();
    Ok(Structure { columns, indexes: shapes, foreign_keys })
}
