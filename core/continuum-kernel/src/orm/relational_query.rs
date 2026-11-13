//! Schema-checked relational queries. No data-dependent SQL or application filtering.
use super::{
    adapter::naming,
    query::{CursorDirection, FieldFilter, QueryOperator, SortDirection, StorageQuery},
    types::CollectionSchema,
};
use sea_orm::sea_query::{
    Alias, Condition, Expr, Order, Query, SelectStatement, SimpleExpr, Value as SqlValue,
};
use serde_json::Value;

pub(super) fn column(schema: &CollectionSchema, field: &str) -> Result<Alias, String> {
    let name = naming::to_snake_case(field);
    if schema
        .fields
        .iter()
        .any(|f| naming::to_snake_case(&f.name) == name)
    {
        Ok(Alias::new(name))
    } else {
        Err(format!(
            "{}.{} is not a declared field",
            schema.collection, field
        ))
    }
}

pub(super) fn value(value: &Value) -> Result<SqlValue, String> {
    Ok(match value {
        Value::Null => SqlValue::String(None),
        Value::Bool(v) => (*v).into(),
        Value::String(v) => v.clone().into(),
        Value::Number(v) => {
            if let Some(n) = v.as_i64() {
                n.into()
            } else if v.is_u64() {
                return Err("integer exceeds the declared SQLite signed integer range".into());
            } else {
                v.as_f64().ok_or("invalid finite number")?.into()
            }
        }
        Value::Array(_) | Value::Object(_) => {
            return Err("relational comparison requires a scalar field".into())
        }
    })
}

