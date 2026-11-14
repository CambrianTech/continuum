//! Schema-checked relational queries. No data-dependent SQL or application filtering.
use super::{
    adapter::naming,
    query::{CursorDirection, FieldFilter, QueryOperator, SortDirection, StorageQuery},
    types::{CollectionSchema, SchemaField, FieldType},
};
use sea_orm::sea_query::{
    Alias, Condition, Expr, Order, Query, SelectStatement, SimpleExpr, Value as SqlValue,
};
use serde_json::Value;

use super::storage_error::{QueryFault, StorageError};

pub(super) fn column(schema: &CollectionSchema, field: &str) -> Result<Alias, StorageError> {
    let name = naming::to_snake_case(field);
    if schema
        .fields
        .iter()
        .any(|f| naming::to_snake_case(&f.name) == name)
    {
        Ok(Alias::new(name))
    } else {
        Err(StorageError::Undeclared { collection: schema.collection.clone().into(), field: field.to_owned().into() })
    }
}

/// Predicates bind through the same declared scalar codec as writes. Opaque payloads
/// have no relational ordering: promote their searchable facts into declared columns.
fn bound_value(field: &SchemaField, value: &Value) -> Result<SqlValue, StorageError> {
    if matches!(field.field_type, FieldType::Json | FieldType::Binary) {
        return Err(StorageError::Unqueryable(QueryFault::OpaqueComparison));
    }
    super::relational_codec::encode(field, value)
}
pub(super) fn value(value: &Value) -> Result<SqlValue, StorageError> {
    Ok(match value {
        Value::Null => SqlValue::String(None),
        Value::Bool(v) => (*v).into(),
        Value::String(v) => v.clone().into(),
        Value::Number(v) => {
            if let Some(n) = v.as_i64() {
                n.into()
            } else if v.is_u64() {
                return Err(StorageError::Unqueryable(QueryFault::IntegerRange));
            } else {
                v.as_f64().ok_or(StorageError::Unqueryable(QueryFault::NonFinite))?.into()
            }
        }
        Value::Array(_) | Value::Object(_) => return Err(StorageError::Unqueryable(QueryFault::NonScalar)),
    })
}

