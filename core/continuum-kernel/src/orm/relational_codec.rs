//! Decode only projected columns, using the entity's declared codecs.
//! QueryResult owns its SQLite row; decoding happens after the pooled connection returns.
use super::{
    adapter::naming,
    types::{CollectionSchema, DataRecord, FieldType, RecordMetadata, SchemaField},
};
use sea_orm::sqlx::{Row, TypeInfo, ValueRef};
use sea_orm::{sea_query::Value as SqlValue, QueryResult};
use serde_json::{Map, Number, Value};

/// Bind producer-owned text directly. The SQL driver owns the one required copy;
/// callers need not allocate an intermediate JSON string to reach this codec.
pub(super) fn encode_text(field: &SchemaField, value: &str) -> Result<SqlValue, String> {
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => Ok(value.into()),
        FieldType::Json => serde_json::to_string(value)
            .map(Into::into)
            .map_err(|e| e.to_string()),
        _ => Err(format!("{} does not accept text", field.name)),
    }
}

/// Metadata versions already have an exact integer representation.
pub(super) fn encode_integer(field: &SchemaField, value: i64) -> Result<SqlValue, String> {
    match field.field_type {
        FieldType::Number => Ok(value.into()),
        FieldType::Json => serde_json::to_string(&value)
            .map(Into::into)
            .map_err(|e| e.to_string()),
        _ => Err(format!("{} does not accept an integer", field.name)),
    }
}

pub(super) fn encode(field: &SchemaField, value: &Value) -> Result<SqlValue, String> {
    if value.is_null() {
        return if field.nullable {
            Ok(SqlValue::String(None))
        } else {
            Err(format!("{} is not nullable", field.name))
        };
    }
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => encode_text(
            field,
            value.as_str().ok_or_else(|| format!("{} requires a string", field.name))?,
        ),
        FieldType::Boolean => value
            .as_bool()
            .map(Into::into)
            .ok_or_else(|| format!("{} requires a boolean", field.name)),
        FieldType::Number if value.is_number() => super::relational_query::value(value),
        FieldType::Number => Err(format!("{} requires a number", field.name)),
        FieldType::Binary => Err("binary payload requires the native typed storage interface".into()),
        FieldType::Json => serde_json::to_string(value)
            .map(Into::into)
            .map_err(|e| e.to_string()),
    }
}

pub(super) fn scalar(
    row: &sea_orm::sqlx::sqlite::SqliteRow,
    index: usize,
    field: &SchemaField,
) -> Result<Value, String> {
    let raw = row.try_get_raw(index).map_err(|e| e.to_string())?;
    if raw.is_null() {
        return if field.nullable {
            Ok(Value::Null)
        } else {
            Err(format!("{} contains NULL but is not nullable", field.name))
        };
    }
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => row
            .try_get::<String, _>(index)
            .map(Value::String)
            .map_err(|e| e.to_string()),
        FieldType::Binary => Err("binary payload requires the native typed storage interface".into()),
        FieldType::Json => {
            let text: String = row.try_get(index).map_err(|e| e.to_string())?;
            serde_json::from_str(&text)
                .map_err(|e| format!("{} has invalid declared JSON: {e}", field.name))
        }
        FieldType::Boolean => match row.try_get::<i64, _>(index).map_err(|e| e.to_string())? {
            0 => Ok(Value::Bool(false)),
            1 => Ok(Value::Bool(true)),
            other => Err(format!("{} has invalid boolean {other}", field.name)),
        },
        FieldType::Number => match raw.type_info().name() {
            "INTEGER" => row
                .try_get::<i64, _>(index)
                .map(|v| Value::Number(v.into()))
                .map_err(|e| e.to_string()),
            "REAL" => {
                let v: f64 = row.try_get(index).map_err(|e| e.to_string())?;
                Number::from_f64(v)
                    .map(Value::Number)
                    .ok_or_else(|| format!("{} has a non-finite number", field.name))
            }
            storage => Err(format!(
                "{} requires numeric storage, found {storage}; migrate the column",
                field.name
            )),
        },
    }
}

/// Compile physical field names once per query, not once per cell in every row.
/// The schema allocation stays shared; the plan stores only names and field ordinals.
pub(super) struct RowDecoder {
    schema: std::sync::Arc<CollectionSchema>,
    fields: std::collections::HashMap<String, usize>,
    joined_columns: std::collections::HashSet<String>,
}

impl RowDecoder {
    pub(super) fn new(schema: std::sync::Arc<CollectionSchema>, joined_columns: &[String]) -> Self {
        let fields = schema.fields.iter().enumerate()
            .map(|(index, field)| (naming::to_snake_case(&field.name), index)).collect();
        Self { schema, fields, joined_columns: joined_columns.iter().cloned().collect() }
    }

    pub(super) fn decode(&self, result: QueryResult) -> Result<DataRecord, String> {
        let schema = &self.schema;
        let row = result.try_as_sqlite_row().ok_or("SQLite result required")?;
        use sea_orm::sqlx::Column;
        let mut fields = Map::new();
        for (index, col) in row.columns().iter().enumerate() {
            if self.joined_columns.contains(col.name()) {
                continue;
            }
            let field = self.fields.get(col.name()).map(|index| &schema.fields[*index])
                .ok_or_else(|| format!("{}.{} has no declared codec", schema.collection, col.name()))?;
            fields.insert(field.name.clone(), scalar(row, index, field)?);
        }
        let metadata_string = |name: &str| -> Result<String, String> {
            match fields.get(name) {
                Some(Value::String(s)) => Ok(s.clone()),
                _ => Err(format!("projection omitted required metadata {name}")),
            }
        };
        // Entity decoding consumes data, including flattened BaseEntity fields. The
        // record envelope also exposes those small values for generic storage callers.
        let id = metadata_string("id")?;
        let created_at = metadata_string("createdAt")?;
        let updated_at = metadata_string("updatedAt")?;
        let version = fields
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or("missing version")?;
        Ok(DataRecord {
            id,
            collection: schema.collection.clone(),
            data: Value::Object(fields),
            metadata: RecordMetadata {
                created_at,
                updated_at,
                version: version.try_into().map_err(|_| "version exceeds u32")?,
                tags: None,
                schema: None,
                ttl: None,
            },
        })
    }
}