pub(super) fn condition(
    schema: &CollectionSchema,
    query: &StorageQuery,
) -> Result<Condition, String> {
    let table = Alias::new(naming::to_table_name(&schema.collection));
    let mut all = Condition::all();
    if let Some(filters) = &query.filter {
        // Deterministic statement order keeps identical query shapes cacheable.
        let mut fields: Vec<_> = filters.iter().collect();
        fields.sort_by_key(|(field, _)| *field);
        for (field, filter) in fields {
            let col = Expr::col((table.clone(), column(schema, field)?));
            let expr: SimpleExpr = match filter {
                FieldFilter::Value(v) | FieldFilter::Operator(QueryOperator::Eq(v))
                    if v.is_null() =>
                {
                    col.is_null()
                }
                FieldFilter::Value(v) | FieldFilter::Operator(QueryOperator::Eq(v)) => {
                    col.eq(value(v)?)
                }
                FieldFilter::Operator(op) => match op {
                    QueryOperator::Eq(v) => col.eq(value(v)?),
                    QueryOperator::Ne(v) if v.is_null() => col.is_not_null(),
                    QueryOperator::Ne(v) => col.ne(value(v)?),
                    QueryOperator::Gt(v) => col.gt(value(v)?),
                    QueryOperator::Gte(v) => col.gte(value(v)?),
                    QueryOperator::Lt(v) => col.lt(value(v)?),
                    QueryOperator::Lte(v) => col.lte(value(v)?),
                    QueryOperator::In(v) => {
                        col.is_in(v.iter().map(value).collect::<Result<Vec<_>, _>>()?)
                    }
                    QueryOperator::NotIn(v) => {
                        col.is_not_in(v.iter().map(value).collect::<Result<Vec<_>, _>>()?)
                    }
                    QueryOperator::Exists(true) | QueryOperator::IsNotNull => col.is_not_null(),
                    QueryOperator::Exists(false) | QueryOperator::IsNull => col.is_null(),
                    QueryOperator::Contains(text) => {
                        // Escape LIKE wildcards: contains is literal, not a pattern language.
                        let escaped = text
                            .replace('!', "!!")
                            .replace('%', "!%")
                            .replace('_', "!_");
                        col.like(
                            sea_orm::sea_query::LikeExpr::new(format!("%{escaped}%")).escape('!'),
                        )
                    }
                    QueryOperator::Regex(_) => {
                        return Err("SQLite provider does not support regex queries".into())
                    }
                },
            };
            all = all.add(expr);
        }
    }
    if let Some(cursor) = &query.cursor {
        let field = schema
            .fields
            .iter()
            .find(|field| {
                naming::to_snake_case(&field.name) == naming::to_snake_case(&cursor.field)
            })
            .ok_or_else(|| format!("undeclared cursor field {}", cursor.field))?;
        let unique = field.name == "id"
            || field.unique
            || schema.indexes.iter().any(|index| {
                index.unique
                    && index.fields.len() == 1
                    && naming::to_snake_case(&index.fields[0]) == naming::to_snake_case(&field.name)
            });
        if !unique || field.nullable || cursor.value.is_null() {
            return Err("single-value cursors require a declared non-null unique field; a non-unique order needs a composite cursor".into());
        }
        if query.sort.as_ref().is_some_and(|sort| {
            sort.len() != 1
                || naming::to_snake_case(&sort[0].field) != naming::to_snake_case(&cursor.field)
        }) {
            return Err("cursor field must match the query's unique ordering field".into());
        }
        let col = Expr::col((table, column(schema, &cursor.field)?));
        all = all.add(match cursor.direction {
            CursorDirection::After => col.gt(value(&cursor.value)?),
            CursorDirection::Before => col.lt(value(&cursor.value)?),
        });
    }
    if query.tags.is_some() {
        return Err("tags must be expressed through a declared relationship".into());
    }
    if let Some(range) = &query.time_range {
        let col = || {
            Expr::col((
                Alias::new(naming::to_table_name(&schema.collection)),
                Alias::new("created_at"),
            ))
        };
        if let Some(start) = &range.start {
            all = all.add(col().gte(start.clone()));
        }
        if let Some(end) = &range.end {
            all = all.add(col().lte(end.clone()));
        }
    }
    Ok(all)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Selection { Page, Stream, Count }

pub(super) fn select(
    schema: &CollectionSchema,
    query: &StorageQuery,
    selection: Selection,
) -> Result<SelectStatement, String> {
    if selection == Selection::Page && query.limit.is_none() {
        return Err(
            "record reads require an explicit page limit; use count for cardinality".into(),
        );
    }
    if query.joins.as_ref().is_some_and(|joins| !joins.is_empty()) {
        return Err("join requires the relationship query compiler".into());
    }
    let table = Alias::new(naming::to_table_name(&schema.collection));
    let mut statement = Query::select();
    statement
        .from(table.clone())
        .cond_where(condition(schema, query)?);
    if selection == Selection::Count {
        statement.expr_as(
            Expr::col((table, column(schema, "id")?)).count(),
            Alias::new("count"),
        );
        return Ok(statement);
    }
    let mut fields = match &query.select {
        Some(fields) => fields.clone(),
        None => schema.fields.iter().map(|f| f.name.clone()).collect(),
    };
    if fields.is_empty() {
        return Err("empty projection".into());
    }
    for metadata in ["id", "createdAt", "updatedAt", "version"] {
        if !fields
            .iter()
            .any(|f| naming::to_snake_case(f) == naming::to_snake_case(metadata))
        {
            fields.push(metadata.into());
        }
    }
    for field in fields {
        statement.column((table.clone(), column(schema, &field)?));
    }
    if let Some(sort) = &query.sort {
        for order in sort {
            statement.order_by(
                (table.clone(), column(schema, &order.field)?),
                match order.direction {
                    SortDirection::Asc => Order::Asc,
                    SortDirection::Desc => Order::Desc,
                },
            );
        }
    }
    if query.sort.as_ref().is_none_or(|sort| sort.is_empty()) {
        let field = match &query.cursor {
            Some(cursor) => cursor.field.as_str(),
            None => "id",
        };
        statement.order_by((table.clone(), column(schema, field)?), Order::Asc);
    } else if !query.sort.as_ref().is_some_and(|sort| {
        sort.iter()
            .any(|order| naming::to_snake_case(&order.field) == "id")
    }) {
        statement.order_by((table.clone(), column(schema, "id")?), Order::Asc);
    }
    if let Some(limit) = query.limit {
        statement.limit(limit.try_into().map_err(|_| "limit exceeds u64")?);
    }
    if let Some(offset) = query.offset {
        statement.offset(offset.try_into().map_err(|_| "offset exceeds u64")?);
    }
    Ok(statement)
}
