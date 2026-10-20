use super::*;

/// Build WHERE clause with $N placeholders and collect parameter values.
/// When `col_types` is provided, uses type-aware coercion to match actual Postgres
/// column types — prevents WrongType errors when auto-created columns have unexpected types.
pub(super) fn build_where_clause(
    filter: &Option<HashMap<String, FieldFilter>>,
    param_offset: usize,
    col_types: &HashMap<String, String>,
) -> (String, Vec<Box<dyn ToSql + Sync + Send>>) {
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn ToSql + Sync + Send>> = Vec::new();
    let mut idx = param_offset;

    // Helper: coerce value using column type if known
    let coerce = |v: &Value, column: &str| -> Box<dyn ToSql + Sync + Send> {
        let pg_type = col_types.get(column).map(|s| s.as_str());
        value_to_pg_typed(v, pg_type)
    };

    if let Some(filters) = filter {
        for (field, filter) in filters {
            let column = naming::to_snake_case(field);
            match filter {
                FieldFilter::Value(v) => {
                    if v.is_null() {
                        conditions.push(format!("{} IS NULL", column));
                    } else {
                        idx += 1;
                        conditions.push(format!("{} = ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                }
                FieldFilter::Operator(op) => match op {
                    QueryOperator::Eq(v) => {
                        idx += 1;
                        conditions.push(format!("{} = ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::Ne(v) => {
                        idx += 1;
                        conditions.push(format!("{} != ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::Gt(v) => {
                        idx += 1;
                        conditions.push(format!("{} > ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::Gte(v) => {
                        idx += 1;
                        conditions.push(format!("{} >= ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::Lt(v) => {
                        idx += 1;
                        conditions.push(format!("{} < ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::Lte(v) => {
                        idx += 1;
                        conditions.push(format!("{} <= ${}", column, idx));
                        params.push(coerce(v, &column));
                    }
                    QueryOperator::In(values) => {
                        let placeholders: Vec<String> = values
                            .iter()
                            .map(|v| {
                                idx += 1;
                                params.push(coerce(v, &column));
                                format!("${}", idx)
                            })
                            .collect();
                        conditions.push(format!("{} IN ({})", column, placeholders.join(", ")));
                    }
                    QueryOperator::NotIn(values) => {
                        let placeholders: Vec<String> = values
                            .iter()
                            .map(|v| {
                                idx += 1;
                                params.push(coerce(v, &column));
                                format!("${}", idx)
                            })
                            .collect();
                        conditions.push(format!("{} NOT IN ({})", column, placeholders.join(", ")));
                    }
                    QueryOperator::Exists(exists) => {
                        if *exists {
                            conditions.push(format!("{} IS NOT NULL", column));
                        } else {
                            conditions.push(format!("{} IS NULL", column));
                        }
                    }
                    QueryOperator::Regex(pattern) => {
                        idx += 1;
                        conditions.push(format!("{} ~ ${}", column, idx));
                        params.push(Box::new(pattern.clone()));
                    }
                    QueryOperator::Contains(substr) => {
                        idx += 1;
                        conditions.push(format!("{} ILIKE ${}", column, idx));
                        params.push(Box::new(format!("%{}%", substr)));
                    }
                    QueryOperator::IsNull => {
                        conditions.push(format!("{} IS NULL", column));
                    }
                    QueryOperator::IsNotNull => {
                        conditions.push(format!("{} IS NOT NULL", column));
                    }
                },
            }
        }
    }

    if conditions.is_empty() {
        (String::new(), params)
    } else {
        (format!("WHERE {}", conditions.join(" AND ")), params)
    }
}

/// Build ORDER BY clause
/// Build SELECT clause from optional column projection.
/// Converts camelCase field names to snake_case for SQL.
/// Always includes id, created_at, updated_at, version (metadata columns).
pub(super) fn build_select_clause(select: &Option<Vec<String>>) -> String {
    match select {
        Some(cols) if !cols.is_empty() => {
            let mut selected: Vec<String> = vec![
                "id".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
                "version".to_string(),
            ];
            for col in cols {
                let snake = naming::to_snake_case(col);
                if snake != "id"
                    && snake != "created_at"
                    && snake != "updated_at"
                    && snake != "version"
                {
                    selected.push(snake);
                }
            }
            selected.join(", ")
        }
        _ => "*".to_string(),
    }
}

pub(super) fn build_order_clause(sort: &Option<Vec<super::super::query::SortSpec>>) -> String {
    if let Some(sorts) = sort {
        if !sorts.is_empty() {
            let parts: Vec<_> = sorts
                .iter()
                .map(|s| {
                    let dir = match s.direction {
                        SortDirection::Asc => "ASC",
                        SortDirection::Desc => "DESC",
                    };
                    format!("{} {}", naming::to_snake_case(&s.field), dir)
                })
                .collect();
            return format!("ORDER BY {}", parts.join(", "));
        }
    }
    String::new()
}

/// Convert a tokio_postgres Row to a DataRecord
pub(super) fn row_to_record(
    row: &tokio_postgres::Row,
    collection: &str,
    columns: &[tokio_postgres::Column],
) -> Result<DataRecord, String> {
    let mut data = serde_json::Map::new();
    let mut id: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut version: Option<u32> = None;

    for (i, col) in columns.iter().enumerate() {
        let col_name = col.name();
        let pg_type = col.type_();

        let value: Value = match pg_type {
            &tokio_postgres::types::Type::BOOL => match row.try_get::<_, Option<bool>>(i) {
                Ok(Some(b)) => json!(b),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::INT2 => match row.try_get::<_, Option<i16>>(i) {
                Ok(Some(n)) => json!(n),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::INT4 => match row.try_get::<_, Option<i32>>(i) {
                Ok(Some(n)) => json!(n),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::INT8 => match row.try_get::<_, Option<i64>>(i) {
                Ok(Some(n)) => json!(n),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::FLOAT4 => match row.try_get::<_, Option<f32>>(i) {
                Ok(Some(n)) => json!(n),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::FLOAT8 => match row.try_get::<_, Option<f64>>(i) {
                Ok(Some(n)) => json!(n),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            },
            &tokio_postgres::types::Type::JSONB | &tokio_postgres::types::Type::JSON => {
                match row.try_get::<_, Option<Json<Value>>>(i) {
                    Ok(Some(j)) => j.0,
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::TIMESTAMPTZ => {
                match row.try_get::<_, Option<DateTime<Utc>>>(i) {
                    Ok(Some(dt)) => json!(dt.to_rfc3339()),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::TIMESTAMP => {
                match row.try_get::<_, Option<chrono::NaiveDateTime>>(i) {
                    Ok(Some(ndt)) => {
                        let dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
                        json!(dt.to_rfc3339())
                    }
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            _ => {
                // Default: read as text — no speculative JSON parsing.
                // JSONB/JSON columns are already handled above with proper typed extraction.
                // TEXT that happens to look like JSON stays as a string (the schema said TEXT).
                match row.try_get::<_, Option<String>>(i) {
                    Ok(Some(s)) => json!(s),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
        };

        let camel_col = naming::to_camel_case(col_name);
        match col_name {
            "id" => id = value.as_str().map(|s| s.to_string()),
            "created_at" => created_at = value.as_str().map(|s| s.to_string()),
            "updated_at" => updated_at = value.as_str().map(|s| s.to_string()),
            "version" => {
                version = value
                    .as_u64()
                    .map(|n| n as u32)
                    .or(value.as_i64().map(|n| n as u32))
            }
            _ => {
                data.insert(camel_col, value);
            }
        }
    }

    // Include base fields in data for TypeScript compatibility
    if let Some(ref id_str) = id {
        data.insert("id".to_string(), json!(id_str));
    }
    if let Some(ref ts) = created_at {
        data.insert("createdAt".to_string(), json!(ts));
    }
    if let Some(ref ts) = updated_at {
        data.insert("updatedAt".to_string(), json!(ts));
    }
    if let Some(v) = version {
        data.insert("version".to_string(), json!(v));
    }

    Ok(DataRecord {
        id: id.unwrap_or_default(),
        collection: collection.to_string(),
        data: Value::Object(data),
        metadata: RecordMetadata {
            created_at: created_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            updated_at: updated_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            version: version.unwrap_or(1),
            tags: None,
            schema: None,
            ttl: None,
        },
    })
}
