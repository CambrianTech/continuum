//! Joins use declared relationships and project related columns in the database.
use super::{
    adapter::naming,
    query::StorageQuery,
    types::{CollectionSchema, DataRecord, SchemaField},
};
use sea_orm::sea_query::{Alias, Expr, JoinType};
use sea_orm::sqlx::{Column, Row, ValueRef};
use sea_orm::{ConnectionTrait, DbBackend};
use serde_json::{Map, Value};

struct Joined {
    alias: String,
    id_alias: String,
    fields: Vec<(String, SchemaField)>,
}

fn declared(from: &CollectionSchema, field: &str, to: &CollectionSchema, target: &str) -> bool {
    from.fields.iter().any(|f| {
        naming::to_snake_case(&f.name) == naming::to_snake_case(field)
            && f.foreign_key.as_ref().is_some_and(|reference| {
                reference.collection == to.collection
                    && naming::to_snake_case(&reference.field) == naming::to_snake_case(target)
            })
    })
}

impl super::relational::RelationalStore {
    pub async fn query_joined(&self, query: StorageQuery) -> Result<Vec<DataRecord>, String> {
        let schema = self.schema(&query.collection)?;
        let mut primary = query.clone();
        primary.joins = None;
        let mut statement = super::relational_query::select(&schema, &primary, super::relational_query::Selection::Page)?;
        let table = Alias::new(naming::to_table_name(&schema.collection));
        let mut joined = Vec::new();
        let mut aliases = std::collections::BTreeSet::new();
        if let Some(joins) = &query.joins {
            for (number, join) in joins.iter().enumerate() {
                if join.alias.is_empty()
                    || !aliases.insert(join.alias.clone())
                    || schema.fields.iter().any(|f| f.name == join.alias)
                {
                    return Err(format!("invalid or colliding join alias {}", join.alias));
                }
                let related = self.schema(&join.collection)?;
                let local = super::relational_query::column(&schema, &join.local_field)?;
                let foreign = super::relational_query::column(&related, &join.foreign_field)?;
                if !declared(&schema, &join.local_field, &related, &join.foreign_field)
                    && !declared(&related, &join.foreign_field, &schema, &join.local_field)
                {
                    return Err(format!(
                        "{} -> {} is not a declared entity relationship",
                        schema.collection, related.collection
                    ));
                }
                let alias = Alias::new(format!("relation_{number}"));
                statement.join_as(
                    match join.join_type {
                        super::query::JoinType::Inner => JoinType::InnerJoin,
                        super::query::JoinType::Left => JoinType::LeftJoin,
                    },
                    Alias::new(naming::to_table_name(&related.collection)),
                    alias.clone(),
                    Expr::col((table.clone(), local)).equals((alias.clone(), foreign)),
                );
                let selected = match &join.select {
                    Some(fields) => fields.clone(),
                    None => related.fields.iter().map(|f| f.name.clone()).collect(),
                };
                let id_alias = format!("joined_{number}_id");
                statement.expr_as(
                    Expr::col((alias.clone(), Alias::new("id"))),
                    Alias::new(&id_alias),
                );
                let mut fields = Vec::new();
                for (i, name) in selected.iter().enumerate() {
                    let col = super::relational_query::column(&related, name)?;
                    let field = related
                        .fields
                        .iter()
                        .find(|f| naming::to_snake_case(&f.name) == naming::to_snake_case(name))
                        .ok_or_else(|| format!("undeclared joined field {name}"))?
                        .clone();
                    let output = format!("joined_{number}_field_{i}");
                    statement.expr_as(Expr::col((alias.clone(), col)), Alias::new(&output));
                    fields.push((output, field));
                }
                joined.push(Joined {
                    alias: join.alias.clone(),
                    id_alias,
                    fields,
                });
            }
        }
        let joined_columns: Vec<String> = joined
            .iter()
            .flat_map(|join| {
                std::iter::once(join.id_alias.clone())
                    .chain(join.fields.iter().map(|(name, _)| name.clone()))
            })
            .collect();
        if schema
            .fields
            .iter()
            .any(|field| joined_columns.contains(&naming::to_snake_case(&field.name)))
        {
            return Err("generated join projection collides with a declared column".into());
        }
        let rows = self
            .db
            .query_all(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?;
        let decoder = super::relational_codec::RowDecoder::new(schema.clone(), &joined_columns);
        rows.into_iter()
            .map(|result| {
                let row = result.try_as_sqlite_row().ok_or("SQLite result required")?;
                let mut nested = Map::new();
                for join in &joined {
                    let id = row
                        .try_get_raw(join.id_alias.as_str())
                        .map_err(|e| e.to_string())?;
                    if id.is_null() {
                        nested.insert(join.alias.clone(), Value::Null);
                        continue;
                    }
                    let mut fields = Map::new();
                    for (alias, field) in &join.fields {
                        let index = row
                            .columns()
                            .iter()
                            .position(|c| c.name() == alias)
                            .ok_or("missing joined column")?;
                        fields.insert(
                            field.name.clone(),
                            super::relational_codec::scalar(row, index, field)?,
                        );
                    }
                    nested.insert(join.alias.clone(), Value::Object(fields));
                }
                let mut record =
                    decoder.decode(result)?;
                record
                    .data
                    .as_object_mut()
                    .ok_or("record data is not an object")?
                    .extend(nested);
                Ok(record)
            })
            .collect()
    }
}