pub(super) fn condition(
    schema: &CollectionSchema,
    query: &StorageQuery,
) -> Result<Condition, StorageError> {
    let table = Alias::new(naming::to_table_name(&schema.collection));
    let mut all = Condition::all();
    if let Some(filters) = &query.filter {
        // Deterministic statement order keeps identical query shapes cacheable.
        let mut fields: Vec<_> = filters.iter().collect();
        fields.sort_by_key(|(field, _)| *field);
        for (field, filter) in fields {
            all = all.add(field_condition(
                table.clone(),
                schema.fields.iter().find(|declared| naming::to_snake_case(&declared.name) == naming::to_snake_case(field))
                    .ok_or_else(|| StorageError::Undeclared { collection: schema.collection.clone().into(), field: field.clone().into() })?,
                filter,
            )?);
        }
    }
    if let Some(cursor) = &query.cursor {
        let field = schema
            .fields
            .iter()
            .find(|field| {
                naming::to_snake_case(&field.name) == naming::to_snake_case(&cursor.field)
            })
            .ok_or_else(|| StorageError::Undeclared { collection: schema.collection.clone().into(), field: cursor.field.clone().into() })?;
        let unique = field.name == "id"
            || field.unique
            || schema.indexes.iter().any(|index| {
                index.unique
                    && index.fields.len() == 1
                    && naming::to_snake_case(&index.fields[0]) == naming::to_snake_case(&field.name)
            });
        if !unique || field.nullable || cursor.value.is_null() {
            return Err(StorageError::Unqueryable(QueryFault::CursorNotUnique));
        }
        if query.sort.as_ref().is_some_and(|sort| {
            sort.len() != 1
                || naming::to_snake_case(&sort[0].field) != naming::to_snake_case(&cursor.field)
        }) {
            return Err(StorageError::Unqueryable(QueryFault::CursorOrder));
        }
        let col = Expr::col((table, column(schema, &cursor.field)?));
        // Before/after follow the declared traversal order, not numeric magnitude.
        let descending = query.sort.as_ref().and_then(|sort| sort.first())
            .is_some_and(|sort| sort.direction == SortDirection::Desc);
        all = all.add(match (cursor.direction, descending) {
            (CursorDirection::After, false) | (CursorDirection::Before, true) => col.gt(bound_value(field, &cursor.value)?),
            (CursorDirection::Before, false) | (CursorDirection::After, true) => col.lt(bound_value(field, &cursor.value)?),
        });
    }
    if query.tags.is_some() {
        return Err(StorageError::Unqueryable(QueryFault::Tags));
    }
    if let Some(range) = &query.time_range {
        let col = || {
            Expr::col((
                Alias::new(naming::to_table_name(&schema.collection)),
                Alias::new("created_at"),
            ))
        };
        if let Some(start) = &range.start {
            all = all.add(col().gte(start.as_str()));
        }
        if let Some(end) = &range.end {
            all = all.add(col().lte(end.as_str()));
        }
    }
    Ok(all)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Selection {
    Page,
    Stream,
    Count,
}

pub(super) fn select(
    schema: &CollectionSchema,
    query: &StorageQuery,
    selection: Selection,
) -> Result<SelectStatement, StorageError> {
    if selection == Selection::Page && query.limit.is_none() {
        return Err(StorageError::Unqueryable(QueryFault::Unbounded));
    }
    if query.joins.as_ref().is_some_and(|joins| !joins.is_empty()) {
        return Err(StorageError::Unqueryable(QueryFault::Join));
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
        // A native payload is opened only by its consumer through the binary interface;
        // the default projection carries the record's scalars, never its payload bodies.
        None => schema.fields.iter()
            .filter(|f| f.field_type != FieldType::Binary)
            .map(|f| f.name.clone()).collect(),
    };
    if fields.is_empty() {
        return Err(StorageError::Unqueryable(QueryFault::EmptyProjection));
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
        statement.limit(limit.try_into().map_err(|_| StorageError::Overflow { what: "a query limit" })?);
    }
    if let Some(offset) = query.offset {
        statement.offset(offset.try_into().map_err(|_| StorageError::Overflow { what: "a query offset" })?);
    }
    Ok(statement)
}

fn field_condition(table: Alias, field: &SchemaField, filter: &FieldFilter) -> Result<Condition, StorageError> {
    let col = Expr::col((table.clone(), Alias::new(naming::to_snake_case(&field.name))));
    let expr: SimpleExpr = match filter {
        FieldFilter::Value(v) | FieldFilter::Operator(QueryOperator::Eq(v)) if v.is_null() => {
            col.is_null()
        }
        FieldFilter::Value(v) | FieldFilter::Operator(QueryOperator::Eq(v)) => col.eq(bound_value(field, v)?),
        FieldFilter::Operator(op) => match op {
            QueryOperator::And(operators) => {
                let mut conjunction = Condition::all();
                for operator in operators {
                    conjunction = conjunction.add(field_condition(
                        table.clone(),
                        field,
                        &FieldFilter::Operator(operator.clone()),
                    )?);
                }
                return Ok(conjunction);
            }
            QueryOperator::Eq(v) => col.eq(bound_value(field, v)?),
            QueryOperator::Ne(v) if v.is_null() => col.is_not_null(),
            QueryOperator::Ne(v) => col.ne(bound_value(field, v)?),
            QueryOperator::Gt(v) => col.gt(bound_value(field, v)?),
            QueryOperator::Gte(v) => col.gte(bound_value(field, v)?),
            QueryOperator::Lt(v) => col.lt(bound_value(field, v)?),
            QueryOperator::Lte(v) => col.lte(bound_value(field, v)?),
            QueryOperator::In(v) => col.is_in(v.iter().map(|value| bound_value(field, value)).collect::<Result<Vec<_>, _>>()?),
            QueryOperator::NotIn(v) => {
                col.is_not_in(v.iter().map(|value| bound_value(field, value)).collect::<Result<Vec<_>, _>>()?)
            }
            QueryOperator::Exists(true) | QueryOperator::IsNotNull => col.is_not_null(),
            QueryOperator::Exists(false) | QueryOperator::IsNull => col.is_null(),
            QueryOperator::Contains(text) => {
                if field.field_type != FieldType::String { return Err(StorageError::Unqueryable(QueryFault::ContainsNotText)); }
                // Escape LIKE wildcards: contains is literal, not a pattern language.
                let escaped = text
                    .replace('!', "!!")
                    .replace('%', "!%")
                    .replace('_', "!_");
                col.like(sea_orm::sea_query::LikeExpr::new(format!("%{escaped}%")).escape('!'))
            }
            QueryOperator::Regex(_) => return Err(StorageError::Unqueryable(QueryFault::Regex)),
        },
    };
    Ok(Condition::all().add(expr))
}
